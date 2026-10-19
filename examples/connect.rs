use std::num::NonZeroU16;
use std::sync::Arc;
use std::time::Duration;

use futures_rustls::rustls::{ClientConfig, RootCertStore};
use paseo_client::protocol::agents::{
    AgentCreateRequest, AgentDirectoryEntry, AgentDirectoryQuery, AgentSessionConfig, AgentStatus,
    AgentUpdate,
};
use paseo_client::protocol::diff::DiffCompare;
use paseo_client::protocol::stream::{AgentStreamEvent, TimelineUpdate};
use paseo_client::protocol::terminals::{
    CreateTerminalOptions, RestoreMode, TerminalRestore, TerminalSize, TerminalStreamEvent,
};
use paseo_client::protocol::timeline::{TimelineItem, TimelineQuery};
use paseo_client::protocol::workspaces::{
    DirectorySource, DirectorySuggestionsQuery, WorkspaceDirectoryQuery, WorkspaceSource,
};
use paseo_client::{ConnectOptions, ConnectTarget, PaseoClient, ResizeIntent, TerminalHandle};

const CLIENT_ID: &str = "paseo-client-example";

struct Options {
    stream: bool,
    probe: bool,
    restore: RestoreMode,
    create_probe: Option<String>,
    dump_timeline: Option<String>,
    watch_stream: Option<String>,
    create_agent: Option<String>,
    inspect: Option<String>,
    diff: Option<String>,
    spawn_editor: Option<String>,
    suggest: Option<String>,
    archive: Option<String>,
    branches: Option<String>,
    wscreate: Option<String>,
}

fn usage() -> anyhow::Error {
    anyhow::anyhow!(
        "usage:\n  connect <endpoint> [--password <pw>] [--stream] [--probe] [--restore <live|visible-snapshot|full-snapshot>]\n\n  <endpoint> is host:port, tcp://host:port?ssl=true, unix:///path, ssh://[user@]host[:port][?daemonPort=N],\n  relay://endpoint/serverId?key=..., or a pairing offer URL.\n\n  --stream  subscribe to the first terminal and print output for ~3s (read-only)\n  --probe   additionally send a test keystroke and a resize (mutates that terminal)"
    )
}

fn tls() -> anyhow::Result<Arc<ClientConfig>> {
    let mut roots = RootCertStore::empty();
    roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    let provider = Arc::new(futures_rustls::rustls::crypto::ring::default_provider());
    let config = ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()?
        .with_root_certificates(roots)
        .with_no_client_auth();
    Ok(Arc::new(config))
}

fn restore_mode(value: &str) -> anyhow::Result<RestoreMode> {
    match value {
        "live" => Ok(RestoreMode::Live),
        "visible-snapshot" => Ok(RestoreMode::VisibleSnapshot),
        "full-snapshot" => Ok(RestoreMode::FullSnapshot),
        other => Err(anyhow::anyhow!("unknown restore mode {other}")),
    }
}

async fn connect_from_args(args: &[String]) -> anyhow::Result<(PaseoClient, Options)> {
    let mut iter = args.iter().skip(1);
    let mut target = iter.next().ok_or_else(usage)?.parse::<ConnectTarget>()?;
    let mut options = Options {
        stream: false,
        probe: false,
        restore: RestoreMode::Live,
        create_probe: None,
        dump_timeline: None,
        watch_stream: None,
        create_agent: None,
        inspect: None,
        diff: None,
        spawn_editor: None,
        suggest: None,
        archive: None,
        branches: None,
        wscreate: None,
    };

    while let Some(flag) = iter.next() {
        match flag.as_str() {
            "--password" => target.set_password(iter.next().cloned()),
            "--stream" => options.stream = true,
            "--restore" => {
                options.stream = true;
                options.restore = restore_mode(iter.next().ok_or_else(usage)?)?;
            }
            "--probe" => {
                options.stream = true;
                options.probe = true;
            }
            "--create-probe" => {
                options.create_probe = Some(iter.next().cloned().ok_or_else(usage)?)
            }
            "--inspect" => options.inspect = Some(iter.next().cloned().unwrap_or_default()),
            "--create-agent" => {
                options.create_agent = Some(iter.next().cloned().unwrap_or_default())
            }
            "--watch" => options.watch_stream = Some(iter.next().cloned().unwrap_or_default()),
            "--dump-timeline" => {
                options.dump_timeline = Some(iter.next().cloned().unwrap_or_default())
            }
            "--diff" => options.diff = Some(iter.next().cloned().unwrap_or_default()),
            "--spawn-editor" => {
                options.spawn_editor = Some(iter.next().cloned().unwrap_or_default())
            }
            "--suggest" => options.suggest = Some(iter.next().cloned().unwrap_or_default()),
            "--archive" => options.archive = Some(iter.next().cloned().unwrap_or_default()),
            "--branches" => options.branches = Some(iter.next().cloned().unwrap_or_default()),
            "--wscreate" => options.wscreate = Some(iter.next().cloned().unwrap_or_default()),
            other => return Err(anyhow::anyhow!("unknown flag: {other}")),
        }
    }

    println!("connecting to {:?}", target.endpoint());
    let client = PaseoClient::connect(&target, &ConnectOptions::new(CLIENT_ID, tls()?)).await?;
    Ok((client, options))
}

fn pick_agent<'a>(
    agent_arg: &str,
    agents: &'a [AgentDirectoryEntry],
    prefer: Option<AgentStatus>,
) -> anyhow::Result<String> {
    if !agent_arg.is_empty() {
        return Ok(agent_arg.to_owned());
    }
    let active = |entry: &&'a AgentDirectoryEntry| entry.agent().archived_at().is_none();
    prefer
        .and_then(|status| agents.iter().find(|entry| entry.agent().status() == status))
        .or_else(|| agents.iter().find(active))
        .map(|entry| entry.agent().id().to_owned())
        .ok_or_else(|| anyhow::anyhow!("no agents"))
}

async fn run_script(client: PaseoClient, options: Options) -> anyhow::Result<()> {
    let info = client.server_info();
    println!(
        "server_info: serverId={} version={:?} hostname={:?}",
        info.server_id(),
        info.version(),
        info.hostname()
    );

    let agent_page = client.fetch_agents(&AgentDirectoryQuery::default()).await?;
    let agents = agent_page.entries();
    println!("agents: {}", agents.len());
    for entry in agents {
        let agent = entry.agent();
        println!(
            "  {} [{:?}] {} ({})",
            agent.id(),
            agent.status(),
            agent.title().unwrap_or_default(),
            agent.provider()
        );
    }

    let workspace_page = client
        .fetch_workspaces(&WorkspaceDirectoryQuery::default())
        .await?;
    println!("workspaces: {}", workspace_page.entries().len());
    for workspace in workspace_page.entries() {
        println!(
            "  {} project={} kind={:?} cwd={}",
            workspace.id(),
            workspace.project_display_name(),
            workspace.workspace_kind(),
            workspace.directory()
        );
    }

    let terminals = client.list_terminals(None, None).await?;
    println!("terminals: {}", terminals.len());
    for terminal in &terminals {
        println!(
            "  {} name={} cwd={:?}",
            terminal.id(),
            terminal.name(),
            terminal.cwd()
        );
    }

    if let Some(path) = &options.wscreate {
        let source = WorkspaceSource::Directory(DirectorySource::new(path.clone()));
        let created = client.create_workspace(&source, None).await?;
        println!(
            "workspace.create dir -> id={} cwd={}",
            created.workspace().id(),
            created.workspace().directory()
        );
    } else if let Some(cwd) = &options.branches {
        let branches = client.branch_suggestions(cwd, "", 30).await?;
        println!("branches in {cwd}: {}", branches.len());
        for branch in branches.iter().take(15) {
            println!("  {branch}");
        }
    } else if let Some(agent_id) = &options.archive {
        let archived_at = client.archive_agent(agent_id).await?;
        println!("archived {agent_id} at {archived_at}");
    } else if let Some(query) = &options.suggest {
        let suggestions = client
            .directory_suggestions(
                &DirectorySuggestionsQuery::directories(query.clone()).with_limit(20)?,
            )
            .await?;
        println!("suggestions for {query:?}: {}", suggestions.len());
        for suggestion in &suggestions {
            println!("  {}", suggestion.path());
        }
    } else if let Some(cwd) = &options.spawn_editor {
        spawn_editor_probe(&client, cwd).await?;
    } else if let Some(cwd) = &options.diff {
        probe_diff(&client, cwd).await?;
    } else if let Some(agent_arg) = &options.inspect {
        inspect_agent(&client, &pick_agent(agent_arg, agents, None)?).await?;
    } else if let Some(spec) = &options.create_agent {
        create_agent(&client, spec).await?;
    } else if let Some(agent_arg) = &options.watch_stream {
        watch_stream(
            &client,
            &pick_agent(agent_arg, agents, Some(AgentStatus::Running))?,
        )
        .await?;
    } else if let Some(agent_arg) = &options.dump_timeline {
        dump_timeline(&client, &pick_agent(agent_arg, agents, None)?).await?;
    } else if let Some(cwd) = &options.create_probe {
        create_probe(&client, cwd).await?;
    } else if options.stream {
        match terminals.first() {
            Some(terminal) => {
                stream_terminal(&client, terminal.id(), options.restore, options.probe).await?
            }
            None => println!("no terminals to stream"),
        }
    }

    client.close();
    Ok(())
}

async fn inspect_agent(client: &PaseoClient, agent_id: &str) -> anyhow::Result<()> {
    let entry = client.fetch_agent(agent_id).await?;
    let snapshot = entry.agent();
    println!(
        "agent {} provider={} status={:?} model={:?} mode={:?} thinking={:?}",
        snapshot.id(),
        snapshot.provider(),
        snapshot.status(),
        snapshot.model(),
        snapshot.current_mode_id(),
        snapshot.thinking_option_id()
    );
    let modes = snapshot
        .available_modes()
        .iter()
        .map(|mode| mode.id())
        .collect::<Vec<_>>();
    println!("availableModes: {modes:?}");
    let models = client
        .list_provider_models(snapshot.provider(), None)
        .await?;
    println!("provider models ({}):", models.len());
    for model in &models {
        let thinking = model
            .thinking_options()
            .iter()
            .map(|option| option.id())
            .collect::<Vec<_>>();
        println!(
            "  {} thinking={thinking:?} default={:?}",
            model.id(),
            model.default_thinking_option_id()
        );
    }

    let current = snapshot.current_mode_id().unwrap_or_default();
    let Some(next) = snapshot
        .available_modes()
        .iter()
        .find(|mode| mode.id() != current)
    else {
        return Ok(());
    };
    println!(
        "observing agent updates, then setting mode {current:?} -> {}",
        next.id()
    );
    let subscription = client
        .observe_agents(&AgentDirectoryQuery::default())
        .await?;
    client.set_agent_mode(agent_id, next.id()).await?;
    let collect = async {
        while let Ok(update) = subscription.updates().recv_async().await {
            if let Ok(AgentUpdate::Upsert(upsert)) = update
                && upsert.agent().id() == agent_id
            {
                println!(
                    "  agent_update received: mode={:?}",
                    upsert.agent().current_mode_id()
                );
            }
        }
    };
    smol::future::or(collect, async {
        smol::Timer::after(Duration::from_secs(4)).await;
    })
    .await;
    Ok(())
}

async fn create_agent(client: &PaseoClient, spec: &str) -> anyhow::Result<()> {
    let mut parts = spec.splitn(3, ',');
    let provider = parts.next().unwrap_or("").trim();
    let cwd = parts.next().unwrap_or("").trim();
    if provider.is_empty() || cwd.is_empty() {
        return Err(anyhow::anyhow!(
            "usage: --create-agent provider,cwd[,prompt]"
        ));
    }
    println!("opening project {cwd}");
    let workspace = client.open_project(cwd).await?;
    println!("workspace {}", workspace.id());
    let mut request = AgentCreateRequest::new(AgentSessionConfig::new(provider, cwd))
        .with_workspace_id(workspace.id());
    if let Some(prompt) = parts.next() {
        request = request.with_initial_prompt(prompt.trim());
    }
    println!("creating {provider} agent in {cwd}");
    let creation = client.create_agent(&request)?;
    let progress = creation.progress().clone();
    let report = async {
        while let Ok(Ok(snapshot)) = progress.recv_async().await {
            println!("  creation phase {:?}", snapshot.phase());
        }
    };
    let snapshot = smol::future::or(async { Some(creation.finish().await) }, async {
        report.await;
        None
    })
    .await
    .ok_or_else(|| anyhow::anyhow!("creation progress ended without a result"))??;
    println!(
        "created agent {} [{:?}] provider={} model={:?}",
        snapshot.id(),
        snapshot.status(),
        snapshot.provider(),
        snapshot.model()
    );
    Ok(())
}

async fn watch_stream(client: &PaseoClient, agent_id: &str) -> anyhow::Result<()> {
    println!("watching stream for agent {agent_id} (~15s)");
    let subscription = client.observe_timeline(&[agent_id.to_owned()]).await?;
    let collect = async {
        while let Ok(update) = subscription.updates().recv_async().await {
            match update {
                Ok(TimelineUpdate::Stream(message)) => match message.event() {
                    AgentStreamEvent::Timeline(timeline) => {
                        println!("seq={:?} {}", message.seq(), describe(timeline.item()));
                    }
                    event => println!("seq={:?} event {event:?}", message.seq()),
                },
                Ok(TimelineUpdate::Replacement(replacement)) => {
                    println!("timeline replaced, epoch {}", replacement.epoch());
                }
                Err(error) => println!("update failed: {error}"),
            }
        }
    };
    smol::future::or(collect, async {
        smol::Timer::after(Duration::from_secs(15)).await;
    })
    .await;
    Ok(())
}

fn describe(item: &TimelineItem) -> String {
    let preview = |text: &str| text.chars().take(60).collect::<String>().replace('\n', "⏎");
    match item {
        TimelineItem::AssistantMessage(message) => {
            format!("assistant | {}", preview(message.text()))
        }
        TimelineItem::UserMessage(message) => format!("user      | {}", preview(message.text())),
        TimelineItem::Reasoning(reasoning) => format!("reasoning | {}", preview(reasoning.text())),
        TimelineItem::ToolCall(call) => {
            format!("tool      | {} {:?}", call.name(), call.status())
        }
        other => format!("{other:?}"),
    }
}

async fn dump_timeline(client: &PaseoClient, agent_id: &str) -> anyhow::Result<()> {
    println!("timeline for agent {agent_id}");
    let page = client
        .fetch_agent_timeline(&TimelineQuery::tail(agent_id, 200))
        .await?;
    println!("entries: {} (epoch {})", page.entries().len(), page.epoch());
    for entry in page.entries() {
        println!(
            "{:4}-{:<4} {}",
            entry.seq_start(),
            entry.seq_end(),
            describe(entry.item())
        );
    }
    Ok(())
}

async fn print_terminal(terminal: &TerminalHandle, label: &str, duration: Duration) {
    let collect = async {
        while let Ok(event) = terminal.events().recv_async().await {
            match event {
                Ok(TerminalStreamEvent::Output(bytes) | TerminalStreamEvent::Restore(bytes)) => {
                    print!("{}", String::from_utf8_lossy(&bytes));
                }
                Ok(TerminalStreamEvent::Snapshot(_)) => println!("[{label} snapshot]"),
                Ok(TerminalStreamEvent::Exit(error)) => {
                    println!("[{label} exited: {error:?}]");
                    break;
                }
                Err(error) => println!("[{label} stream error: {error}]"),
            }
        }
    };
    smol::future::or(collect, async {
        smol::Timer::after(duration).await;
    })
    .await;
    println!();
}

fn terminal_size(rows: u16, cols: u16) -> TerminalSize {
    TerminalSize::new(
        NonZeroU16::new(rows).expect("example rows are non-zero"),
        NonZeroU16::new(cols).expect("example columns are non-zero"),
    )
}

async fn create_probe(client: &PaseoClient, cwd: &str) -> anyhow::Result<()> {
    println!("opening project for {cwd}");
    let workspace = client.open_project(cwd).await?;
    println!("creating throwaway terminal in {cwd}");
    let options = CreateTerminalOptions::default()
        .with_name("paseo-client-test")
        .with_workspace_id(workspace.id())
        .with_size(terminal_size(24, 80));
    let terminal = client.create_terminal(cwd, &options).await?;
    println!("created terminal {}", terminal.id());

    let handle = client
        .subscribe_terminal(terminal.id(), &TerminalRestore::new(RestoreMode::Live))
        .await?;
    let writer = handle.writer();
    writer.resize(terminal_size(30, 100), ResizeIntent::Claim)?;
    writer.send_input(b"echo paseo-client-roundtrip\r")?;
    print_terminal(&handle, "probe", Duration::from_secs(2)).await;
    drop(handle);

    println!("killing terminal {}", terminal.id());
    client.kill_terminal(terminal.id()).await?;
    Ok(())
}

async fn spawn_editor_probe(client: &PaseoClient, cwd: &str) -> anyhow::Result<()> {
    let workspace = client.open_project(cwd).await?;
    println!("workspace {}", workspace.id());
    let file = "/tmp/paseo-editor-probe.txt";
    let variants = [
        (
            "direct",
            "nvim".to_owned(),
            vec!["+1".to_owned(), file.to_owned()],
        ),
        (
            "shell",
            "/bin/sh".to_owned(),
            vec![
                "-c".to_owned(),
                "exec \"${SHELL:-/bin/sh}\" -ilc \"$1\"".to_owned(),
                "sh".to_owned(),
                format!("exec nvim '+1' '{file}'"),
            ],
        ),
    ];

    for (label, program, args) in variants {
        println!("\n=== variant {label}: {program} {args:?} ===");
        let options = CreateTerminalOptions::default()
            .with_name(format!("paseo-editor-{label}"))
            .with_command(program, args)
            .with_workspace_id(workspace.id())
            .with_size(terminal_size(40, 120));
        let terminal = match client.create_terminal(cwd, &options).await {
            Ok(terminal) => terminal,
            Err(error) => {
                println!("[{label}] create_terminal FAILED: {error}");
                continue;
            }
        };
        println!("[{label}] created terminal {}", terminal.id());
        let handle = client
            .subscribe_terminal(terminal.id(), &TerminalRestore::new(RestoreMode::Live))
            .await?;
        handle
            .writer()
            .resize(terminal_size(40, 120), ResizeIntent::Claim)?;
        print_terminal(&handle, label, Duration::from_secs(4)).await;
        let alive = client
            .list_terminals(None, None)
            .await?
            .iter()
            .any(|t| t.id() == terminal.id());
        println!("[{label}] alive after 4s: {alive}");
        drop(handle);
        if let Err(error) = client.kill_terminal(terminal.id()).await {
            println!("[{label}] kill failed: {error}");
        }
    }
    Ok(())
}

async fn probe_diff(client: &PaseoClient, cwd: &str) -> anyhow::Result<()> {
    let status = client.checkout_status(cwd).await?;
    match status.repository() {
        Some(repository) => println!(
            "checkout_status: repoRoot={} branch={:?} baseRef={:?} dirty={}",
            repository.repo_root(),
            repository.current_branch(),
            repository.base_ref(),
            repository.is_dirty()
        ),
        None => println!("checkout_status: {cwd} is not a git checkout"),
    }
    println!("subscribing to uncommitted diff for {cwd}");
    let subscription = client
        .observe_checkout_diff(cwd, &DiffCompare::uncommitted())
        .await?;
    let diff = subscription.snapshot();
    println!(
        "subscription {} — {} file(s)",
        subscription.id(),
        diff.files().len()
    );
    if let Some(error) = diff.error() {
        println!("error [{:?}]: {}", error.code(), error.message());
    }
    for file in diff.files() {
        println!(
            "  {} +{} -{} new={} deleted={} hunks={}",
            file.path(),
            file.additions(),
            file.deletions(),
            file.is_new(),
            file.is_deleted(),
            file.hunks().len()
        );
    }
    Ok(())
}

async fn stream_terminal(
    client: &PaseoClient,
    terminal_id: &str,
    restore: RestoreMode,
    probe: bool,
) -> anyhow::Result<()> {
    println!("subscribing to terminal {terminal_id} for ~3s (restore={restore:?}, probe={probe})");
    let handle = client
        .subscribe_terminal(terminal_id, &TerminalRestore::new(restore))
        .await?;
    if probe {
        let writer = handle.writer();
        writer.send_input(b"echo paseo-client-probe\r")?;
        writer.resize(terminal_size(40, 120), ResizeIntent::Claim)?;
    }
    print_terminal(&handle, "stream", Duration::from_secs(3)).await;
    Ok(())
}

fn main() -> anyhow::Result<()> {
    let args = std::env::args().collect::<Vec<_>>();
    smol::block_on(async move {
        let (client, options) = connect_from_args(&args).await?;
        let script = run_script(client.clone(), options);
        let driver = async {
            client.run().await?;
            Ok::<(), anyhow::Error>(())
        };
        smol::future::or(script, driver).await
    })
}
