use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use async_trait::async_trait;
use async_tungstenite::tungstenite::client::IntoClientRequest;
use async_tungstenite::tungstenite::http::header::{AUTHORIZATION, SEC_WEBSOCKET_PROTOCOL};
use async_tungstenite::tungstenite::http::HeaderValue;
use async_tungstenite::tungstenite::Message;
use async_tungstenite::{client_async, WebSocketStream};
use futures::lock::Mutex as AsyncMutex;
use futures_util::io::{AsyncRead, AsyncWrite};
use futures_util::stream::{SplitSink, SplitStream};
use futures_util::{SinkExt, StreamExt};
use url::Url;

use crate::e2ee::{Channel, Decrypted};
use crate::error::{PaseoError, Result};
use crate::offer::build_daemon_ws_url;

pub enum Frame {
    Json(String),
    Binary(Vec<u8>),
}

#[async_trait]
pub trait Transport: Send + Sync {
    async fn send_text(&self, text: String) -> Result<()>;
    async fn send_binary(&self, bytes: Vec<u8>) -> Result<()>;
    async fn recv(&self) -> Result<Option<Frame>>;
    async fn close(&self);
}

enum MaybeTls {
    Plain(async_net::TcpStream),
    Tls(Box<futures_rustls::client::TlsStream<async_net::TcpStream>>),
}

impl AsyncRead for MaybeTls {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<std::io::Result<usize>> {
        match self.get_mut() {
            MaybeTls::Plain(s) => Pin::new(s).poll_read(cx, buf),
            MaybeTls::Tls(s) => Pin::new(s.as_mut()).poll_read(cx, buf),
        }
    }
}

impl AsyncWrite for MaybeTls {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        match self.get_mut() {
            MaybeTls::Plain(s) => Pin::new(s).poll_write(cx, buf),
            MaybeTls::Tls(s) => Pin::new(s.as_mut()).poll_write(cx, buf),
        }
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        match self.get_mut() {
            MaybeTls::Plain(s) => Pin::new(s).poll_flush(cx),
            MaybeTls::Tls(s) => Pin::new(s.as_mut()).poll_flush(cx),
        }
    }

    fn poll_close(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        match self.get_mut() {
            MaybeTls::Plain(s) => Pin::new(s).poll_close(cx),
            MaybeTls::Tls(s) => Pin::new(s.as_mut()).poll_close(cx),
        }
    }
}

type WsStream = WebSocketStream<MaybeTls>;

fn ws_err<E: std::fmt::Display>(e: E) -> PaseoError {
    PaseoError::WebSocket(e.to_string())
}

async fn tls_connect(
    host: &str,
    tcp: async_net::TcpStream,
) -> Result<futures_rustls::client::TlsStream<async_net::TcpStream>> {
    let mut roots = futures_rustls::rustls::RootCertStore::empty();
    roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    let config = futures_rustls::rustls::ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth();
    let connector = futures_rustls::TlsConnector::from(Arc::new(config));
    let server_name = futures_rustls::rustls::pki_types::ServerName::try_from(host.to_owned())
        .map_err(|e| PaseoError::WebSocket(format!("bad tls server name: {e}")))?;
    connector
        .connect(server_name, tcp)
        .await
        .map_err(|e| PaseoError::WebSocket(format!("tls: {e}")))
}

async fn open_ws(url: &str, password: Option<&str>) -> Result<WsStream> {
    let parsed = Url::parse(url).map_err(|e| PaseoError::WebSocket(format!("bad url: {e}")))?;
    let use_tls = parsed.scheme() == "wss";
    let host = parsed
        .host_str()
        .ok_or_else(|| PaseoError::WebSocket("url missing host".into()))?
        .to_owned();
    let port = parsed
        .port_or_known_default()
        .ok_or_else(|| PaseoError::WebSocket("url missing port".into()))?;

    let tcp = async_net::TcpStream::connect((host.as_str(), port))
        .await
        .map_err(|e| PaseoError::WebSocket(format!("connect {host}:{port}: {e}")))?;

    let stream = if use_tls {
        MaybeTls::Tls(Box::new(tls_connect(&host, tcp).await?))
    } else {
        MaybeTls::Plain(tcp)
    };

    let mut request = url
        .into_client_request()
        .map_err(|e| PaseoError::WebSocket(format!("request: {e}")))?;
    if let Some(pw) = password {
        let bearer = HeaderValue::from_str(&format!("Bearer {pw}")).map_err(ws_err)?;
        let subproto = HeaderValue::from_str(&format!("paseo.bearer.{pw}")).map_err(ws_err)?;
        request.headers_mut().insert(AUTHORIZATION, bearer);
        request
            .headers_mut()
            .insert(SEC_WEBSOCKET_PROTOCOL, subproto);
    }

    let (ws, _response) = client_async(request, stream).await.map_err(ws_err)?;
    Ok(ws)
}

pub struct WsTransport {
    write: AsyncMutex<SplitSink<WsStream, Message>>,
    read: AsyncMutex<SplitStream<WsStream>>,
    e2ee: Option<Channel>,
}

impl WsTransport {
    pub async fn connect_local(
        host_port: &str,
        use_tls: bool,
        password: Option<&str>,
    ) -> Result<Arc<WsTransport>> {
        let url = build_daemon_ws_url(host_port, use_tls)?;
        let ws = open_ws(&url, password).await?;
        let (write, read) = ws.split();
        Ok(Arc::new(WsTransport {
            write: AsyncMutex::new(write),
            read: AsyncMutex::new(read),
            e2ee: None,
        }))
    }

    pub async fn connect_relay(url: &str, daemon_public_key: [u8; 32]) -> Result<Arc<WsTransport>> {
        let ws = open_ws(url, None).await?;
        let (mut write, mut read) = ws.split();
        let channel = Channel::new(daemon_public_key)?;
        write
            .send(Message::text(channel.hello_frame()))
            .await
            .map_err(ws_err)?;

        loop {
            match read.next().await {
                None => return Err(PaseoError::Handshake("closed before e2ee_ready".into())),
                Some(Err(e)) => return Err(ws_err(e)),
                Some(Ok(Message::Text(text))) => {
                    if let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) {
                        match value.get("type").and_then(|v| v.as_str()) {
                            Some("e2ee_ready") => break,
                            _ => continue,
                        }
                    }
                }
                Some(Ok(Message::Close(_))) => {
                    return Err(PaseoError::Handshake("closed before e2ee_ready".into()))
                }
                Some(Ok(_)) => continue,
            }
        }

        Ok(Arc::new(WsTransport {
            write: AsyncMutex::new(write),
            read: AsyncMutex::new(read),
            e2ee: Some(channel),
        }))
    }
}

#[async_trait]
impl Transport for WsTransport {
    async fn send_text(&self, text: String) -> Result<()> {
        let msg = match &self.e2ee {
            Some(channel) => Message::text(channel.encrypt(text.as_bytes())?),
            None => Message::text(text),
        };
        self.write.lock().await.send(msg).await.map_err(ws_err)
    }

    async fn send_binary(&self, bytes: Vec<u8>) -> Result<()> {
        let msg = match &self.e2ee {
            Some(channel) => Message::text(channel.encrypt(&bytes)?),
            None => Message::binary(bytes),
        };
        self.write.lock().await.send(msg).await.map_err(ws_err)
    }

    async fn recv(&self) -> Result<Option<Frame>> {
        loop {
            let next = self.read.lock().await.next().await;
            match next {
                None => return Ok(None),
                Some(Err(e)) => return Err(ws_err(e)),
                Some(Ok(Message::Text(text))) => match &self.e2ee {
                    Some(channel) => match channel.decrypt(&text)? {
                        Decrypted::Json(s) => return Ok(Some(Frame::Json(s))),
                        Decrypted::Binary(b) => return Ok(Some(Frame::Binary(b))),
                    },
                    None => return Ok(Some(Frame::Json(text))),
                },
                Some(Ok(Message::Binary(bytes))) => match &self.e2ee {
                    Some(_) => continue,
                    None => return Ok(Some(Frame::Binary(bytes))),
                },
                Some(Ok(Message::Close(_))) => return Ok(None),
                Some(Ok(_)) => continue,
            }
        }
    }

    async fn close(&self) {
        let _ = self.write.lock().await.close().await;
    }
}
