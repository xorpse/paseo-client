use thiserror::Error;

#[derive(Debug, Error)]
pub enum PaseoError {
    #[error("connection closed")]
    Closed,

    #[error("request timed out")]
    Timeout,

    #[error("rpc error: {0}")]
    Rpc(String),

    #[error("invalid pairing offer: {0}")]
    Offer(String),

    #[error("handshake failed: {0}")]
    Handshake(String),

    #[error("crypto failure: {0}")]
    Crypto(String),

    #[error("protocol violation: {0}")]
    Protocol(String),

    #[error("websocket error: {0}")]
    WebSocket(String),

    #[error(transparent)]
    Json(#[from] serde_json::Error),

    #[error(transparent)]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, PaseoError>;
