use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use crypto_box::aead::Aead;
use crypto_box::{Nonce, PublicKey, SalsaBox, SecretKey};
use serde::{Deserialize, Serialize};

use crate::error::CryptoError;

const NONCE_LEN: usize = 24;

pub(crate) struct Channel {
    sealed_box: SalsaBox,
    our_public: [u8; 32],
}

#[derive(Serialize)]
struct Hello<'a> {
    #[serde(rename = "type")]
    kind: &'static str,
    key: &'a str,
    capabilities: Capabilities,
}

#[derive(Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Capabilities {
    #[serde(default)]
    pub(crate) binary_ciphertext: bool,
}

#[derive(Deserialize)]
pub(crate) struct Ready {
    #[serde(rename = "type")]
    pub(crate) kind: String,
    #[serde(default)]
    pub(crate) capabilities: Capabilities,
}

impl Channel {
    pub(crate) fn new(daemon_public_key: [u8; 32]) -> Result<Channel, CryptoError> {
        let mut secret = [0u8; 32];
        getrandom::fill(&mut secret).map_err(CryptoError::Random)?;
        let secret = SecretKey::from_bytes(secret);
        Ok(Channel {
            our_public: secret.public_key().to_bytes(),
            sealed_box: SalsaBox::new(&PublicKey::from_bytes(daemon_public_key), &secret),
        })
    }

    pub(crate) fn hello_frame(&self) -> String {
        let key = STANDARD.encode(self.our_public);
        let hello = Hello {
            kind: "e2ee_hello",
            key: &key,
            capabilities: Capabilities {
                binary_ciphertext: true,
            },
        };
        serde_json::to_string(&hello).expect("the e2ee hello serialises")
    }

    pub(crate) fn seal(&self, plaintext: &[u8]) -> Result<Vec<u8>, CryptoError> {
        let mut nonce = [0u8; NONCE_LEN];
        getrandom::fill(&mut nonce).map_err(CryptoError::Random)?;
        let ciphertext = self
            .sealed_box
            .encrypt(Nonce::from_slice(&nonce), plaintext)
            .map_err(|_| CryptoError::Encrypt)?;
        let mut bundle = Vec::with_capacity(NONCE_LEN + ciphertext.len());
        bundle.extend_from_slice(&nonce);
        bundle.extend_from_slice(&ciphertext);
        Ok(bundle)
    }

    pub(crate) fn seal_text(&self, plaintext: &str) -> Result<String, CryptoError> {
        Ok(STANDARD.encode(self.seal(plaintext.as_bytes())?))
    }

    pub(crate) fn open(&self, bundle: &[u8]) -> Result<Vec<u8>, CryptoError> {
        if bundle.len() < NONCE_LEN {
            return Err(CryptoError::FrameTooShort);
        }
        let (nonce, ciphertext) = bundle.split_at(NONCE_LEN);
        self.sealed_box
            .decrypt(Nonce::from_slice(nonce), ciphertext)
            .map_err(|_| CryptoError::Decrypt)
    }

    pub(crate) fn open_text(&self, encoded: &str) -> Result<String, CryptoError> {
        let plaintext = self.open(&STANDARD.decode(encoded.trim())?)?;
        String::from_utf8(plaintext).map_err(|_| CryptoError::PlaintextUtf8)
    }
}
