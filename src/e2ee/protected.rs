use std::sync::atomic::{AtomicU64, Ordering};

use snow::params::NoiseParams;
use snow::{Builder, HandshakeState, StatelessTransportState};

use crate::error::{CryptoError, RecordViolation};
use crate::transport::Frame;

const PROTOCOL: &str = "Noise_NK_25519_ChaChaPoly_SHA256";
const PROLOGUE: &[u8] = b"paseo-relay-e2ee/1";
const PARAMS_VERSION: u8 = 0x01;
const PARAM_BINARY_CIPHERTEXT: u8 = 0x01;
const MAX_MESSAGE: usize = 65535;
const TAG_LEN: usize = 16;
const HEADER_LEN: usize = 6;
const LENGTH_PREFIX_LEN: usize = 2;
const MAX_CHUNK_DATA: usize = MAX_MESSAGE - TAG_LEN - HEADER_LEN;
const CHUNK_OVERHEAD: usize = LENGTH_PREFIX_LEN + HEADER_LEN + TAG_LEN;
const KIND_TEXT: u8 = 0x00;
const KIND_BINARY: u8 = 0x01;
const FLAG_FIRST: u8 = 0x01;
const FLAG_FINAL: u8 = 0x02;

pub(crate) struct Initiator {
    handshake: HandshakeState,
    message: Vec<u8>,
}

impl Initiator {
    pub(crate) fn new(daemon_public_key: &[u8; 32]) -> Result<Initiator, CryptoError> {
        Initiator::start(builder(daemon_public_key)?)
    }

    fn start(builder: Builder<'_>) -> Result<Initiator, CryptoError> {
        let mut handshake = builder.build_initiator()?;
        let mut message = vec![0; MAX_MESSAGE];
        let length =
            handshake.write_message(&[PARAMS_VERSION, PARAM_BINARY_CIPHERTEXT], &mut message)?;
        message.truncate(length);
        Ok(Initiator { handshake, message })
    }

    pub(crate) fn message(&self) -> &[u8] {
        &self.message
    }

    pub(crate) fn finish(mut self, reply: &[u8]) -> Result<Channel, CryptoError> {
        let mut payload = vec![0; MAX_MESSAGE];
        let length = self.handshake.read_message(reply, &mut payload)?;
        let binary_ciphertext = match payload[..length] {
            [PARAMS_VERSION, flags] if flags & !PARAM_BINARY_CIPHERTEXT == 0 => {
                flags & PARAM_BINARY_CIPHERTEXT != 0
            }
            _ => return Err(CryptoError::ProtectedParameters),
        };
        Ok(Channel {
            transport: self.handshake.into_stateless_transport_mode()?,
            sent: AtomicU64::new(0),
            received: AtomicU64::new(0),
            binary_ciphertext,
        })
    }
}

fn builder(daemon_public_key: &[u8; 32]) -> Result<Builder<'_>, CryptoError> {
    let params = PROTOCOL
        .parse::<NoiseParams>()
        .expect("the Noise protocol name is valid");
    Ok(Builder::new(params)
        .remote_public_key(daemon_public_key)?
        .prologue(PROLOGUE)?)
}

pub(crate) struct Channel {
    transport: StatelessTransportState,
    sent: AtomicU64,
    received: AtomicU64,
    binary_ciphertext: bool,
}

struct Assembly {
    kind: u8,
    total: usize,
    chunks: usize,
    index: usize,
    frame: Vec<u8>,
}

impl Channel {
    pub(crate) fn binary_ciphertext(&self) -> bool {
        self.binary_ciphertext
    }

    pub(crate) fn seal(&self, frame: &Frame) -> Result<Vec<u8>, CryptoError> {
        let (kind, bytes) = match frame {
            Frame::Binary(bytes) => (KIND_BINARY, bytes.as_slice()),
            Frame::Text(text) => (KIND_TEXT, text.as_bytes()),
        };
        let total = u32::try_from(bytes.len()).map_err(|_| CryptoError::FrameTooLarge)?;
        let chunks = chunk_count(bytes.len());
        let mut record = Vec::with_capacity(record_len(bytes.len()));
        let mut plaintext = Vec::with_capacity(HEADER_LEN + bytes.len().min(MAX_CHUNK_DATA));
        let mut ciphertext = vec![0; MAX_MESSAGE];
        for index in 0..chunks {
            let start = index * MAX_CHUNK_DATA;
            let data = &bytes[start..bytes.len().min(start + MAX_CHUNK_DATA)];
            let first = if index == 0 { FLAG_FIRST } else { 0 };
            let last = if index + 1 == chunks { FLAG_FINAL } else { 0 };
            plaintext.clear();
            plaintext.extend_from_slice(&[kind, first | last]);
            plaintext.extend_from_slice(&total.to_be_bytes());
            plaintext.extend_from_slice(data);
            let nonce = self
                .sent
                .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |nonce| {
                    (nonce < u64::MAX).then_some(nonce + 1)
                })
                .map_err(|_| CryptoError::NonceExhausted)?;
            let length = self
                .transport
                .write_message(nonce, &plaintext, &mut ciphertext)?;
            let prefix = u16::try_from(length).expect("a Noise message fits in a u16 length");
            record.extend_from_slice(&prefix.to_be_bytes());
            record.extend_from_slice(&ciphertext[..length]);
        }
        Ok(record)
    }

    pub(crate) fn open(&self, record: &[u8]) -> Result<Frame, CryptoError> {
        let mut plaintext = vec![0; MAX_MESSAGE];
        let mut assembly = None::<Assembly>;
        let mut offset = 0;
        while offset < record.len() {
            if assembly
                .as_ref()
                .is_some_and(|assembly| assembly.index >= assembly.chunks)
            {
                return Err(RecordViolation::ChunkAfterFinal.into());
            }
            let Some(prefix) = record.get(offset..offset + LENGTH_PREFIX_LEN) else {
                return Err(RecordViolation::Truncated.into());
            };
            let length = usize::from(u16::from_be_bytes([prefix[0], prefix[1]]));
            if length < TAG_LEN + HEADER_LEN {
                return Err(RecordViolation::ChunkLength.into());
            }
            let start = offset + LENGTH_PREFIX_LEN;
            let Some(ciphertext) = record.get(start..start + length) else {
                return Err(RecordViolation::Truncated.into());
            };
            let read = self.open_chunk(ciphertext, &mut plaintext)?;
            offset = start + length;
            let chunk = &plaintext[..read];
            let (kind, flags) = (chunk[0], chunk[1]);
            if flags & !(FLAG_FIRST | FLAG_FINAL) != 0 {
                return Err(RecordViolation::ReservedFlag.into());
            }
            if kind != KIND_TEXT && kind != KIND_BINARY {
                return Err(RecordViolation::Kind.into());
            }
            let total =
                usize::try_from(u32::from_be_bytes([chunk[2], chunk[3], chunk[4], chunk[5]]))
                    .map_err(|_| CryptoError::FrameTooLarge)?;
            let data = &chunk[HEADER_LEN..];
            let assembly = match assembly.as_mut() {
                None => {
                    if flags & FLAG_FIRST == 0 {
                        return Err(RecordViolation::NotFirst.into());
                    }
                    if record_len(total) != record.len() {
                        return Err(RecordViolation::LengthMismatch.into());
                    }
                    assembly.insert(Assembly {
                        kind,
                        total,
                        chunks: chunk_count(total),
                        index: 0,
                        frame: Vec::with_capacity(total),
                    })
                }
                Some(assembly) => {
                    if flags & FLAG_FIRST != 0 {
                        return Err(RecordViolation::FirstRepeated.into());
                    }
                    if kind != assembly.kind {
                        return Err(RecordViolation::KindChanged.into());
                    }
                    if total != assembly.total {
                        return Err(RecordViolation::LengthChanged.into());
                    }
                    assembly
                }
            };
            let last = assembly.index + 1 == assembly.chunks;
            if (flags & FLAG_FINAL != 0) != last {
                return Err(RecordViolation::FinalFlag.into());
            }
            let expected = if last {
                assembly.total - assembly.index * MAX_CHUNK_DATA
            } else {
                MAX_CHUNK_DATA
            };
            if data.len() != expected {
                return Err(RecordViolation::ChunkSize.into());
            }
            assembly.frame.extend_from_slice(data);
            assembly.index += 1;
        }
        let Some(assembly) = assembly else {
            return Err(RecordViolation::Empty.into());
        };
        if assembly.index != assembly.chunks || assembly.frame.len() != assembly.total {
            return Err(RecordViolation::Incomplete.into());
        }
        if assembly.kind == KIND_BINARY {
            return Ok(Frame::Binary(assembly.frame));
        }
        String::from_utf8(assembly.frame)
            .map(Frame::Text)
            .map_err(|_| CryptoError::PlaintextUtf8)
    }

    fn open_chunk(&self, ciphertext: &[u8], plaintext: &mut [u8]) -> Result<usize, CryptoError> {
        let nonce = self.received.load(Ordering::Relaxed);
        if nonce == u64::MAX {
            return Err(CryptoError::NonceExhausted);
        }
        let read = self.transport.read_message(nonce, ciphertext, plaintext)?;
        self.received.store(nonce + 1, Ordering::Relaxed);
        Ok(read)
    }
}

fn chunk_count(frame_len: usize) -> usize {
    frame_len.div_ceil(MAX_CHUNK_DATA).max(1)
}

fn record_len(frame_len: usize) -> usize {
    frame_len + chunk_count(frame_len) * CHUNK_OVERHEAD
}

#[cfg(test)]
mod test {
    use serde::Deserialize;

    use super::{Channel, Initiator, builder};
    use crate::error::{CryptoError, RecordViolation};
    use crate::transport::Frame;

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Fixture {
        daemon_static_public: String,
        client_ephemeral_secret: String,
        message1: String,
        message2: String,
        client_text: String,
        client_text_record: String,
        client_empty_binary_record: String,
        daemon_text: String,
        daemon_text_record: String,
        daemon_binary_record: String,
    }

    fn fixture() -> Fixture {
        serde_json::from_str(include_str!("../../tests/fixtures/relay_protected.json"))
            .expect("the relay fixture parses")
    }

    fn bytes(hex: &str) -> Vec<u8> {
        (0..hex.len())
            .step_by(2)
            .map(|index| u8::from_str_radix(&hex[index..index + 2], 16).expect("fixture hex"))
            .collect()
    }

    fn handshake(fixture: &Fixture) -> (Vec<u8>, Channel) {
        let daemon_key = <[u8; 32]>::try_from(bytes(&fixture.daemon_static_public))
            .expect("a 32-byte daemon key");
        let ephemeral = bytes(&fixture.client_ephemeral_secret);
        let initiator = Initiator::start(
            builder(&daemon_key)
                .expect("builder")
                .fixed_ephemeral_key_for_testing_only(&ephemeral),
        )
        .expect("initiator");
        let message1 = initiator.message().to_vec();
        let channel = initiator
            .finish(&bytes(&fixture.message2))
            .expect("the daemon's reply completes the handshake");
        (message1, channel)
    }

    #[test]
    fn interoperates_with_the_daemon_noise_implementation() {
        let fixture = fixture();
        let (message1, channel) = handshake(&fixture);
        assert_eq!(message1, bytes(&fixture.message1));
        assert!(channel.binary_ciphertext());

        let text = channel
            .seal(&Frame::Text(fixture.client_text.clone()))
            .expect("seal text");
        assert_eq!(text, bytes(&fixture.client_text_record));
        let empty = channel
            .seal(&Frame::Binary(Vec::new()))
            .expect("seal empty");
        assert_eq!(empty, bytes(&fixture.client_empty_binary_record));

        let Frame::Text(received) = channel
            .open(&bytes(&fixture.daemon_text_record))
            .expect("open text")
        else {
            panic!("a text record opens as text");
        };
        assert_eq!(received, fixture.daemon_text);
        let Frame::Binary(binary) = channel
            .open(&bytes(&fixture.daemon_binary_record))
            .expect("open a multi-chunk binary record")
        else {
            panic!("a binary record opens as binary");
        };
        assert_eq!(binary.len(), 70_000);
        assert!(
            binary
                .iter()
                .enumerate()
                .all(|(index, byte)| *byte == ((index * 13) & 0xff) as u8)
        );
    }

    #[test]
    fn rejects_replayed_and_tampered_records() {
        let fixture = fixture();
        let (_, channel) = handshake(&fixture);
        let record = bytes(&fixture.daemon_text_record);
        channel.open(&record).expect("first delivery");
        assert!(matches!(channel.open(&record), Err(CryptoError::Noise(_))));

        let (_, channel) = handshake(&fixture);
        channel.open(&record).expect("first delivery");
        let mut truncated = bytes(&fixture.daemon_binary_record);
        truncated.truncate(truncated.len() - 1);
        assert!(matches!(
            channel.open(&truncated),
            Err(CryptoError::Record(RecordViolation::LengthMismatch))
        ));
    }
}
