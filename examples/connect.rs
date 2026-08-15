use std::time::Duration;

use paseo_client::{parse_offer_url, CreateTerminalOpts, PaseoClient, TerminalStreamEvent};

const CLIENT_ID: &str = "paseo-client-example";

struct Options {
    stream: bool,
    probe: bool,
    restore: String,
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
        "usage:\n  connect <pairing-offer-url> [--stream] [--probe]\n  connect --local <host:port> [--tls] [--password <pw>] [--stream] [--probe]\n\n  --stream  subscribe to the first terminal and print output for ~3s (read-only)\n  --probe   additionally send a test keystroke and a resize (mutates that terminal)"
    )
}

async fn connect_from_args(args: &[String]) -> anyhow::Result<(PaseoClient, Options)> {
    let mut iter = args.iter().skip(1).peekable();
    let first = iter.next().ok_or_else(usage)?.clone();

    let mut host_port: Option<String> = None;
    let mut use_tls = false;
    let mut password: Option<String> = None;
    let mut options = Options {
        stream: false,
        probe: false,
        restore: "live".to_owned(),
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

    if first == "--local" {
        host_port = Some(iter.next().ok_or_else(usage)?.clone());
    }

    while let Some(flag) = iter.next() {
        match flag.as_str() {
            "--tls" => use_tls = true,
            "--password" => password = iter.next().cloned(),
            "--stream" => options.stream = true,
            "--restore" => {
                options.stream = true;
                options.restore = iter.next().cloned().ok_or_else(usage)?;
            }
            "--probe" => {
                options.stream = true;
                options.probe = true;
            }
            "--create-probe" => {
                options.create_probe = Some(iter.next().cloned().ok_or_else(usage)?);
            }
            "--inspect" => {
                options.inspect = Some(iter.next().cloned().unwrap_or_default());
            }
            "--create-agent" => {
                options.create_agent = Some(iter.next().cloned().unwrap_or_default());
            }
            "--watch" => {
                options.watch_stream = Some(iter.next().cloned().unwrap_or_default());
            }
            "--dump-timeline" => {
                options.dump_timeline = Some(iter.next().cloned().unwrap_or_default());
            }
            "--diff" => {
                options.diff = Some(iter.next().cloned().unwrap_or_default());
            }
            "--spawn-editor" => {
                options.spawn_editor = Some(iter.next().cloned().unwrap_or_default());
            }
            "--suggest" => {
                options.suggest = Some(iter.next().cloned().unwrap_or_default());
            }
            "--archive" => {
                options.archive = Some(iter.next().cloned().unwrap_or_default());
            }
            "--branches" => {
                options.branches = Some(iter.next().cloned().unwrap_or_default());
            }
            "--wscreate" => {
                options.wscreate = Some(iter.next().cloned().unwrap_or_default());
            }
            other => return Err(anyhow::anyhow!("unknown flag: {other}")),
        }
    }

    let client = if let Some(host_port) = host_port {
        println!("connecting to local daemon {host_port} (tls={use_tls})");
        PaseoClient::connect_local(&host_port, use_tls, password.as_deref(), CLIENT_ID).await?
    } else {
        let offer = parse_offer_url(&first)?;
        println!(
            "connecting via relay {} (serverId={})",
            offer.relay.endpoint, offer.server_id
        );
        PaseoClient::connect_relay(&offer, CLIENT_ID).await?
    };

    Ok((client, options))
}

async fn run_script(client: PaseoClient, options: Options) -> anyhow::Result<()> {
    let info = client.server_info();
    println!(
        "server_info: serverId={} version={:?} hostname={:?}",
        info.server_id, info.version, info.hostname
    );
    let mut features: Vec<&String> = info.features.keys().collect();
    features.sort();
    println!("features: {features:?}");

    let agents = client.fetch_agents().await?;
    println!("agents: {}", agents.len());
    for entry in &agents {
        let agent = &entry.agent;
        println!(
            "  {} [{}] {} ({})",
            agent.id,
            agent.status,
            agent.title.clone().unwrap_or_default(),
            agent.provider
        );
    }

    let workspaces = client.fetch_workspaces().await?;
    println!("workspaces: {}", workspaces.len());
    for ws in &workspaces {
        println!(
            "  {} project={} kind={} cwd={}",
            ws.id,
            ws.project_display_name,
            ws.workspace_kind,
            ws.cwd()
        );
    }

    let terminals = client.list_terminals(None).await?;
    println!("terminals: {}", terminals.len());
    for terminal in &terminals {
        println!(
            "  {} name={} cwd={}",
            terminal.id, terminal.name, terminal.cwd
        );
    }

    if let Some(path) = &options.wscreate {
        let ws = client.workspace_create_directory(path).await?;
        println!("workspace.create dir -> id={} cwd={}", ws.id, ws.cwd);
    } else if let Some(cwd) = &options.branches {
        let branches = client.branch_suggestions(cwd, "", 30).await?;
        println!("branches in {cwd}: {}", branches.len());
        for b in branches.iter().take(15) {
            println!("  {b}");
        }
    } else if let Some(agent_id) = &options.archive {
        client.archive_agent(agent_id).await?;
        println!("archived {agent_id}");
    } else if let Some(query) = &options.suggest {
        let dirs = client.directory_suggestions(query, 20).await?;
        println!("suggestions for {query:?}: {}", dirs.len());
        for d in &dirs {
            println!("  {d}");
        }
    } else if let Some(cwd) = &options.spawn_editor {
        spawn_editor_probe(&client, cwd).await?;
    } else if let Some(cwd) = &options.diff {
        probe_diff(&client, cwd).await?;
    } else if let Some(a) = &options.inspect {
        inspect_agent(&client, a, &agents).await?;
    } else if let Some(spec) = &options.create_agent {
        create_agent(&client, spec).await?;
    } else if let Some(agent_arg) = &options.watch_stream {
        watch_stream(&client, agent_arg, &agents).await?;
    } else if let Some(agent_arg) = &options.dump_timeline {
        dump_timeline(&client, agent_arg, &agents).await?;
    } else if let Some(cwd) = &options.create_probe {
        create_probe(&client, cwd).await?;
    } else if options.stream {
        if let Some(terminal) = terminals.first() {
            stream_terminal(&client, &terminal.id, &options.restore, options.probe).await?;
        } else {
            println!("no terminals to stream");
        }
    }

    client.close().await;
    Ok(())
}

async fn inspect_agent(
    client: &PaseoClient,
    agent_arg: &str,
    agents: &[paseo_client::AgentListEntry],
) -> anyhow::Result<()> {
    let agent_id = if agent_arg.is_empty() {
        agents
            .iter()
            .find(|e| e.agent.archived_at.is_none())
            .map(|e| e.agent.id.clone())
            .ok_or_else(|| anyhow::anyhow!("no agents"))?
    } else {
        agent_arg.to_owned()
    };
    let snap = client.fetch_agent(&agent_id).await?;
    println!(
        "agent {} provider={} status={} model={:?} mode={:?} thinking={:?}",
        snap.id,
        snap.provider,
        snap.status,
        snap.model,
        snap.current_mode_id,
        snap.thinking_option_id
    );
    println!(
        "availableModes: {:?}",
        snap.available_modes
            .iter()
            .map(|m| m.id.clone())
            .collect::<Vec<_>>()
    );
    let models = client.list_provider_models(&snap.provider, None).await?;
    println!("provider models ({}):", models.len());
    for m in &models {
        println!(
            "  {} thinking={:?} default={:?}",
            m.id,
            m.thinking_options
                .iter()
                .map(|o| o.id.clone())
                .collect::<Vec<_>>(),
            m.default_thinking_option_id
        );
    }

    if snap.available_modes.len() >= 2 {
        let current = snap.current_mode_id.clone().unwrap_or_default();
        let next = snap
            .available_modes
            .iter()
            .find(|m| m.id != current)
            .map(|m| m.id.clone())
            .unwrap();
        println!("subscribing to agent updates, then setting mode {current:?} -> {next}");
        let mut events = client.events();
        client.subscribe_agents().await?;
        client.set_agent_mode(&agent_id, &next).await?;
        let collect = async {
            while let Ok(event) = events.recv().await {
                if let paseo_client::DaemonEvent::AgentUpsert(s) = event {
                    if s.id == agent_id {
                        println!("  agent_update PUSH received: mode={:?}", s.current_mode_id);
                    }
                }
            }
        };
        let timer = async {
            smol::Timer::after(Duration::from_secs(4)).await;
        };
        smol::future::or(collect, timer).await;
    }
    Ok(())
}

async fn create_agent(client: &PaseoClient, spec: &str) -> anyhow::Result<()> {
    let mut parts = spec.splitn(3, ',');
    let provider = parts.next().unwrap_or("").trim();
    let cwd = parts.next().unwrap_or("").trim();
    let prompt = parts.next().map(|p| p.trim().to_owned());
    if provider.is_empty() || cwd.is_empty() {
        return Err(anyhow::anyhow!(
            "usage: --create-agent provider,cwd[,prompt]"
        ));
    }
    println!("opening project {cwd}");
    let workspace = client.open_project(cwd).await?;
    println!("workspace {workspace}");
    println!("creating {provider} agent in {cwd}");
    let snapshot = client
        .create_agent(provider, cwd, Some(&workspace), prompt.as_deref(), &[])
        .await?;
    println!(
        "created agent {} [{}] provider={} model={:?}",
        snapshot.id, snapshot.status, snapshot.provider, snapshot.model
    );
    Ok(())
}

async fn watch_stream(
    client: &PaseoClient,
    agent_arg: &str,
    agents: &[paseo_client::AgentListEntry],
) -> anyhow::Result<()> {
    let agent_id = if agent_arg.is_empty() {
        agents
            .iter()
            .find(|e| e.agent.status == "running")
            .or_else(|| agents.iter().find(|e| e.agent.archived_at.is_none()))
            .map(|e| e.agent.id.clone())
            .ok_or_else(|| anyhow::anyhow!("no agents"))?
    } else {
        agent_arg.to_owned()
    };
    println!("watching stream for agent {agent_id} (~15s)");
    let mut events = client.events();
    let _ = client
        .set_timeline_subscription(std::slice::from_ref(&agent_id))
        .await;

    let collect = async {
        while let Ok(event) = events.recv().await {
            if let paseo_client::DaemonEvent::AgentStream {
                agent_id: aid,
                event,
            } = event
            {
                if aid != agent_id {
                    continue;
                }
                if event.kind == "timeline" {
                    if let Some(item) = &event.item {
                        let text = item.text.clone().unwrap_or_default();
                        let preview: String = text.chars().take(60).collect();
                        println!(
                            "timeline kind={:18} msg={:10} call={:10} len={:4} | {}",
                            item.kind,
                            item.message_id.as_deref().unwrap_or("-"),
                            item.call_id.as_deref().unwrap_or("-"),
                            text.chars().count(),
                            preview.replace('\n', "⏎")
                        );
                    }
                } else {
                    println!("event kind={}", event.kind);
                }
            }
        }
    };
    let timer = async {
        smol::Timer::after(Duration::from_secs(15)).await;
    };
    smol::future::or(collect, timer).await;
    Ok(())
}

async fn dump_timeline(
    client: &PaseoClient,
    agent_arg: &str,
    agents: &[paseo_client::AgentListEntry],
) -> anyhow::Result<()> {
    let agent_id = if agent_arg.is_empty() {
        agents
            .iter()
            .find(|e| e.agent.archived_at.is_none())
            .map(|e| e.agent.id.clone())
            .ok_or_else(|| anyhow::anyhow!("no agents"))?
    } else {
        agent_arg.to_owned()
    };
    println!("timeline for agent {agent_id}");
    let items = client.fetch_agent_timeline(&agent_id, "tail", 200).await?;
    println!("items: {}", items.len());
    for (i, item) in items.iter().enumerate() {
        let msg = item.message_id.as_deref().unwrap_or("-");
        let call = item.call_id.as_deref().unwrap_or("-");
        let text = item.text.clone().unwrap_or_default();
        let preview: String = text.chars().take(50).collect();
        println!(
            "{i:3} kind={:20} msg={:10} call={:10} len={:4} | {}",
            item.kind,
            &msg[..msg.len().min(10)],
            &call[..call.len().min(10)],
            text.chars().count(),
            preview.replace('\n', "⏎")
        );
    }
    Ok(())
}

async fn create_probe(client: &PaseoClient, cwd: &str) -> anyhow::Result<()> {
    println!("opening project for {cwd}");
    let workspace_id = client.open_project(cwd).await?;
    println!("workspace {workspace_id}");
    println!("creating throwaway terminal in {cwd}");
    let opts = CreateTerminalOpts {
        name: Some("paseo-client-test".to_owned()),
        workspace_id: Some(workspace_id),
        rows: 24,
        cols: 80,
        ..Default::default()
    };
    let terminal = client.create_terminal(cwd, opts).await?;
    println!("created terminal {}", terminal.id);

    let handle = client.subscribe_terminal(&terminal.id, "live").await?;
    let rx = handle.output();

    handle.resize(30, 100).await?;
    handle.send_input(b"echo paseo-client-roundtrip\r").await?;

    let collect = async {
        while let Ok(event) = rx.recv_async().await {
            if let TerminalStreamEvent::Output(bytes) | TerminalStreamEvent::Restore(bytes) = event
            {
                print!("{}", String::from_utf8_lossy(&bytes));
            }
        }
    };
    let timer = async {
        smol::Timer::after(Duration::from_secs(2)).await;
    };
    smol::future::or(collect, timer).await;
    println!();

    handle.unsubscribe().await?;
    println!("killing terminal {}", terminal.id);
    client.kill_terminal(&terminal.id).await?;
    Ok(())
}

async fn spawn_editor_probe(client: &PaseoClient, cwd: &str) -> anyhow::Result<()> {
    let workspace_id = client.open_project(cwd).await?;
    println!("workspace {workspace_id}");
    let file = "/tmp/paseo-editor-probe.txt";
    let variants: Vec<(&str, String, Vec<String>)> = vec![
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
        let opts = CreateTerminalOpts {
            name: Some(format!("paseo-editor-{label}")),
            command: Some(program),
            args: Some(args),
            workspace_id: Some(workspace_id.clone()),
            rows: 40,
            cols: 120,
            ..Default::default()
        };
        let terminal = match client.create_terminal(cwd, opts).await {
            Ok(t) => t,
            Err(e) => {
                println!("[{label}] create_terminal FAILED: {e}");
                continue;
            }
        };
        println!("[{label}] created terminal {}", terminal.id);
        let handle = client.subscribe_terminal(&terminal.id, "live").await?;
        let rx = handle.output();
        handle.resize(40, 120).await?;

        let tid = terminal.id.clone();
        let collect = async {
            let mut total = 0usize;
            while let Ok(event) = rx.recv_async().await {
                match event {
                    TerminalStreamEvent::Output(bytes) | TerminalStreamEvent::Restore(bytes) => {
                        total += bytes.len();
                        let preview: String =
                            String::from_utf8_lossy(&bytes).chars().take(80).collect();
                        println!("[{label} +{} bytes] {:?}", bytes.len(), preview);
                    }
                    TerminalStreamEvent::Snapshot(_) => println!("[{label} snapshot]"),
                }
            }
            println!("[{label}] STREAM ENDED after {total} bytes");
        };
        let timer = async {
            smol::Timer::after(Duration::from_secs(4)).await;
        };
        smol::future::or(collect, timer).await;

        let terms = client.list_terminals(None).await?;
        let alive = terms.iter().any(|t| t.id == tid);
        println!("[{label}] alive after 4s: {alive}");
        let _ = handle.unsubscribe().await;
        let _ = client.kill_terminal(&tid).await;
    }
    Ok(())
}

async fn probe_diff(client: &PaseoClient, cwd: &str) -> anyhow::Result<()> {
    let status = client.checkout_status(cwd).await?;
    println!(
        "checkout_status: repoRoot={:?} branch={:?} baseRef={:?} dirty={:?}",
        status.repo_root, status.current_branch, status.base_ref, status.is_dirty
    );
    println!("subscribing to uncommitted diff for {cwd}");
    let diff = client
        .subscribe_checkout_diff(cwd, "uncommitted", None)
        .await?;
    println!(
        "subscription {} — {} file(s)",
        diff.subscription_id,
        diff.files.len()
    );
    if let Some(error) = &diff.error {
        println!("error [{}]: {}", error.code, error.message);
    }
    for file in &diff.files {
        println!(
            "  {} +{} -{} new={} deleted={} hunks={}",
            file.path,
            file.additions,
            file.deletions,
            file.is_new,
            file.is_deleted,
            file.hunks.len()
        );
    }
    client
        .unsubscribe_checkout_diff(&diff.subscription_id)
        .await?;
    Ok(())
}

async fn stream_terminal(
    client: &PaseoClient,
    terminal_id: &str,
    restore: &str,
    probe: bool,
) -> anyhow::Result<()> {
    println!("subscribing to terminal {terminal_id} for ~3s (restore={restore}, probe={probe})");
    let handle = client.subscribe_terminal(terminal_id, restore).await?;
    let rx = handle.output();

    if probe {
        handle.send_input(b"echo paseo-client-probe\r").await?;
        handle.resize(40, 120).await?;
    }

    let collect = async {
        while let Ok(event) = rx.recv_async().await {
            match event {
                TerminalStreamEvent::Output(bytes) | TerminalStreamEvent::Restore(bytes) => {
                    print!("{}", String::from_utf8_lossy(&bytes));
                }
                TerminalStreamEvent::Snapshot(_) => println!("[snapshot]"),
            }
        }
    };
    let timer = async {
        smol::Timer::after(Duration::from_secs(3)).await;
    };
    smol::future::or(collect, timer).await;
    println!();

    handle.unsubscribe().await?;
    Ok(())
}

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    smol::block_on(async move {
        let (client, options) = connect_from_args(&args).await?;
        let script = run_script(client.clone(), options);
        let driver = async {
            let _ = client.run().await;
            Ok::<(), anyhow::Error>(())
        };
        smol::future::or(script, driver).await
    })
}
