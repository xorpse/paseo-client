use std::{fmt, io};

use async_tungstenite::tungstenite;
use serde::Deserialize;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum PaseoError {
    #[error("invalid argument: {0}")]
    Argument(#[from] ArgumentError),
    #[error("connection closed")]
    Closed,
    #[error("crypto failure: {0}")]
    Crypto(#[from] CryptoError),
    #[error("failed to decode {message_type}: {source}")]
    Decode {
        message_type: String,
        source: serde_json::Error,
    },
    #[error("invalid endpoint: {0}")]
    Endpoint(#[from] EndpointError),
    #[error("daemon rejected hello: {0}")]
    HelloRejected(HelloRejection),
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error("liveness check failed")]
    LivenessTimeout,
    #[error("protocol violation: {0}")]
    Protocol(#[from] ProtocolViolation),
    #[error("{request_type} failed: {failure}")]
    Request {
        request_type: &'static str,
        failure: RequestFailure,
    },
    #[error("{0}")]
    Rpc(RpcError),
    #[error("ssh tunnel failed: {detail}")]
    Ssh { detail: String },
    #[error("{request_type} timed out")]
    Timeout { request_type: &'static str },
    #[error("daemon does not support {feature}; update the host")]
    UnsupportedDaemon { feature: &'static str },
    #[error(transparent)]
    WebSocket(#[from] tungstenite::Error),
}

impl PaseoError {
    pub fn is_connection_lost(&self) -> bool {
        matches!(
            self,
            PaseoError::Closed
                | PaseoError::Io(_)
                | PaseoError::LivenessTimeout
                | PaseoError::Ssh { .. }
                | PaseoError::WebSocket(_)
        )
    }

    pub(crate) fn decode(message_type: impl Into<String>, source: serde_json::Error) -> PaseoError {
        PaseoError::Decode {
            message_type: message_type.into(),
            source,
        }
    }

    pub(crate) fn request(request_type: &'static str, failure: RequestFailure) -> PaseoError {
        PaseoError::Request {
            request_type,
            failure,
        }
    }

    pub(crate) fn rpc(request_type: &str, message: impl Into<String>) -> PaseoError {
        PaseoError::Rpc(RpcError {
            request_type: request_type.to_owned(),
            message: message.into(),
            code: None,
        })
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RpcError {
    #[serde(default)]
    request_type: String,
    #[serde(rename = "error")]
    message: String,
    #[serde(default)]
    code: Option<String>,
}

impl RpcError {
    pub fn request_type(&self) -> &str {
        &self.request_type
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    pub fn code(&self) -> Option<&str> {
        self.code.as_deref()
    }

    pub(crate) fn with_request_type(mut self, request_type: &str) -> RpcError {
        if self.request_type.is_empty() {
            request_type.clone_into(&mut self.request_type);
        }
        self
    }
}

impl fmt::Display for RpcError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} failed: {}", self.request_type, self.message)
    }
}

#[derive(Debug, Error)]
pub enum RequestFailure {
    #[error("the daemon declined the request")]
    Declined,
    #[error("the file changed while it was being written")]
    FileChanged,
    #[error("the daemon replied without the {0}")]
    MissingResult(&'static str),
    #[error("{path:?} exists and is not a directory")]
    NotADirectory { path: String },
    #[error("not found")]
    NotFound,
    #[error("the daemon replied with status {0:?}")]
    UnexpectedStatus(String),
}

#[derive(Debug, Error)]
pub enum ArgumentError {
    #[error("{name:?} is not a valid file name")]
    FileNameInvalid { name: String },
    #[error("{name} must be between 1 and {max}, not {value}")]
    LimitOutOfRange {
        name: &'static str,
        value: u32,
        max: u32,
    },
    #[error("only a process running on a pseudo-terminal can be resized")]
    NotPseudoTerminal,
    #[error("{path:?} is not a relative path inside the workspace")]
    PathOutsideWorkspace { path: String },
    #[error("a title must not be blank")]
    TitleBlank,
    #[error("a title must be at most {max} UTF-16 code units, not {length}")]
    TitleTooLong { length: usize, max: usize },
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum HelloRejection {
    IncompatibleProtocol,
    IncorrectPassword,
    PasswordRequired,
    #[serde(other)]
    Unrecognised,
}

impl fmt::Display for HelloRejection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            HelloRejection::IncompatibleProtocol => "incompatible protocol version",
            HelloRejection::IncorrectPassword => "incorrect password",
            HelloRejection::PasswordRequired => "password required",
            HelloRejection::Unrecognised => "unrecognised reason",
        })
    }
}

#[derive(Debug, Error)]
pub enum CryptoError {
    #[error("invalid base64: {0}")]
    Base64(#[from] base64::DecodeError),
    #[error("decryption failed")]
    Decrypt,
    #[error("encryption failed")]
    Encrypt,
    #[error("ciphertext shorter than its nonce")]
    FrameTooShort,
    #[error("decrypted text frame is not UTF-8")]
    PlaintextUtf8,
    #[error("daemon public key is not 32 bytes")]
    PublicKeyLength,
    #[error("random number generation failed: {0}")]
    Random(getrandom::Error),
}

#[derive(Debug, Error)]
pub enum EndpointError {
    #[error("{input:?} needs a home directory, but HOME is not set")]
    HomeUnavailable { input: String },
    #[error("{input:?} has an invalid port")]
    InvalidPort { input: String },
    #[error("{input:?} is not a valid ssh host")]
    InvalidSshHost { input: String },
    #[error("{input:?} is missing a host")]
    MissingHost { input: String },
    #[error("{input:?} is missing a port")]
    MissingPort { input: String },
    #[error("invalid base64 in pairing offer: {0}")]
    OfferBase64(#[from] base64::DecodeError),
    #[error("pairing offer is missing its serverId or daemon key")]
    OfferIncomplete,
    #[error("invalid pairing offer JSON: {0}")]
    OfferJson(serde_json::Error),
    #[error("invalid TLS server name {host:?}")]
    TlsServerName { host: String },
    #[error("unix socket endpoints are not supported on this platform")]
    UnixUnsupported,
    #[error("{input:?} has unsupported part {part:?}")]
    UnsupportedPart { input: String, part: String },
    #[error("{input:?} uses an unsupported scheme")]
    UnsupportedScheme { input: String },
    #[error("invalid URL {input:?}: {source}")]
    Url {
        input: String,
        source: url::ParseError,
    },
}

#[derive(Debug, Error)]
#[error("ssh tunnel closed: {detail}")]
pub(crate) struct SshTunnelClosed {
    pub(crate) detail: String,
}

#[derive(Debug, Error)]
pub enum ProtocolViolation {
    #[error("connection closed before the handshake completed")]
    ClosedDuringHandshake,
    #[error("git checkout status without {0}")]
    IncompleteCheckoutStatus(&'static str),
    #[error("relay did not negotiate binary ciphertext frames")]
    MissingBinaryCiphertext,
    #[error("{response_type} carried no subscription id")]
    MissingSubscriptionId { response_type: &'static str },
    #[error("plaintext frame on an encrypted channel")]
    PlaintextOnEncryptedChannel,
}
