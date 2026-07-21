use base64::Engine;
use crypto_box::aead::Aead;
use crypto_box::{Nonce, PublicKey, SalsaBox, SecretKey};

use crate::binary_frames;
use crate::error::{PaseoError, Result};

const NONCE_LEN: usize = 24;

pub enum Decrypted {
    Json(String),
    Binary(Vec<u8>),
}

pub struct Channel {
    sbox: SalsaBox,
    our_public: [u8; 32],
}

fn random_bytes<const N: usize>() -> Result<[u8; N]> {
    let mut buf = [0u8; N];
    getrandom::fill(&mut buf).map_err(|e| PaseoError::Crypto(format!("rng: {e}")))?;
    Ok(buf)
}

fn decode_b64_permissive(input: &str) -> Result<Vec<u8>> {
    let mut normalized = input.replace('-', "+").replace('_', "/");
    normalized.retain(|c| c != '=');
    let padding = (4 - normalized.len() % 4) % 4;
    normalized.push_str(&"=".repeat(padding));
    base64::engine::general_purpose::STANDARD
        .decode(normalized.as_bytes())
        .map_err(|e| PaseoError::Crypto(format!("bad base64: {e}")))
}

impl Channel {
    pub fn new(daemon_public_key: [u8; 32]) -> Result<Channel> {
        let secret_bytes = random_bytes::<32>()?;
        Ok(Channel::from_secret_bytes(daemon_public_key, secret_bytes))
    }

    pub fn from_secret_bytes(daemon_public_key: [u8; 32], secret_bytes: [u8; 32]) -> Channel {
        let secret = SecretKey::from_bytes(secret_bytes);
        let our_public = secret.public_key().to_bytes();
        let their_public = PublicKey::from_bytes(daemon_public_key);
        let sbox = SalsaBox::new(&their_public, &secret);
        Channel { sbox, our_public }
    }

    pub fn hello_frame(&self) -> String {
        let key = base64::engine::general_purpose::STANDARD.encode(self.our_public);
        serde_json::json!({ "type": "e2ee_hello", "key": key }).to_string()
    }

    pub fn encrypt_with_nonce(
        &self,
        nonce_bytes: [u8; NONCE_LEN],
        plaintext: &[u8],
    ) -> Result<String> {
        let nonce = Nonce::from_slice(&nonce_bytes);
        let ciphertext = self
            .sbox
            .encrypt(nonce, plaintext)
            .map_err(|e| PaseoError::Crypto(format!("encrypt: {e}")))?;
        let mut bundle = Vec::with_capacity(NONCE_LEN + ciphertext.len());
        bundle.extend_from_slice(&nonce_bytes);
        bundle.extend_from_slice(&ciphertext);
        Ok(base64::engine::general_purpose::STANDARD.encode(&bundle))
    }

    pub fn encrypt(&self, plaintext: &[u8]) -> Result<String> {
        let nonce_bytes = random_bytes::<NONCE_LEN>()?;
        self.encrypt_with_nonce(nonce_bytes, plaintext)
    }

    pub fn decrypt(&self, b64: &str) -> Result<Decrypted> {
        let bundle = decode_b64_permissive(b64)?;
        if bundle.len() < NONCE_LEN {
            return Err(PaseoError::Crypto("frame shorter than nonce".into()));
        }
        let (nonce_bytes, ciphertext) = bundle.split_at(NONCE_LEN);
        let nonce = Nonce::from_slice(nonce_bytes);
        let plaintext = self
            .sbox
            .decrypt(nonce, ciphertext)
            .map_err(|_| PaseoError::Crypto("decrypt failed".into()))?;
        Ok(classify(plaintext))
    }
}

fn classify(plaintext: Vec<u8>) -> Decrypted {
    if binary_frames::is_terminal_frame(&plaintext) {
        return Decrypted::Binary(plaintext);
    }
    match String::from_utf8(plaintext) {
        Ok(text) => Decrypted::Json(text),
        Err(err) => Decrypted::Binary(err.into_bytes()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn daemon_side(daemon_secret: [u8; 32], client_public: [u8; 32]) -> SalsaBox {
        SalsaBox::new(
            &PublicKey::from_bytes(client_public),
            &SecretKey::from_bytes(daemon_secret),
        )
    }

    #[test]
    fn client_ciphertext_decrypts_on_daemon_side() {
        let daemon_secret = SecretKey::from_bytes([7u8; 32]);
        let daemon_public = daemon_secret.public_key().to_bytes();

        let client = Channel::from_secret_bytes(daemon_public, [9u8; 32]);
        let client_public = SecretKey::from_bytes([9u8; 32]).public_key().to_bytes();

        let b64 = client.encrypt(br#"{"type":"hello"}"#).expect("encrypt");
        let bundle = decode_b64_permissive(&b64).expect("b64");
        let (nonce, ct) = bundle.split_at(NONCE_LEN);

        let plain = daemon_side(daemon_secret.to_bytes(), client_public)
            .decrypt(Nonce::from_slice(nonce), ct)
            .expect("daemon decrypt");
        assert_eq!(plain, br#"{"type":"hello"}"#);
    }

    #[test]
    fn json_and_binary_are_classified() {
        match classify(br#"{"type":"session"}"#.to_vec()) {
            Decrypted::Json(s) => assert!(s.contains("session")),
            Decrypted::Binary(_) => panic!("json misclassified as binary"),
        }
        match classify(vec![binary_frames::OP_OUTPUT, 3, b'{', b'}']) {
            Decrypted::Binary(b) => assert_eq!(b[0], binary_frames::OP_OUTPUT),
            Decrypted::Json(_) => panic!("terminal frame misclassified as json"),
        }
    }

    #[test]
    fn hello_frame_encodes_public_key() {
        let client = Channel::from_secret_bytes([1u8; 32], [2u8; 32]);
        let hello = client.hello_frame();
        assert!(hello.contains("e2ee_hello"));
        let value: serde_json::Value = serde_json::from_str(&hello).expect("json");
        let key = value["key"].as_str().expect("key");
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(key)
            .expect("b64");
        assert_eq!(decoded.len(), 32);
    }
}
