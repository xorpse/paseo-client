use std::net::IpAddr;
use std::path::PathBuf;
use std::str::FromStr;
use std::{env, fmt};

use url::Url;

use crate::error::EndpointError;
use crate::offer::ConnectionOffer;

pub const DEFAULT_SSH_DAEMON_PORT: u16 = 6767;

#[derive(Clone, Eq, PartialEq)]
pub struct ConnectTarget {
    endpoint: Endpoint,
    password: Option<String>,
}

impl fmt::Debug for ConnectTarget {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ConnectTarget")
            .field("endpoint", &self.endpoint)
            .field("password", &self.password.as_ref().map(|_| "<redacted>"))
            .finish()
    }
}

impl ConnectTarget {
    pub fn new(endpoint: Endpoint) -> ConnectTarget {
        ConnectTarget {
            endpoint,
            password: None,
        }
    }

    pub fn endpoint(&self) -> &Endpoint {
        &self.endpoint
    }

    pub fn password(&self) -> Option<&str> {
        self.password.as_deref()
    }

    pub fn set_password(&mut self, password: Option<String>) {
        self.password = password.filter(|password| !password.is_empty());
    }

    pub fn with_password(mut self, password: Option<String>) -> ConnectTarget {
        self.set_password(password);
        self
    }
}

impl FromStr for ConnectTarget {
    type Err = EndpointError;

    fn from_str(input: &str) -> Result<ConnectTarget, EndpointError> {
        let input = input.trim();
        if input.contains("#offer=") {
            return Ok(ConnectTarget::new(Endpoint::Relay(
                ConnectionOffer::parse_url(input)?,
            )));
        }
        if let Some(path) = input.strip_prefix("unix://") {
            return Ok(ConnectTarget::new(Endpoint::Unix(socket_path(
                path.trim(),
                input,
            )?)));
        }
        if input.starts_with('/') || input.starts_with('~') {
            return Ok(ConnectTarget::new(Endpoint::Unix(socket_path(
                input, input,
            )?)));
        }
        let relay_uri = match input.find("#connect=") {
            Some(index) => Some(
                percent_encoding::percent_decode_str(&input[index + "#connect=".len()..])
                    .decode_utf8_lossy()
                    .into_owned(),
            ),
            None => input.starts_with("relay://").then(|| input.to_owned()),
        };
        if let Some(relay_uri) = relay_uri {
            let url = parse_url(&relay_uri)?;
            let offer = ConnectionOffer::from_relay_uri(&url, &relay_uri)?;
            return Ok(ConnectTarget::new(Endpoint::Relay(offer))
                .with_password(query_value(&url, "password")));
        }
        if input.starts_with("ssh://") {
            return Ok(ConnectTarget::new(Endpoint::Ssh(SshTarget::parse(input)?)));
        }
        if input.starts_with("tcp://") {
            let url = parse_url(input)?;
            if !url.username().is_empty() || url.password().is_some() {
                return Err(EndpointError::UnsupportedPart {
                    input: input.to_owned(),
                    part: "userinfo".to_owned(),
                });
            }
            let host = url.host_str().ok_or_else(|| EndpointError::MissingHost {
                input: input.to_owned(),
            })?;
            let port = require_port(url.port(), input)?;
            let endpoint = TcpEndpoint {
                address: HostPort::new(host.trim_start_matches('[').trim_end_matches(']'), port),
                use_tls: query_value(&url, "ssl").as_deref() == Some("true"),
            };
            return Ok(ConnectTarget::new(Endpoint::Tcp(endpoint))
                .with_password(query_value(&url, "password")));
        }
        if input.contains("://") {
            return Err(EndpointError::UnsupportedScheme {
                input: input.to_owned(),
            });
        }
        if !input.is_empty() && input.bytes().all(|byte| byte.is_ascii_digit()) {
            return Ok(ConnectTarget::new(Endpoint::Tcp(TcpEndpoint {
                address: HostPort::new("127.0.0.1", parse_port(input, input)?),
                use_tls: false,
            })));
        }
        Ok(ConnectTarget::new(Endpoint::Tcp(TcpEndpoint {
            address: HostPort::parse(input)?,
            use_tls: false,
        })))
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Endpoint {
    Relay(ConnectionOffer),
    Ssh(SshTarget),
    Tcp(TcpEndpoint),
    Unix(PathBuf),
}

impl Endpoint {
    pub fn is_local(&self) -> bool {
        match self {
            Endpoint::Relay(_) | Endpoint::Ssh(_) => false,
            Endpoint::Tcp(tcp) => tcp.address().is_loopback(),
            Endpoint::Unix(_) => true,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TcpEndpoint {
    address: HostPort,
    use_tls: bool,
}

impl TcpEndpoint {
    pub fn new(address: HostPort, use_tls: bool) -> TcpEndpoint {
        TcpEndpoint { address, use_tls }
    }

    pub fn address(&self) -> &HostPort {
        &self.address
    }

    pub fn use_tls(&self) -> bool {
        self.use_tls
    }

    pub(crate) fn ws_url(&self) -> String {
        let scheme = if self.use_tls { "wss" } else { "ws" };
        format!("{scheme}://{}/ws", self.address)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HostPort {
    host: String,
    port: u16,
}

impl HostPort {
    pub fn new(host: impl Into<String>, port: u16) -> HostPort {
        HostPort {
            host: host.into(),
            port,
        }
    }

    pub fn parse(input: &str) -> Result<HostPort, EndpointError> {
        let trimmed = input.trim();
        let (host, port) = match trimmed.strip_prefix('[') {
            Some(rest) => {
                let (host, port) =
                    rest.split_once("]:")
                        .ok_or_else(|| EndpointError::MissingPort {
                            input: input.to_owned(),
                        })?;
                (host, port)
            }
            None => trimmed
                .rsplit_once(':')
                .ok_or_else(|| EndpointError::MissingPort {
                    input: input.to_owned(),
                })?,
        };
        let host = host.trim();
        if host.is_empty() {
            return Err(EndpointError::MissingHost {
                input: input.to_owned(),
            });
        }
        Ok(HostPort::new(host, parse_port(port, input)?))
    }

    pub fn host(&self) -> &str {
        &self.host
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    pub fn is_loopback(&self) -> bool {
        self.host.eq_ignore_ascii_case("localhost")
            || self
                .host
                .parse::<IpAddr>()
                .is_ok_and(|address| address.is_loopback())
    }
}

impl fmt::Display for HostPort {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.host.contains(':') {
            write!(f, "[{}]:{}", self.host, self.port)
        } else {
            write!(f, "{}:{}", self.host, self.port)
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SshTarget {
    destination: String,
    ssh_port: Option<u16>,
    daemon_port: u16,
}

impl SshTarget {
    pub fn new(destination: impl Into<String>, daemon_port: u16) -> SshTarget {
        SshTarget {
            destination: destination.into(),
            ssh_port: None,
            daemon_port,
        }
    }

    pub fn destination(&self) -> &str {
        &self.destination
    }

    pub fn ssh_port(&self) -> Option<u16> {
        self.ssh_port
    }

    pub fn set_ssh_port(&mut self, ssh_port: Option<u16>) {
        self.ssh_port = ssh_port;
    }

    pub fn with_ssh_port(mut self, ssh_port: Option<u16>) -> SshTarget {
        self.set_ssh_port(ssh_port);
        self
    }

    pub fn daemon_port(&self) -> u16 {
        self.daemon_port
    }

    fn parse(input: &str) -> Result<SshTarget, EndpointError> {
        let url = parse_url(input)?;
        let unsupported = |part: &str| EndpointError::UnsupportedPart {
            input: input.to_owned(),
            part: part.to_owned(),
        };
        if url.password().is_some() {
            return Err(unsupported("password"));
        }
        if !matches!(url.path(), "" | "/") {
            return Err(unsupported(url.path()));
        }
        if url.fragment().is_some() {
            return Err(unsupported("fragment"));
        }
        let mut daemon_port = None;
        for (name, value) in url.query_pairs() {
            if name != "daemonPort" || daemon_port.is_some() {
                return Err(unsupported(&name));
            }
            daemon_port = Some(parse_port(&value, input)?);
        }
        let host = url
            .host_str()
            .map(|host| host.trim_start_matches('[').trim_end_matches(']').trim())
            .filter(|host| !host.is_empty())
            .ok_or_else(|| EndpointError::MissingHost {
                input: input.to_owned(),
            })?;
        let user = percent_encoding::percent_decode_str(url.username()).decode_utf8_lossy();
        let destination = if user.is_empty() {
            host.to_owned()
        } else {
            format!("{user}@{host}")
        };
        let invalid = |name: &str| name.starts_with('-') || name.chars().any(char::is_whitespace);
        if invalid(host) || invalid(destination.trim()) {
            return Err(EndpointError::InvalidSshHost {
                input: input.to_owned(),
            });
        }
        let ssh_port = url
            .port()
            .map(|port| require_port(Some(port), input))
            .transpose()?;
        Ok(SshTarget {
            destination,
            ssh_port,
            daemon_port: daemon_port.unwrap_or(DEFAULT_SSH_DAEMON_PORT),
        })
    }

    pub(crate) fn tunnel_args(&self) -> Vec<String> {
        let mut args = [
            "-T",
            "-o",
            "BatchMode=yes",
            "-o",
            "ConnectTimeout=10",
            "-o",
            "ServerAliveInterval=15",
            "-o",
            "ServerAliveCountMax=3",
            "-o",
            "ClearAllForwardings=yes",
            "-o",
            "ExitOnForwardFailure=yes",
        ]
        .map(str::to_owned)
        .to_vec();
        if let Some(port) = self.ssh_port {
            args.push("-p".to_owned());
            args.push(port.to_string());
        }
        args.push("-W".to_owned());
        args.push(format!("127.0.0.1:{}", self.daemon_port));
        args.push(self.destination.clone());
        args
    }

    pub(crate) fn ws_url(&self) -> String {
        format!("ws://127.0.0.1:{}/ws", self.daemon_port)
    }
}

fn socket_path(path: &str, input: &str) -> Result<PathBuf, EndpointError> {
    if path.is_empty() {
        return Err(EndpointError::MissingHost {
            input: input.to_owned(),
        });
    }
    let home_relative = match path {
        "~" => Some(""),
        _ => path.strip_prefix("~/"),
    };
    let Some(relative) = home_relative else {
        return Ok(PathBuf::from(path));
    };
    let home = env::home_dir().ok_or_else(|| EndpointError::HomeUnavailable {
        input: input.to_owned(),
    })?;
    Ok(home.join(relative))
}

fn parse_url(input: &str) -> Result<Url, EndpointError> {
    Url::parse(input).map_err(|source| EndpointError::Url {
        input: input.to_owned(),
        source,
    })
}

fn query_value(url: &Url, key: &str) -> Option<String> {
    url.query_pairs()
        .find(|(name, _)| name == key)
        .map(|(_, value)| value.into_owned())
        .filter(|value| !value.is_empty())
}

fn require_port(port: Option<u16>, input: &str) -> Result<u16, EndpointError> {
    match port {
        None => Err(EndpointError::MissingPort {
            input: input.to_owned(),
        }),
        Some(0) => Err(EndpointError::InvalidPort {
            input: input.to_owned(),
        }),
        Some(port) => Ok(port),
    }
}

fn parse_port(port: &str, input: &str) -> Result<u16, EndpointError> {
    let port = port
        .parse::<u16>()
        .map_err(|_| EndpointError::InvalidPort {
            input: input.to_owned(),
        })?;
    require_port(Some(port), input)
}
