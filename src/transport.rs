use std::collections::VecDeque;
use std::io;
use std::path::Path;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Duration;

use async_io::Timer;
use async_net::TcpStream;
#[cfg(unix)]
use async_net::unix::UnixStream;
use async_process::{Child, ChildStderr, ChildStdin, ChildStdout, Command, Stdio};
use async_tungstenite::tungstenite::Message;
use async_tungstenite::tungstenite::client::IntoClientRequest;
use async_tungstenite::{WebSocketStream, client_async};
use futures::io::{AsyncRead, AsyncWrite};
use futures::{StreamExt, future};
use futures_rustls::TlsConnector;
use futures_rustls::client::TlsStream;
use futures_rustls::pki_types::ServerName;
use futures_rustls::rustls::ClientConfig;
use parking_lot::Mutex;

use crate::e2ee::{Channel, Ready};
use crate::endpoint::{Endpoint, HostPort, SshTarget};
use crate::error::{EndpointError, PaseoError, ProtocolViolation, SshTunnelClosed};

const HANDSHAKE_RETRY: Duration = Duration::from_secs(1);
const SSH_STDERR_LIMIT: usize = 8192;

pub(crate) enum Stream {
    Ssh(SshStream),
    Tcp(TcpStream),
    Tls(Box<TlsStream<TcpStream>>),
    #[cfg(unix)]
    Unix(UnixStream),
}

pub(crate) struct SshStream {
    _child: Child,
    stdin: ChildStdin,
    stdout: ChildStdout,
    stderr: Option<ChildStderr>,
    diagnostics: Arc<Mutex<SshDiagnostics>>,
}

#[derive(Default)]
struct SshDiagnostics {
    tail: VecDeque<u8>,
    line: Vec<u8>,
}

impl SshDiagnostics {
    fn record(&mut self, bytes: &[u8]) {
        self.tail.extend(bytes);
        let excess = self.tail.len().saturating_sub(SSH_STDERR_LIMIT);
        self.tail.drain(..excess);
        for &byte in bytes {
            if byte != b'\n' {
                if self.line.len() < SSH_STDERR_LIMIT {
                    self.line.push(byte);
                }
                continue;
            }
            log::warn!("ssh: {}", String::from_utf8_lossy(&self.line).trim_end());
            self.line.clear();
        }
    }

    fn detail(&self) -> String {
        let (front, back) = self.tail.as_slices();
        let mut detail = String::from_utf8_lossy(front).into_owned();
        detail.push_str(&String::from_utf8_lossy(back));
        detail.trim().to_owned()
    }
}

impl AsyncRead for Stream {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<io::Result<usize>> {
        match self.get_mut() {
            Stream::Ssh(stream) => stream.poll_read(cx, buf),
            Stream::Tcp(stream) => Pin::new(stream).poll_read(cx, buf),
            Stream::Tls(stream) => Pin::new(stream.as_mut()).poll_read(cx, buf),
            #[cfg(unix)]
            Stream::Unix(stream) => Pin::new(stream).poll_read(cx, buf),
        }
    }
}

impl AsyncWrite for Stream {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        match self.get_mut() {
            Stream::Ssh(stream) => Pin::new(&mut stream.stdin).poll_write(cx, buf),
            Stream::Tcp(stream) => Pin::new(stream).poll_write(cx, buf),
            Stream::Tls(stream) => Pin::new(stream.as_mut()).poll_write(cx, buf),
            #[cfg(unix)]
            Stream::Unix(stream) => Pin::new(stream).poll_write(cx, buf),
        }
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match self.get_mut() {
            Stream::Ssh(stream) => Pin::new(&mut stream.stdin).poll_flush(cx),
            Stream::Tcp(stream) => Pin::new(stream).poll_flush(cx),
            Stream::Tls(stream) => Pin::new(stream.as_mut()).poll_flush(cx),
            #[cfg(unix)]
            Stream::Unix(stream) => Pin::new(stream).poll_flush(cx),
        }
    }

    fn poll_close(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match self.get_mut() {
            Stream::Ssh(stream) => Pin::new(&mut stream.stdin).poll_close(cx),
            Stream::Tcp(stream) => Pin::new(stream).poll_close(cx),
            Stream::Tls(stream) => Pin::new(stream.as_mut()).poll_close(cx),
            #[cfg(unix)]
            Stream::Unix(stream) => Pin::new(stream).poll_close(cx),
        }
    }
}

impl SshStream {
    fn spawn(target: &SshTarget) -> Result<SshStream, PaseoError> {
        let mut child = Command::new("ssh")
            .args(target.tunnel_args())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()?;
        let stdin = child.stdin.take().expect("ssh stdin is piped");
        let stdout = child.stdout.take().expect("ssh stdout is piped");
        let stderr = child.stderr.take().expect("ssh stderr is piped");
        Ok(SshStream {
            _child: child,
            stdin,
            stdout,
            stderr: Some(stderr),
            diagnostics: Arc::default(),
        })
    }

    fn poll_read(&mut self, cx: &mut Context<'_>, buf: &mut [u8]) -> Poll<io::Result<usize>> {
        self.drain_stderr(cx);
        match Pin::new(&mut self.stdout).poll_read(cx, buf) {
            Poll::Ready(Ok(0)) if !buf.is_empty() => {
                let detail = self.diagnostics.lock().detail();
                if detail.is_empty() {
                    return Poll::Ready(Ok(0));
                }
                Poll::Ready(Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    SshTunnelClosed { detail },
                )))
            }
            poll => poll,
        }
    }

    fn drain_stderr(&mut self, cx: &mut Context<'_>) {
        let mut chunk = [0u8; 1024];
        while let Some(stderr) = self.stderr.as_mut() {
            match Pin::new(stderr).poll_read(cx, &mut chunk) {
                Poll::Ready(Ok(0)) => self.stderr = None,
                Poll::Ready(Ok(read)) => self.diagnostics.lock().record(&chunk[..read]),
                Poll::Ready(Err(error)) => {
                    log::warn!("could not read ssh stderr: {error}");
                    self.stderr = None;
                }
                Poll::Pending => return,
            }
        }
    }
}

pub(crate) type Socket = WebSocketStream<Stream>;

pub(crate) struct Connection {
    pub(crate) socket: Socket,
    pub(crate) wire: Wire,
}

impl Connection {
    pub(crate) async fn open(
        endpoint: &Endpoint,
        tls: &Arc<ClientConfig>,
    ) -> Result<Connection, PaseoError> {
        match endpoint {
            Endpoint::Tcp(tcp) => {
                let stream = Self::tcp(tcp.address(), tcp.use_tls(), tls).await?;
                Ok(Connection::plain(
                    Self::upgrade(&tcp.ws_url(), stream).await?,
                ))
            }
            Endpoint::Unix(path) => Self::unix(path).await,
            Endpoint::Ssh(target) => {
                let stream = SshStream::spawn(target)?;
                let diagnostics = stream.diagnostics.clone();
                match client_async(target.ws_url(), Stream::Ssh(stream)).await {
                    Ok((socket, _)) => Ok(Connection::plain(socket)),
                    Err(error) => {
                        let detail = diagnostics.lock().detail();
                        if detail.is_empty() {
                            return Err(error.into());
                        }
                        Err(PaseoError::Ssh { detail })
                    }
                }
            }
            Endpoint::Relay(offer) => {
                let channel = Channel::new(offer.daemon_public_key()?)?;
                let address = HostPort::parse(offer.relay().endpoint())?;
                let stream = Self::tcp(&address, offer.relay().use_tls(), tls).await?;
                let mut socket = Self::upgrade(&offer.relay_ws_url()?, stream).await?;
                Self::e2ee_handshake(&mut socket, &channel).await?;
                Ok(Connection {
                    socket,
                    wire: Wire {
                        channel: Some(channel),
                    },
                })
            }
        }
    }

    fn plain(socket: Socket) -> Connection {
        Connection {
            socket,
            wire: Wire { channel: None },
        }
    }

    #[cfg(unix)]
    async fn unix(path: &Path) -> Result<Connection, PaseoError> {
        let stream = UnixStream::connect(path).await?;
        Ok(Connection::plain(
            Self::upgrade("ws://localhost/ws", Stream::Unix(stream)).await?,
        ))
    }

    #[cfg(not(unix))]
    async fn unix(_path: &Path) -> Result<Connection, PaseoError> {
        Err(EndpointError::UnixUnsupported.into())
    }

    async fn tcp(
        address: &HostPort,
        use_tls: bool,
        tls: &Arc<ClientConfig>,
    ) -> Result<Stream, PaseoError> {
        let tcp = TcpStream::connect((address.host(), address.port())).await?;
        if !use_tls {
            return Ok(Stream::Tcp(tcp));
        }
        let server_name = ServerName::try_from(address.host().to_owned()).map_err(|_| {
            EndpointError::TlsServerName {
                host: address.host().to_owned(),
            }
        })?;
        let stream = TlsConnector::from(tls.clone())
            .connect(server_name, tcp)
            .await?;
        Ok(Stream::Tls(Box::new(stream)))
    }

    async fn upgrade(url: &str, stream: Stream) -> Result<Socket, PaseoError> {
        let request = url.into_client_request()?;
        let (socket, _) = client_async(request, stream).await?;
        Ok(socket)
    }

    async fn e2ee_handshake(socket: &mut Socket, channel: &Channel) -> Result<(), PaseoError> {
        let hello = channel.hello_frame();
        loop {
            socket.send(Message::text(hello.clone())).await?;
            let retry = Timer::after(HANDSHAKE_RETRY);
            let next = match future::select(socket.next(), retry).await {
                future::Either::Left((next, _)) => next,
                future::Either::Right(_) => continue,
            };
            let text = match next {
                None | Some(Ok(Message::Close(_))) => {
                    return Err(ProtocolViolation::ClosedDuringHandshake.into());
                }
                Some(Err(error)) => return Err(error.into()),
                Some(Ok(Message::Text(text))) => text,
                Some(Ok(_)) => continue,
            };
            let Ok(ready) = serde_json::from_str::<Ready>(text.as_str()) else {
                continue;
            };
            if ready.kind != "e2ee_ready" {
                continue;
            }
            if !ready.capabilities.binary_ciphertext {
                return Err(ProtocolViolation::MissingBinaryCiphertext.into());
            }
            return Ok(());
        }
    }
}

pub(crate) enum Frame {
    Binary(Vec<u8>),
    Text(String),
}

pub(crate) struct Wire {
    channel: Option<Channel>,
}

impl Wire {
    pub(crate) fn encode(&self, frame: Frame) -> Result<Message, PaseoError> {
        Ok(match (&self.channel, frame) {
            (None, Frame::Text(text)) => Message::text(text),
            (None, Frame::Binary(bytes)) => Message::binary(bytes),
            (Some(channel), Frame::Text(text)) => Message::text(channel.seal_text(&text)?),
            (Some(channel), Frame::Binary(bytes)) => Message::binary(channel.seal(&bytes)?),
        })
    }

    pub(crate) fn decode(&self, message: Message) -> Result<Option<Frame>, PaseoError> {
        match (&self.channel, message) {
            (None, Message::Text(text)) => Ok(Some(Frame::Text(text.as_str().to_owned()))),
            (None, Message::Binary(bytes)) => Ok(Some(Frame::Binary(bytes.into()))),
            (Some(channel), Message::Text(text)) => {
                let text = text.as_str();
                if text.trim_start().starts_with('{') {
                    let ready = serde_json::from_str::<Ready>(text)
                        .map_err(|_| ProtocolViolation::PlaintextOnEncryptedChannel)?;
                    if ready.kind == "e2ee_ready" || ready.kind == "e2ee_hello" {
                        return Ok(None);
                    }
                    return Err(ProtocolViolation::PlaintextOnEncryptedChannel.into());
                }
                Ok(Some(Frame::Text(channel.open_text(text)?)))
            }
            (Some(channel), Message::Binary(bytes)) => {
                Ok(Some(Frame::Binary(channel.open(&bytes)?)))
            }
            (_, Message::Ping(_) | Message::Pong(_) | Message::Frame(_) | Message::Close(_)) => {
                Ok(None)
            }
        }
    }
}
