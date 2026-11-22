use std::num::{NonZeroU16, NonZeroU64};
use std::path::{Path, PathBuf};
use std::process::{self, Command, Stdio};
use std::sync::Arc;
use std::time::Duration;
use std::{env, fs};

use async_net::TcpListener;
use async_net::unix::UnixListener;
use futures::io::{AsyncReadExt, AsyncWriteExt};
use futures_rustls::rustls::crypto::ring;
use futures_rustls::rustls::{ClientConfig, RootCertStore};
use paseo_client::protocol::channels::ProcessSpawn;
use paseo_client::protocol::checkout::ForgeAuthState;
use paseo_client::protocol::config::{DaemonConfigPatch, ProjectConfigError};
use paseo_client::protocol::diff::DiffCompare;
use paseo_client::protocol::files::{FileState, FileWrite, FileWriteOutcome};
use paseo_client::protocol::hosts::{HostAdd, HostStatus};
use paseo_client::protocol::labels::{LabelColour, LabelEdit, LabelUpdate, WorkspaceLabel};
use paseo_client::protocol::providers::RecentSessionsQuery;
use paseo_client::protocol::review::FileReference;
use paseo_client::protocol::schedules::{
    ScheduleAgentConfig, ScheduleCadence, ScheduleCreate, ScheduleStatus, ScheduleTarget,
    ScheduleUpdate,
};
use paseo_client::protocol::terminals::TerminalSize;
use paseo_client::protocol::workspaces::{EntryKind, WorkspaceDirectoryQuery, WorkspaceRecovery};
use paseo_client::{
    ConnectOptions, ConnectTarget, EventKind, PaseoClient, PaseoError, RequestFailure,
    SessionEvent, StatusEvent,
};
use serde_json::{Map, json};

const PORT_VARIABLE: &str = "PASEO_TEST_DAEMON_PORT";

fn tls() -> Arc<ClientConfig> {
    let provider = Arc::new(ring::default_provider());
    Arc::new(
        ClientConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()
            .expect("protocol versions")
            .with_root_certificates(RootCertStore::empty())
            .with_no_client_auth(),
    )
}

async fn client() -> PaseoClient {
    let port = env::var(PORT_VARIABLE).expect("PASEO_TEST_DAEMON_PORT names an isolated daemon");
    let target = format!("127.0.0.1:{port}")
        .parse::<ConnectTarget>()
        .expect("target");
    let client = PaseoClient::connect(
        &target,
        &ConnectOptions::new("paseo-client-daemon-test", tls()),
    )
    .await
    .expect("connect");
    smol::spawn({
        let client = client.clone();
        async move { client.run().await }
    })
    .detach();
    client
}

fn terminal_size(rows: u16, cols: u16) -> TerminalSize {
    TerminalSize::new(
        NonZeroU16::new(rows).expect("rows"),
        NonZeroU16::new(cols).expect("cols"),
    )
}

fn is_alive(pid: u64) -> bool {
    Command::new("kill")
        .args(["-0", &pid.to_string()])
        .stderr(Stdio::null())
        .status()
        .expect("kill -0")
        .success()
}

#[test]
#[ignore = "needs an isolated fork daemon named by PASEO_TEST_DAEMON_PORT"]
fn a_large_binary_payload_round_trips_through_cat() {
    smol::block_on(async {
        let client = client().await;
        let parts = client
            .spawn_process(&ProcessSpawn::new("cat", "/tmp"))
            .await
            .expect("spawn")
            .into_parts();
        let (mut input, mut output) = (parts.input, parts.output);
        let payload = (0..4 * 1024 * 1024u32)
            .map(|index| u8::try_from(index % 256).expect("a residue below 256 fits a byte"))
            .collect::<Vec<_>>();
        let writer = smol::spawn({
            let payload = payload.clone();
            async move {
                input.write_all(&payload).await.expect("write");
                input.close().await.expect("close");
            }
        });
        let mut echoed = Vec::new();
        output.read_to_end(&mut echoed).await.expect("read");
        writer.await;
        assert!(echoed == payload, "the echoed bytes differ");
        let exit = parts.process.exited().await.expect("exit");
        assert_eq!((exit.exit_code(), exit.signal()), (Some(0), None));
    });
}

#[test]
#[ignore = "needs an isolated fork daemon named by PASEO_TEST_DAEMON_PORT"]
fn stderr_stays_separate_and_the_exit_code_is_reported() {
    smol::block_on(async {
        let client = client().await;
        let spawn = ProcessSpawn::new("sh", "/tmp")
            .with_arg("-c")
            .with_arg("printf out; printf err >&2; exit 7");
        let mut parts = client
            .spawn_process(&spawn)
            .await
            .expect("spawn")
            .into_parts();
        let (mut stdout, mut stderr) = (Vec::new(), Vec::new());
        parts.output.read_to_end(&mut stdout).await.expect("stdout");
        parts
            .error_output
            .as_mut()
            .expect("stderr")
            .read_to_end(&mut stderr)
            .await
            .expect("stderr");
        assert_eq!(
            (stdout.as_slice(), stderr.as_slice()),
            (&b"out"[..], &b"err"[..])
        );
        assert_eq!(
            parts.process.exited().await.expect("exit").exit_code(),
            Some(7)
        );
    });
}

#[test]
#[ignore = "needs an isolated fork daemon named by PASEO_TEST_DAEMON_PORT"]
fn reading_only_stdout_is_not_stalled_by_a_large_stderr() {
    smol::block_on(async {
        let client = client().await;
        let spawn = ProcessSpawn::new("sh", "/tmp")
            .with_arg("-c")
            .with_arg("head -c 2000000 /dev/zero >&2; printf done");
        let mut parts = client
            .spawn_process(&spawn)
            .await
            .expect("spawn")
            .into_parts();
        drop(parts.error_output.take());
        let mut stdout = Vec::new();
        parts.output.read_to_end(&mut stdout).await.expect("stdout");
        assert_eq!(stdout, b"done");
        assert_eq!(
            parts.process.exited().await.expect("exit").exit_code(),
            Some(0)
        );
    });
}

#[test]
#[ignore = "needs an isolated fork daemon named by PASEO_TEST_DAEMON_PORT"]
fn a_pseudo_terminal_round_trips_input_and_applies_a_resize() {
    smol::block_on(async {
        let client = client().await;
        let spawn = ProcessSpawn::new("sh", "/tmp")
            .with_arg("-c")
            .with_arg("head -c 4 >/dev/null; stty size")
            .with_pty(terminal_size(24, 80));
        let parts = client
            .spawn_process(&spawn)
            .await
            .expect("spawn")
            .into_parts();
        let (mut input, mut output) = (parts.input, parts.output);
        parts
            .process
            .resize(terminal_size(40, 120))
            .expect("resize");
        input.write_all(b"abc\n").await.expect("write");
        let mut transcript = Vec::new();
        output.read_to_end(&mut transcript).await.expect("read");
        let transcript = String::from_utf8_lossy(&transcript);
        assert!(transcript.contains("abc"), "{transcript:?}");
        assert!(transcript.contains("40 120"), "{transcript:?}");
        assert_eq!(
            parts.process.exited().await.expect("exit").exit_code(),
            Some(0)
        );
    });
}

#[test]
#[ignore = "needs an isolated fork daemon named by PASEO_TEST_DAEMON_PORT"]
fn a_net_channel_echoes_through_a_local_server() {
    smol::block_on(async {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let port = listener.local_addr().expect("address").port();
        let server = smol::spawn(async move {
            let (stream, _) = listener.accept().await.expect("accept");
            let (reader, mut writer) = futures::io::AsyncReadExt::split(stream);
            futures::io::copy(reader, &mut writer).await.expect("echo");
            writer.close().await.expect("close");
        });
        let client = client().await;
        let parts = client
            .connect_net("127.0.0.1", port)
            .await
            .expect("connect")
            .into_parts();
        let (mut input, mut output) = (parts.input, parts.output);
        let payload = (0..300_000u32)
            .map(|index| u8::try_from(index % 256).expect("a residue below 256 fits a byte"))
            .collect::<Vec<_>>();
        input.write_all(&payload).await.expect("write");
        input.close().await.expect("close");
        let mut echoed = Vec::new();
        output.read_to_end(&mut echoed).await.expect("read");
        assert!(echoed == payload, "the echoed bytes differ");
        assert_eq!(
            parts.connection.closed().await.expect("closed").error(),
            None
        );
        server.await;
    });
}

#[test]
#[ignore = "needs an isolated fork daemon named by PASEO_TEST_DAEMON_PORT"]
fn a_net_channel_echoes_through_a_unix_socket() {
    smol::block_on(async {
        let directory = std::env::temp_dir().join(format!("paseo-client-{}", std::process::id()));
        std::fs::create_dir_all(&directory).expect("socket directory");
        let path = directory.join("echo.sock");
        let listener = UnixListener::bind(&path).expect("bind");
        let server = smol::spawn(async move {
            let (stream, _) = listener.accept().await.expect("accept");
            let (reader, mut writer) = futures::io::AsyncReadExt::split(stream);
            futures::io::copy(reader, &mut writer).await.expect("echo");
            writer.close().await.expect("close");
        });
        let client = client().await;
        let parts = client
            .connect_unix_socket(path.to_str().expect("UTF-8 socket path"))
            .await
            .expect("connect")
            .into_parts();
        let (mut input, mut output) = (parts.input, parts.output);
        input.write_all(b"through the daemon").await.expect("write");
        input.close().await.expect("close");
        let mut echoed = Vec::new();
        output.read_to_end(&mut echoed).await.expect("read");
        assert_eq!(echoed, b"through the daemon");
        server.await;
        std::fs::remove_dir_all(&directory).expect("remove socket directory");
    });
}

#[test]
#[ignore = "needs an isolated fork daemon named by PASEO_TEST_DAEMON_PORT"]
fn dropping_a_process_channel_kills_the_process() {
    smol::block_on(async {
        let client = client().await;
        let process = client
            .spawn_process(&ProcessSpawn::new("sleep", "/tmp").with_arg("30"))
            .await
            .expect("spawn");
        let pid = process.pid();
        assert!(is_alive(pid));
        drop(process);
        for _ in 0..100 {
            if !is_alive(pid) {
                return;
            }
            smol::Timer::after(Duration::from_millis(50)).await;
        }
        panic!("process {pid} outlived its channel");
    });
}

#[test]
#[ignore = "needs an isolated fork daemon named by PASEO_TEST_DAEMON_PORT"]
fn agent_review_requests_for_an_unknown_agent_are_rejected() {
    smol::block_on(async {
        let client = client().await;
        let subscription = client.observe_agent_review("no-such-agent").await;
        assert!(
            matches!(subscription, Err(PaseoError::Rpc(_))),
            "{:?}",
            subscription.err()
        );
        let unseen = client
            .mark_review_unseen(
                "no-such-agent",
                &[FileReference::new(
                    "/repo".to_owned(),
                    "/repo/a.rs".to_owned(),
                )],
            )
            .await;
        assert!(matches!(unseen, Err(PaseoError::Rpc(_))), "{unseen:?}");
    });
}

#[test]
#[ignore = "needs an isolated paseo daemon; see PASEO_TEST_DAEMON_PORT"]
fn a_schedule_is_created_paused_resumed_and_deleted() {
    smol::block_on(async {
        let client = client().await;
        let cwd = env::temp_dir();
        let created = client
            .create_schedule(
                &ScheduleCreate::new(
                    "Say hello",
                    ScheduleCadence::Every {
                        every_ms: 24 * 60 * 60 * 1000,
                    },
                    ScheduleTarget::NewAgent(ScheduleAgentConfig::new(
                        "mock",
                        cwd.to_string_lossy(),
                    )),
                )
                .with_name("paseo-client-test")
                .with_run_on_create(false),
            )
            .await
            .expect("create");
        assert!(
            client
                .list_schedules()
                .await
                .expect("list")
                .iter()
                .any(|schedule| schedule.id() == created.id())
        );
        let paused = client.pause_schedule(created.id()).await.expect("pause");
        assert_eq!(paused.status(), ScheduleStatus::Paused);
        let resumed = client.resume_schedule(created.id()).await.expect("resume");
        assert_eq!(resumed.status(), ScheduleStatus::Active);
        assert!(
            client
                .schedule_runs(created.id())
                .await
                .expect("runs")
                .is_empty()
        );
        client.delete_schedule(created.id()).await.expect("delete");
    });
}

#[test]
#[ignore = "needs an isolated paseo daemon; see PASEO_TEST_DAEMON_PORT"]
fn checkout_requests_outside_a_repository_are_rejected() {
    smol::block_on(async {
        let client = client().await;
        let cwd = env::temp_dir();
        let cwd = cwd.to_string_lossy();
        assert!(client.checkout_push(&cwd).await.is_err());
        let status = client.pull_request_status(&cwd).await.expect("status");
        assert!(status.pull_request().is_none());
        assert_eq!(status.auth_state(), ForgeAuthState::NoRemote);
    });
}

fn scratch(name: &str) -> PathBuf {
    let path = env::temp_dir().join(format!("paseo-client-{name}-{}", process::id()));
    if path.exists() {
        fs::remove_dir_all(&path).expect("clear scratch directory");
    }
    fs::create_dir_all(&path).expect("scratch directory");
    path.canonicalize().expect("canonical scratch directory")
}

fn git(repo: &Path, args: &[&str]) {
    let status = Command::new("git")
        .args([
            "-c",
            "user.name=Paseo Test",
            "-c",
            "user.email=test@paseo.invalid",
        ])
        .args(args)
        .current_dir(repo)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .expect("git");
    assert!(status.success(), "git {args:?}");
}

fn repository(name: &str) -> PathBuf {
    let repo = scratch(name);
    git(&repo, &["init", "-q", "-b", "main"]);
    fs::write(repo.join("README.md"), "hello\n").expect("readme");
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "initial"]);
    repo
}

async fn next<T>(updates: &flume::Receiver<Result<T, PaseoError>>) -> T {
    smol::future::or(
        async {
            updates
                .recv_async()
                .await
                .expect("update")
                .expect("decoded")
        },
        async {
            smol::Timer::after(Duration::from_secs(10)).await;
            panic!("no update within 10s");
        },
    )
    .await
}

#[test]
#[ignore = "needs an isolated fork daemon named by PASEO_TEST_DAEMON_PORT"]
fn files_are_created_streamed_watched_renamed_and_deleted() {
    smol::block_on(async {
        let client = client().await;
        let root = scratch("files");
        let cwd = root.to_string_lossy();
        let written = client
            .write_text_file(&cwd, "notes", "a.txt", "first")
            .await
            .expect("write");
        assert_eq!(written, "notes/a.txt");
        let listing = client.list_directory(&cwd, "notes").await.expect("list");
        assert_eq!(
            listing
                .entries()
                .iter()
                .map(|entry| (entry.name(), entry.kind(), entry.size()))
                .collect::<Vec<_>>(),
            [("a.txt", EntryKind::File, 5)]
        );

        let payload = (0..3 * 1024 * 1024u32)
            .map(|index| u8::try_from(index % 251).expect("a residue below 251 fits a byte"))
            .collect::<Vec<_>>();
        fs::write(root.join("blob.bin"), &payload).expect("blob");
        let read = client
            .read_file(&cwd, "blob.bin", None)
            .await
            .expect("read");
        assert!(read.bytes() == payload, "the streamed bytes differ");
        assert_eq!(
            read.size(),
            u64::try_from(payload.len()).expect("the payload length fits in 64 bits")
        );
        let refused = client
            .read_file(&cwd, "blob.bin", NonZeroU64::new(1024))
            .await;
        assert!(matches!(refused, Err(PaseoError::Rpc(_))), "{refused:?}");

        let watched = client
            .observe_file(&cwd, "notes/a.txt")
            .await
            .expect("observe");
        let FileState::Ready(ready) = watched.snapshot().state() else {
            panic!("expected a ready file, got {:?}", watched.snapshot());
        };
        let outcome = client
            .write_file(
                &cwd,
                &FileWrite::text("notes/a.txt", "second", ready.revision()),
            )
            .await
            .expect("write file");
        assert!(
            matches!(&outcome, FileWriteOutcome::Written(written) if written.size() == 6),
            "{outcome:?}"
        );
        let stale = client
            .write_file(
                &cwd,
                &FileWrite::text("notes/a.txt", "third", ready.revision()),
            )
            .await
            .expect("stale write");
        assert!(
            matches!(stale, FileWriteOutcome::Conflict { .. }),
            "{stale:?}"
        );
        let update = next(watched.updates()).await;
        assert!(
            matches!(update.state(), FileState::Ready(ready) if ready.size() == 6),
            "{update:?}"
        );

        let renamed = client
            .rename_file_entry(&cwd, "notes/a.txt", "b.txt")
            .await
            .expect("rename");
        assert_eq!(renamed, "notes/b.txt");
        let duplicate = client
            .duplicate_file_entry(&cwd, "notes/b.txt")
            .await
            .expect("duplicate");
        assert_eq!(
            fs::read_to_string(root.join(&duplicate)).expect("duplicate contents"),
            "second"
        );
        let token = client
            .file_download_token(&cwd, "notes/b.txt")
            .await
            .expect("download token");
        assert!(!token.token().is_empty());
        assert_eq!(token.size(), Some(6));
        client
            .delete_file_entry(&cwd, "notes/b.txt")
            .await
            .expect("delete");
        assert!(!root.join("notes/b.txt").exists());
    });
}

#[test]
#[ignore = "needs an isolated fork daemon named by PASEO_TEST_DAEMON_PORT"]
fn a_project_is_renamed_configured_and_removed() {
    smol::block_on(async {
        let client = client().await;
        let repo = repository("project");
        let root = repo.to_string_lossy();
        let project = client.add_project(&root).await.expect("add");
        let renamed = client
            .rename_project(project.project_id(), Some("Renamed"))
            .await
            .expect("rename");
        assert_eq!(renamed.as_deref(), Some("Renamed"));
        assert!(
            client
                .list_projects()
                .await
                .expect("projects")
                .iter()
                .any(|listed| listed.project_id() == project.project_id()
                    && listed.project_custom_name() == Some("Renamed"))
        );

        let absent = client.read_project_config(&root).await.expect("read");
        assert!(absent.config().is_none() && absent.revision().is_none());
        let mut config = Map::new();
        config.insert("scripts".to_owned(), json!({"dev": {"command": "true"}}));
        let written = client
            .write_project_config(&root, &config, None)
            .await
            .expect("write config");
        assert_eq!(written.config(), Some(&config));
        let stale = client.write_project_config(&root, &config, None).await;
        assert!(
            matches!(
                stale,
                Err(PaseoError::Request {
                    failure: RequestFailure::ProjectConfig(
                        ProjectConfigError::StaleProjectConfig {
                            current_revision: Some(_)
                        }
                    ),
                    ..
                })
            ),
            "{stale:?}"
        );
        let reread = client.read_project_config(&root).await.expect("reread");
        assert_eq!(reread.revision(), written.revision());

        client
            .remove_project(project.project_id())
            .await
            .expect("remove");
        assert!(
            !client
                .list_projects()
                .await
                .expect("projects")
                .iter()
                .any(|listed| listed.project_id() == project.project_id())
        );
    });
}

#[test]
#[ignore = "needs an isolated fork daemon named by PASEO_TEST_DAEMON_PORT"]
fn workspace_labels_are_assigned_observed_and_deleted() {
    smol::block_on(async {
        let client = client().await;
        let directory = scratch("labels");
        let workspace = client
            .open_project(&directory.to_string_lossy())
            .await
            .expect("open");
        let name = format!("paseo-client-{}", process::id());
        let labels = client
            .observe_workspace_labels(None)
            .await
            .expect("observe labels");
        let assigned = client
            .set_workspace_label(
                workspace.id(),
                &WorkspaceLabel::new(name.as_str(), LabelColour::Teal),
                true,
            )
            .await
            .expect("assign");
        assert!(assigned.workspace_labels().contains(&name));
        let LabelUpdate::Upsert(upserted) = next(labels.updates()).await else {
            panic!("expected an upsert");
        };
        assert_eq!(upserted.label().colour(), LabelColour::Teal);
        let edited = client
            .edit_workspace_label(&LabelEdit::new(name.as_str()).with_colour(LabelColour::Amber))
            .await
            .expect("edit");
        assert_eq!(edited.label().colour(), LabelColour::Amber);
        assert_eq!(
            client.inspect_label_deletion(&name).await.expect("inspect"),
            1
        );
        let pinned = client
            .set_workspace_pinned(workspace.id(), true)
            .await
            .expect("pin");
        assert!(pinned.is_some());
        let listed = client
            .fetch_workspaces(
                &WorkspaceDirectoryQuery::default().with_project_id(workspace.project_id()),
            )
            .await
            .expect("workspaces");
        let listed = listed
            .entries()
            .iter()
            .find(|listed| listed.id() == workspace.id())
            .expect("listed workspace");
        assert!(listed.labels().contains(&name) && listed.pinned_at().is_some());
        assert!(listed.created_at().is_some());
        assert_eq!(
            client.delete_workspace_label(&name).await.expect("delete"),
            1
        );
        let catalogue = client.list_workspace_labels().await.expect("labels");
        assert!(!catalogue.labels().iter().any(|label| label.name() == name));
        client
            .archive_workspace(workspace.id())
            .await
            .expect("archive");
        client
            .remove_project(workspace.project_id())
            .await
            .expect("remove project");
    });
}

#[test]
#[ignore = "needs an isolated fork daemon named by PASEO_TEST_DAEMON_PORT"]
fn a_repository_is_committed_stashed_and_branched() {
    smol::block_on(async {
        let client = client().await;
        let repo = repository("checkout");
        let cwd = repo.to_string_lossy();
        fs::write(repo.join("README.md"), "hello\nworld\n").expect("edit");
        let diff = client
            .checkout_diff(&cwd, &DiffCompare::uncommitted())
            .await
            .expect("diff");
        assert_eq!(
            diff.files()
                .iter()
                .map(|file| (file.path(), file.additions()))
                .collect::<Vec<_>>(),
            [("README.md", 1)]
        );
        client
            .checkout_commit(&cwd, Some("second"), true)
            .await
            .expect("commit");
        let status = client.checkout_status(&cwd).await.expect("status");
        assert!(!status.repository().expect("git").is_dirty());

        fs::write(repo.join("README.md"), "changed\n").expect("edit");
        client
            .stash_save(&cwd, Some("main"))
            .await
            .expect("stash save");
        let stashes = client.list_stashes(&cwd, true).await.expect("stashes");
        assert_eq!(stashes.len(), 1);
        assert_eq!(stashes[0].branch(), Some("main"));
        client
            .stash_pop(&cwd, stashes[0].index())
            .await
            .expect("stash pop");
        client
            .discard_changes(&cwd, &["README.md".to_owned()])
            .await
            .expect("discard");
        assert_eq!(
            fs::read_to_string(repo.join("README.md")).expect("readme"),
            "hello\nworld\n"
        );

        git(&repo, &["branch", "feature"]);
        let validation = client
            .validate_branch(&cwd, "feature")
            .await
            .expect("validate");
        assert!(validation.exists() && !validation.is_remote());
        assert!(
            !client
                .validate_branch(&cwd, "no-such-branch")
                .await
                .expect("validate missing")
                .exists()
        );
        client.switch_branch(&cwd, "feature").await.expect("switch");
        let renamed = client.rename_branch(&cwd, "renamed").await.expect("rename");
        assert_eq!(renamed.as_deref(), Some("renamed"));
        let commits = client.list_commits(&cwd).await.expect("commits");
        assert_eq!(
            commits
                .commits()
                .iter()
                .map(|commit| (commit.summary().subject(), commit.summary().is_on_base()))
                .collect::<Vec<_>>(),
            [("second", Some(true)), ("initial", Some(true))]
        );
        assert_eq!(commits.base_ref(), Some("main"));
        let first = client
            .list_commit_page(&cwd, None, 1)
            .await
            .expect("first page");
        let second = client
            .list_commit_page(&cwd, first.next_cursor(), 1)
            .await
            .expect("second page");
        assert_eq!(
            [first.commits(), second.commits()]
                .concat()
                .iter()
                .map(|commit| commit.subject())
                .collect::<Vec<_>>(),
            ["second", "initial"]
        );
        assert_eq!(second.next_cursor(), None);
        let files = client
            .commit_files(&cwd, first.commits()[0].sha())
            .await
            .expect("commit files");
        assert_eq!(
            files,
            commits.commits()[0].files(),
            "a commit's files match the unpaginated list"
        );
    });
}

#[test]
#[ignore = "needs an isolated fork daemon named by PASEO_TEST_DAEMON_PORT"]
fn daemon_config_changes_are_applied_and_announced() {
    smol::block_on(async {
        let client = client().await;
        let events = client
            .observe_events(&[EventKind::DaemonConfigChanged], false)
            .await
            .expect("observe events");
        let original = client.daemon_config().await.expect("config");
        let prompt = format!("paseo-client-{}", process::id());
        let changed = client
            .set_daemon_config(&DaemonConfigPatch::default().with_append_system_prompt(&prompt))
            .await
            .expect("set config");
        assert_eq!(changed.append_system_prompt(), prompt);
        let SessionEvent::Status(StatusEvent::DaemonConfigChanged { config }) =
            next(events.updates()).await
        else {
            panic!("expected a configuration change");
        };
        assert_eq!(config.append_system_prompt(), prompt);
        client
            .set_daemon_config(
                &DaemonConfigPatch::default()
                    .with_append_system_prompt(original.append_system_prompt()),
            )
            .await
            .expect("restore config");

        let reload = client.reload_daemon_config().await.expect("reload");
        assert!(reload.restart_required_paths().is_empty(), "{reload:?}");
        let offer = client.pairing_offer().await.expect("pairing offer");
        assert_eq!(offer.url().is_some(), offer.relay_enabled(), "{offer:?}");
        assert!(
            !client
                .daemon_diagnostics()
                .await
                .expect("diagnostics")
                .is_empty()
        );
        let hosts = client.list_hosts().await.expect("hosts");
        assert_eq!(hosts.local().server_id(), client.server_info().server_id());
    });
}

#[test]
#[ignore = "needs an isolated fork daemon named by PASEO_TEST_DAEMON_PORT"]
fn a_schedule_is_inspected_and_updated() {
    smol::block_on(async {
        let client = client().await;
        let cwd = env::temp_dir();
        let created = client
            .create_schedule(
                &ScheduleCreate::new(
                    "Say hello",
                    ScheduleCadence::Every {
                        every_ms: 24 * 60 * 60 * 1000,
                    },
                    ScheduleTarget::NewAgent(ScheduleAgentConfig::new(
                        "mock",
                        cwd.to_string_lossy(),
                    )),
                )
                .with_name("paseo-client-inspect")
                .with_run_on_create(false),
            )
            .await
            .expect("create");
        let inspected = client
            .inspect_schedule(created.id())
            .await
            .expect("inspect");
        assert_eq!(inspected.schedule().id(), created.id());
        assert!(inspected.runs().is_empty());
        let updated = client
            .update_schedule(
                created.id(),
                &ScheduleUpdate::default()
                    .with_name(Some("paseo-client-updated".to_owned()))
                    .with_max_runs(Some(3)),
            )
            .await
            .expect("update");
        assert_eq!(
            (updated.schedule().name(), updated.schedule().max_runs()),
            (Some("paseo-client-updated"), Some(3))
        );
        client.delete_schedule(created.id()).await.expect("delete");
    });
}

#[test]
#[ignore = "needs an isolated fork daemon named by PASEO_TEST_DAEMON_PORT"]
fn workspace_state_and_daemon_catalogues_decode() {
    smol::block_on(async {
        let client = client().await;
        let directory = scratch("workspace-state");
        let workspace = client
            .open_project(&directory.to_string_lossy())
            .await
            .expect("open");
        let setup = client
            .observe_workspace_setup(workspace.id())
            .await
            .expect("observe setup");
        assert!(setup.snapshot().is_none(), "{:?}", setup.snapshot());
        let cleared = client
            .clear_workspace_attention(&[workspace.id().to_owned()])
            .await
            .expect("clear attention");
        assert_eq!(
            cleared
                .iter()
                .map(|cleared| (cleared.workspace_id(), cleared.success()))
                .collect::<Vec<_>>(),
            [(workspace.id(), true)]
        );
        let recovery = client
            .inspect_workspace_recovery(workspace.id())
            .await
            .expect("recovery");
        assert!(
            matches!(recovery, WorkspaceRecovery::Unavailable { ref workspace_id, .. } if workspace_id == workspace.id()),
            "{recovery:?}"
        );
        let hosts = client.observe_hosts().await.expect("observe hosts");
        assert_eq!(
            hosts.snapshot().local().server_id(),
            client.server_info().server_id()
        );
        client.skills_status().await.expect("skills status");
        client
            .recent_provider_sessions(&RecentSessionsQuery::default().with_limit(5).expect("limit"))
            .await
            .expect("recent sessions");
        client
            .archive_workspace(workspace.id())
            .await
            .expect("archive");
        client
            .remove_project(workspace.project_id())
            .await
            .expect("remove project");
    });
}

#[test]
#[ignore = "needs two isolated fork daemons named by PASEO_TEST_DAEMON_PORT and PASEO_TEST_ROUTED_DAEMON_PORT"]
fn a_connected_host_is_reached_through_a_routed_session() {
    smol::block_on(async {
        let routed_port = env::var("PASEO_TEST_ROUTED_DAEMON_PORT")
            .expect("PASEO_TEST_ROUTED_DAEMON_PORT names a second daemon");
        let client = client().await;
        let hosts = client.observe_hosts().await.expect("observe hosts");
        let added = client
            .add_host(&HostAdd::new(format!("127.0.0.1:{routed_port}")).with_label("routed"))
            .await
            .expect("add host");
        let server_id = loop {
            let directory = next(hosts.updates()).await;
            let host = directory
                .hosts()
                .iter()
                .find(|host| host.registration_id() == added.registration_id())
                .expect("added host");
            if host.status() == HostStatus::Online {
                break host
                    .server_id()
                    .expect("online host has a server id")
                    .to_owned();
            }
        };
        let options = ConnectOptions::new("paseo-client-routed-test", tls());
        let routed = client
            .connect_host(&server_id, &options)
            .await
            .expect("connect host");
        smol::spawn({
            let routed = routed.clone();
            async move { routed.run().await }
        })
        .detach();
        assert_eq!(routed.server_info().server_id(), server_id);
        assert_ne!(server_id, client.server_info().server_id());
        assert!(routed.security_mode().is_none());
        let root = scratch("routed");
        let payload = (0..512 * 1024u32)
            .map(|index| u8::try_from(index % 241).expect("a residue below 241 fits a byte"))
            .collect::<Vec<_>>();
        fs::write(root.join("blob.bin"), &payload).expect("blob");
        let read = routed
            .read_file(&root.to_string_lossy(), "blob.bin", None)
            .await
            .expect("read through the route");
        assert!(read.bytes() == payload, "the routed bytes differ");
        routed.close();
        client
            .remove_host(added.registration_id())
            .await
            .expect("remove host");
    });
}
