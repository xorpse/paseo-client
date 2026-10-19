use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use paseo_client::{ConnectTarget, DEFAULT_SSH_DAEMON_PORT, Endpoint, EndpointError};

fn parse(input: &str) -> ConnectTarget {
    input.parse::<ConnectTarget>().expect("endpoint parses")
}

fn reject(input: &str) -> EndpointError {
    input
        .parse::<ConnectTarget>()
        .expect_err("endpoint is rejected")
}

#[test]
fn bare_host_port_is_plain_tcp() {
    let Endpoint::Tcp(tcp) = parse("localhost:6767").endpoint().clone() else {
        panic!("expected tcp");
    };
    assert_eq!(
        (tcp.address().host(), tcp.address().port(), tcp.use_tls()),
        ("localhost", 6767, false)
    );
    let Endpoint::Tcp(tcp) = parse("[::1]:6767").endpoint().clone() else {
        panic!("expected tcp");
    };
    assert_eq!(tcp.address().host(), "::1");
    assert_eq!(tcp.address().to_string(), "[::1]:6767");
}

#[test]
fn tcp_uri_carries_tls_and_password() {
    let target = parse("tcp://daemon.example:443?ssl=true&password=s3cret");
    let Endpoint::Tcp(tcp) = target.endpoint() else {
        panic!("expected tcp");
    };
    assert!(tcp.use_tls());
    assert_eq!(tcp.address().to_string(), "daemon.example:443");
    assert_eq!(target.password(), Some("s3cret"));
}

#[test]
fn invalid_ports_are_rejected() {
    assert!(matches!(
        reject("localhost:0"),
        EndpointError::InvalidPort { .. }
    ));
    assert!(matches!(
        reject("localhost:99999"),
        EndpointError::InvalidPort { .. }
    ));
    assert!(matches!(
        reject("localhost"),
        EndpointError::MissingPort { .. }
    ));
    assert!(matches!(
        reject("tcp://localhost"),
        EndpointError::MissingPort { .. }
    ));
}

#[test]
fn unix_socket_path_is_kept_verbatim() {
    assert_eq!(
        parse("unix:///tmp/paseo/daemon.sock").endpoint(),
        &Endpoint::Unix("/tmp/paseo/daemon.sock".into())
    );
}

#[test]
fn ssh_uri_matches_the_cli_transport_shape() {
    let Endpoint::Ssh(ssh) = parse("ssh://dev%40ops@build-box:2222?daemonPort=7000")
        .endpoint()
        .clone()
    else {
        panic!("expected ssh");
    };
    assert_eq!(ssh.destination(), "dev@ops@build-box");
    assert_eq!(ssh.ssh_port(), Some(2222));
    assert_eq!(ssh.daemon_port(), 7000);

    let Endpoint::Ssh(ssh) = parse("ssh://build-box").endpoint().clone() else {
        panic!("expected ssh");
    };
    assert_eq!(ssh.daemon_port(), DEFAULT_SSH_DAEMON_PORT);
    assert_eq!(ssh.ssh_port(), None);
}

#[test]
fn ssh_uri_rejects_parts_the_tunnel_cannot_honour() {
    assert!(matches!(
        reject("ssh://user:pw@host"),
        EndpointError::UnsupportedPart { .. }
    ));
    assert!(matches!(
        reject("ssh://host/path"),
        EndpointError::UnsupportedPart { .. }
    ));
    assert!(matches!(
        reject("ssh://host?other=1"),
        EndpointError::UnsupportedPart { .. }
    ));
    assert!(matches!(
        reject("ssh://host?daemonPort=1&daemonPort=2"),
        EndpointError::UnsupportedPart { .. }
    ));
    assert!(matches!(
        reject("ssh://-oProxyCommand=x"),
        EndpointError::InvalidSshHost { .. }
    ));
}

#[test]
fn relay_uri_and_offer_fragment_produce_the_same_offer() {
    let offer_json = r#"{"v":2,"serverId":"srv_1","daemonPublicKeyB64":"a2V5","relay":{"endpoint":"relay.example:443","useTls":true}}"#;
    let from_fragment = parse(&format!(
        "https://app.paseo.sh/#offer={}",
        URL_SAFE_NO_PAD.encode(offer_json)
    ));
    let from_uri = parse("relay://relay.example:443/srv_1?key=a2V5&ssl=true&password=pw");
    let (Endpoint::Relay(fragment), Endpoint::Relay(uri)) =
        (from_fragment.endpoint(), from_uri.endpoint())
    else {
        panic!("expected relay endpoints");
    };
    assert_eq!(fragment, uri);
    assert_eq!(uri.server_id(), "srv_1");
    assert_eq!(from_uri.password(), Some("pw"));
    assert_eq!(from_fragment.password(), None);

    let connect = parse(
        "https://app.paseo.sh/#connect=relay%3A%2F%2Frelay.example%3A443%2Fsrv_1%3Fkey%3Da2V5%26ssl%3Dtrue",
    );
    assert_eq!(connect.endpoint(), from_uri.endpoint());
}

#[test]
fn relay_uri_requires_server_and_key() {
    assert!(matches!(
        reject("relay://relay.example:443/?key=a2V5"),
        EndpointError::OfferIncomplete
    ));
    assert!(matches!(
        reject("relay://relay.example:443/srv"),
        EndpointError::OfferIncomplete
    ));
    assert!(matches!(
        reject("relay://relay.example/srv?key=k"),
        EndpointError::MissingPort { .. }
    ));
}

#[test]
fn unknown_schemes_are_rejected() {
    assert!(matches!(
        reject("ws://localhost:6767"),
        EndpointError::UnsupportedScheme { .. }
    ));
}

#[test]
fn bare_paths_and_ports_follow_the_cli_target_rules() {
    let home = std::env::home_dir().expect("home directory");
    let socket = |input: &str| {
        let Endpoint::Unix(path) = parse(input).endpoint().clone() else {
            panic!("expected a unix socket for {input}");
        };
        path
    };
    assert_eq!(
        socket("/run/paseo.sock"),
        std::path::PathBuf::from("/run/paseo.sock")
    );
    assert_eq!(socket("~/paseo.sock"), home.join("paseo.sock"));
    assert_eq!(socket("unix://~/paseo.sock"), home.join("paseo.sock"));
    let Endpoint::Tcp(tcp) = parse("6767").endpoint().clone() else {
        panic!("expected tcp");
    };
    assert_eq!(tcp.address().to_string(), "127.0.0.1:6767");
}

#[test]
fn ssh_host_is_validated_on_its_own() {
    assert!(matches!(
        reject("ssh://user@-badhost"),
        EndpointError::InvalidSshHost { .. }
    ));
}

#[test]
fn debug_output_redacts_the_password() {
    let target = parse("tcp://daemon.example:443?password=s3cret");
    let debug = format!("{target:?}");
    assert!(!debug.contains("s3cret"));
    assert!(debug.contains("<redacted>"));
}

#[test]
fn only_loopback_and_unix_endpoints_are_local() {
    for local in [
        "127.0.0.1:6767",
        "localhost:6767",
        "[::1]:6767",
        "/tmp/paseo.sock",
    ] {
        assert!(parse(local).endpoint().is_local(), "{local}");
    }
    for remote in [
        "10.0.0.2:6767",
        "build.example:6767",
        "ssh://me@127.0.0.1",
        "relay://127.0.0.1:443/srv?key=a2V5",
    ] {
        assert!(!parse(remote).endpoint().is_local(), "{remote}");
    }
}
