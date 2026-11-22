#[cfg(feature = "ssh")]
use std::collections::VecDeque;
use std::io;
use std::path::Path;
use std::pin::Pin;
use std::sync::{Arc, Weak};
use std::task::{Context, Poll};
use std::time::Duration;

use async_io::Timer;
use async_net::TcpStream;
#[cfg(unix)]
use async_net::unix::UnixStream;
#[cfg(feature = "ssh")]
use async_process::{Child, ChildStderr, ChildStdin, ChildStdout, Command, Stdio};
use async_tungstenite::tungstenite::client::IntoClientRequest;
use async_tungstenite::tungstenite::protocol::WebSocketConfig;
use async_tungstenite::tungstenite::{self, Message};
use async_tungstenite::{WebSocketStream, client_async_with_config};
use flume::r#async::RecvStream;
use futures::io::{AsyncRead, AsyncWrite};
use futures::{Sink, SinkExt, Stream as FrameStream, StreamExt, future};
use futures_rustls::TlsConnector;
use futures_rustls::client::TlsStream;
use futures_rustls::pki_types::ServerName;
use futures_rustls::rustls::ClientConfig;
#[cfg(feature = "ssh")]
use parking_lot::Mutex;
use serde::Serialize;

use crate::binary_frames::HostRouteFrame;
use crate::client::Inner;
use crate::e2ee::{Cipher, Handshake, SecurityMode, is_ready};
use crate::endpoint::{Endpoint, HostPort, SshTarget};
use crate::envelope::session_message;
#[cfg(feature = "ssh")]
use crate::error::SshTunnelClosed;
use crate::error::{EndpointError, HostRouteClosed, PaseoError, ProtocolViolation};

const HANDSHAKE_RETRY: Duration = Duration::from_secs(1);
const MAX_MESSAGE_BYTES: usize = 100 << 20;
#[cfg(feature = "ssh")]
const SSH_STDERR_LIMIT: usize = 8192;

pub(crate) enum Stream {
    #[cfg(feature = "ssh")]
    Ssh(SshStream),
    Tcp(TcpStream),
    Tls(Box<TlsStream<TcpStream>>),
    #[cfg(unix)]
    Unix(UnixStream),
}

#[cfg(feature = "ssh")]
pub(crate) struct SshStream {
    _child: Child,
    stdin: ChildStdin,
    stdout: ChildStdout,
    stderr: Option<ChildStderr>,
    diagnostics: Arc<Mutex<SshDiagnostics>>,
}

#[cfg(feature = "ssh")]
#[derive(Default)]
struct SshDiagnostics {
    tail: VecDeque<u8>,
    line: Vec<u8>,
}

#[cfg(feature = "ssh")]
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
            #[cfg(feature = "ssh")]
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
            #[cfg(feature = "ssh")]
            Stream::Ssh(stream) => Pin::new(&mut stream.stdin).poll_write(cx, buf),
            Stream::Tcp(stream) => Pin::new(stream).poll_write(cx, buf),
            Stream::Tls(stream) => Pin::new(stream.as_mut()).poll_write(cx, buf),
            #[cfg(unix)]
            Stream::Unix(stream) => Pin::new(stream).poll_write(cx, buf),
        }
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match self.get_mut() {
            #[cfg(feature = "ssh")]
            Stream::Ssh(stream) => Pin::new(&mut stream.stdin).poll_flush(cx),
            Stream::Tcp(stream) => Pin::new(stream).poll_flush(cx),
            Stream::Tls(stream) => Pin::new(stream.as_mut()).poll_flush(cx),
            #[cfg(unix)]
            Stream::Unix(stream) => Pin::new(stream).poll_flush(cx),
        }
    }

    fn poll_close(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match self.get_mut() {
            #[cfg(feature = "ssh")]
            Stream::Ssh(stream) => Pin::new(&mut stream.stdin).poll_close(cx),
            Stream::Tcp(stream) => Pin::new(stream).poll_close(cx),
            Stream::Tls(stream) => Pin::new(stream.as_mut()).poll_close(cx),
            #[cfg(unix)]
            Stream::Unix(stream) => Pin::new(stream).poll_close(cx),
        }
    }
}

#[cfg(feature = "ssh")]
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

pub(crate) enum Socket {
    Direct(Box<WebSocketStream<Stream>>),
    Routed(RoutedSocket),
}

pub(crate) enum RouteEvent {
    Closed(HostRouteClosed),
    Frame(Frame),
}

pub(crate) struct RoutedSocket {
    route_id: String,
    origin: Weak<Inner>,
    events: RecvStream<'static, RouteEvent>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RouteFrameRequest<'a> {
    route_id: &'a str,
    data: &'a str,
    binary: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RouteClose<'a> {
    route_id: &'a str,
}

impl RoutedSocket {
    pub(crate) fn new(
        route_id: String,
        origin: &Arc<Inner>,
        events: flume::Receiver<RouteEvent>,
    ) -> RoutedSocket {
        RoutedSocket {
            route_id,
            origin: Arc::downgrade(origin),
            events: events.into_stream(),
        }
    }

    fn send(&self, message: Message) -> Result<(), tungstenite::Error> {
        let unsendable = |error: serde_json::Error| tungstenite::Error::Io(io::Error::other(error));
        let frame = match message {
            Message::Text(text) => Frame::Text(
                session_message(
                    "hosts.route.frame.request",
                    None,
                    &RouteFrameRequest {
                        route_id: &self.route_id,
                        data: text.as_str(),
                        binary: false,
                    },
                )
                .map_err(unsendable)?,
            ),
            Message::Binary(bytes) => Frame::Binary(
                HostRouteFrame {
                    route_id: &self.route_id,
                    payload: &bytes,
                }
                .encode(),
            ),
            Message::Close(_) => Frame::Text(
                session_message(
                    "hosts.route.close.request",
                    None,
                    &RouteClose {
                        route_id: &self.route_id,
                    },
                )
                .map_err(unsendable)?,
            ),
            Message::Ping(_) | Message::Pong(_) | Message::Frame(_) => return Ok(()),
        };
        self.origin
            .upgrade()
            .ok_or(tungstenite::Error::AlreadyClosed)?
            .send(frame)
            .map_err(|_| tungstenite::Error::AlreadyClosed)
    }
}

impl Drop for RoutedSocket {
    fn drop(&mut self) {
        if let Some(origin) = self.origin.upgrade() {
            origin.release(&self.route_id);
        }
    }
}

impl FrameStream for Socket {
    type Item = Result<Message, tungstenite::Error>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        match self.get_mut() {
            Socket::Direct(socket) => socket.poll_next_unpin(cx),
            Socket::Routed(socket) => socket.events.poll_next_unpin(cx).map(|event| {
                event.map(|event| match event {
                    RouteEvent::Frame(Frame::Text(text)) => Ok(Message::text(text)),
                    RouteEvent::Frame(Frame::Binary(bytes)) => Ok(Message::binary(bytes)),
                    RouteEvent::Closed(closed) => Err(tungstenite::Error::Io(io::Error::new(
                        io::ErrorKind::ConnectionAborted,
                        closed,
                    ))),
                })
            }),
        }
    }
}

impl Sink<Message> for Socket {
    type Error = tungstenite::Error;

    fn poll_ready(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        match self.get_mut() {
            Socket::Direct(socket) => socket.poll_ready_unpin(cx),
            Socket::Routed(_) => Poll::Ready(Ok(())),
        }
    }

    fn start_send(self: Pin<&mut Self>, message: Message) -> Result<(), Self::Error> {
        match self.get_mut() {
            Socket::Direct(socket) => socket.start_send_unpin(message),
            Socket::Routed(socket) => socket.send(message),
        }
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        match self.get_mut() {
            Socket::Direct(socket) => socket.poll_flush_unpin(cx),
            Socket::Routed(_) => Poll::Ready(Ok(())),
        }
    }

    fn poll_close(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        match self.get_mut() {
            Socket::Direct(socket) => socket.poll_close_unpin(cx),
            Socket::Routed(_) => Poll::Ready(Ok(())),
        }
    }
}

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
            Endpoint::Ssh(target) => Self::ssh(target).await,
            Endpoint::Relay(offer) => {
                let handshake = Handshake::new(
                    offer.daemon_public_key()?,
                    offer
                        .minimum_security_mode()
                        .unwrap_or(SecurityMode::Legacy),
                )?;
                let address = HostPort::parse(offer.relay().endpoint())?;
                let stream = Self::tcp(&address, offer.relay().use_tls(), tls).await?;
                let mut socket = Self::upgrade(&offer.relay_ws_url()?, stream).await?;
                let cipher = Self::e2ee_handshake(&mut socket, handshake).await?;
                Ok(Connection {
                    socket,
                    wire: Wire {
                        cipher: Some(cipher),
                    },
                })
            }
        }
    }

    pub(crate) fn plain(socket: Socket) -> Connection {
        Connection {
            socket,
            wire: Wire { cipher: None },
        }
    }

    #[cfg(feature = "ssh")]
    async fn ssh(target: &SshTarget) -> Result<Connection, PaseoError> {
        let stream = SshStream::spawn(target)?;
        let diagnostics = stream.diagnostics.clone();
        match Self::upgrade(&target.ws_url(), Stream::Ssh(stream)).await {
            Ok(socket) => Ok(Connection::plain(socket)),
            Err(error) => {
                let detail = diagnostics.lock().detail();
                if detail.is_empty() {
                    return Err(error);
                }
                Err(PaseoError::Ssh { detail })
            }
        }
    }

    #[cfg(not(feature = "ssh"))]
    async fn ssh(target: &SshTarget) -> Result<Connection, PaseoError> {
        Err(EndpointError::SshUnsupported {
            input: format!("ssh://{}", target.destination()),
        }
        .into())
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
        let config = WebSocketConfig::default()
            .max_message_size(Some(MAX_MESSAGE_BYTES))
            .max_frame_size(Some(MAX_MESSAGE_BYTES));
        let (socket, _) = client_async_with_config(request, stream, Some(config)).await?;
        Ok(Socket::Direct(Box::new(socket)))
    }

    async fn e2ee_handshake(
        socket: &mut Socket,
        handshake: Handshake,
    ) -> Result<Cipher, PaseoError> {
        loop {
            socket
                .send(Message::text(handshake.hello_frame().to_owned()))
                .await?;
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
            if is_ready(text.as_str()) {
                return handshake.finish(text.as_str());
            }
        }
    }
}

pub(crate) enum Frame {
    Binary(Vec<u8>),
    Text(String),
}

pub(crate) struct Wire {
    cipher: Option<Cipher>,
}

impl Wire {
    pub(crate) fn security_mode(&self) -> Option<SecurityMode> {
        self.cipher.as_ref().map(Cipher::mode)
    }

    pub(crate) fn encode(&self, frame: Frame) -> Result<Message, PaseoError> {
        Ok(match (&self.cipher, frame) {
            (None, Frame::Text(text)) => Message::text(text),
            (None, Frame::Binary(bytes)) => Message::binary(bytes),
            (Some(cipher), frame) => cipher.seal(frame)?,
        })
    }

    pub(crate) fn decode(&self, message: Message) -> Result<Option<Frame>, PaseoError> {
        match (&self.cipher, message) {
            (_, Message::Ping(_) | Message::Pong(_) | Message::Frame(_) | Message::Close(_)) => {
                Ok(None)
            }
            (None, Message::Text(text)) => Ok(Some(Frame::Text(text.as_str().to_owned()))),
            (None, Message::Binary(bytes)) => Ok(Some(Frame::Binary(bytes.into()))),
            (Some(cipher), message) => cipher.open(message),
        }
    }
}
