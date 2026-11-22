use async_tungstenite::tungstenite::Message;
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use serde::{Deserialize, Serialize};

use crate::error::{CryptoError, PaseoError, ProtocolViolation};
use crate::transport::Frame;

mod legacy;
mod protected;

const PROTECTED_ENVELOPE_VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum SecurityMode {
    Legacy,
    Protected,
}

#[derive(Deserialize, Serialize)]
struct ProtectedEnvelope {
    v: u32,
    message: String,
}

#[derive(Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct Capabilities {
    #[serde(default)]
    binary_ciphertext: bool,
}

#[derive(Serialize)]
struct Hello<'a> {
    #[serde(rename = "type")]
    kind: &'static str,
    key: &'a str,
    capabilities: Capabilities,
    protected: ProtectedEnvelope,
}

#[derive(Deserialize)]
struct Ready {
    #[serde(default)]
    capabilities: Capabilities,
    #[serde(default)]
    protected: Option<ProtectedEnvelope>,
}

#[derive(Deserialize)]
struct EnvelopeKind<'a> {
    #[serde(rename = "type", borrow)]
    kind: &'a str,
}

pub(crate) struct Handshake {
    legacy: legacy::Channel,
    protected: protected::Initiator,
    minimum: SecurityMode,
    hello: String,
}

impl Handshake {
    pub(crate) fn new(
        daemon_public_key: [u8; 32],
        minimum: SecurityMode,
    ) -> Result<Handshake, CryptoError> {
        let legacy = legacy::Channel::new(daemon_public_key)?;
        let protected = protected::Initiator::new(&daemon_public_key)?;
        let hello = serde_json::to_string(&Hello {
            kind: "e2ee_hello",
            key: &STANDARD.encode(legacy.our_public()),
            capabilities: Capabilities {
                binary_ciphertext: true,
            },
            protected: ProtectedEnvelope {
                v: PROTECTED_ENVELOPE_VERSION,
                message: STANDARD.encode(protected.message()),
            },
        })
        .expect("the e2ee hello serialises");
        Ok(Handshake {
            legacy,
            protected,
            minimum,
            hello,
        })
    }

    pub(crate) fn hello_frame(&self) -> &str {
        &self.hello
    }

    pub(crate) fn finish(self, text: &str) -> Result<Cipher, PaseoError> {
        let ready = serde_json::from_str::<Ready>(text)
            .map_err(|error| PaseoError::decode("e2ee_ready", error))?;
        if let Some(envelope) = ready.protected {
            if envelope.v != PROTECTED_ENVELOPE_VERSION {
                return Err(ProtocolViolation::ProtectedEnvelopeVersion {
                    version: envelope.v,
                }
                .into());
            }
            let reply = STANDARD
                .decode(envelope.message.as_bytes())
                .map_err(CryptoError::from)?;
            return Ok(Cipher::Protected(self.protected.finish(&reply)?));
        }
        if self.minimum == SecurityMode::Protected {
            return Err(ProtocolViolation::LegacySecurityRefused.into());
        }
        if !ready.capabilities.binary_ciphertext {
            return Err(ProtocolViolation::MissingBinaryCiphertext.into());
        }
        Ok(Cipher::Legacy(self.legacy))
    }
}

pub(crate) fn is_ready(text: &str) -> bool {
    serde_json::from_str::<EnvelopeKind<'_>>(text)
        .is_ok_and(|envelope| envelope.kind == "e2ee_ready")
}

pub(crate) enum Cipher {
    Legacy(legacy::Channel),
    Protected(protected::Channel),
}

impl Cipher {
    pub(crate) fn mode(&self) -> SecurityMode {
        match self {
            Cipher::Legacy(_) => SecurityMode::Legacy,
            Cipher::Protected(_) => SecurityMode::Protected,
        }
    }

    pub(crate) fn seal(&self, frame: Frame) -> Result<Message, CryptoError> {
        Ok(match (self, frame) {
            (Cipher::Legacy(channel), Frame::Text(text)) => {
                Message::text(channel.seal_text(&text)?)
            }
            (Cipher::Legacy(channel), Frame::Binary(bytes)) => {
                Message::binary(channel.seal(&bytes)?)
            }
            (Cipher::Protected(channel), frame) => {
                let record = channel.seal(&frame)?;
                match frame {
                    Frame::Binary(_) if channel.binary_ciphertext() => Message::binary(record),
                    _ => Message::text(STANDARD.encode(record)),
                }
            }
        })
    }

    pub(crate) fn open(&self, message: Message) -> Result<Option<Frame>, PaseoError> {
        match message {
            Message::Text(text) => {
                let text = text.as_str();
                if text.trim_start().starts_with('{') {
                    let envelope = serde_json::from_str::<EnvelopeKind<'_>>(text)
                        .map_err(|_| ProtocolViolation::PlaintextOnEncryptedChannel)?;
                    if envelope.kind == "e2ee_ready" || envelope.kind == "e2ee_hello" {
                        return Ok(None);
                    }
                    return Err(ProtocolViolation::PlaintextOnEncryptedChannel.into());
                }
                Ok(Some(match self {
                    Cipher::Legacy(channel) => Frame::Text(channel.open_text(text)?),
                    Cipher::Protected(channel) => {
                        channel.open(&STANDARD.decode(text.trim()).map_err(CryptoError::from)?)?
                    }
                }))
            }
            Message::Binary(bytes) => Ok(Some(match self {
                Cipher::Legacy(channel) => Frame::Binary(channel.open(&bytes)?),
                Cipher::Protected(channel) => channel.open(&bytes)?,
            })),
            Message::Ping(_) | Message::Pong(_) | Message::Frame(_) | Message::Close(_) => Ok(None),
        }
    }
}
