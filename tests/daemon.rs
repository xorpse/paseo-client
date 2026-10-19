use std::num::NonZeroU16;
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::Duration;

use async_net::TcpListener;
use futures::io::{AsyncReadExt, AsyncWriteExt};
use futures_rustls::rustls::{ClientConfig, RootCertStore};
use paseo_client::protocol::channels::ProcessSpawn;
use paseo_client::protocol::terminals::TerminalSize;
use paseo_client::{ConnectOptions, ConnectTarget, PaseoClient};

const PORT_VARIABLE: &str = "PASEO_TEST_DAEMON_PORT";

async fn client() -> PaseoClient {
    let port =
        std::env::var(PORT_VARIABLE).expect("PASEO_TEST_DAEMON_PORT names an isolated daemon");
    let target = format!("127.0.0.1:{port}")
        .parse::<ConnectTarget>()
        .expect("target");
    let provider = Arc::new(futures_rustls::rustls::crypto::ring::default_provider());
    let tls = ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .expect("protocol versions")
        .with_root_certificates(RootCertStore::empty())
        .with_no_client_auth();
    let client = PaseoClient::connect(
        &target,
        &ConnectOptions::new("paseo-client-daemon-test", Arc::new(tls)),
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
        let payload = (0..4 * 1024 * 1024)
            .map(|index: u32| (index % 256) as u8)
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
            .map(|index| (index % 256) as u8)
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
