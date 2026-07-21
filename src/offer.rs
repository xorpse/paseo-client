use base64::Engine;
use serde::Deserialize;
use url::Url;

use crate::error::{PaseoError, Result};

const OFFER_MARKER: &str = "#offer=";

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RelayInfo {
    pub endpoint: String,
    #[serde(default)]
    pub use_tls: Option<bool>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionOffer {
    #[serde(default)]
    pub v: u32,
    pub server_id: String,
    pub daemon_public_key_b64: String,
    pub relay: RelayInfo,
}

impl ConnectionOffer {
    pub fn use_tls(&self) -> bool {
        self.relay.use_tls.unwrap_or(true)
    }
}

fn base64url_decode(input: &str) -> Result<Vec<u8>> {
    let mut normalized = input.replace('-', "+").replace('_', "/");
    normalized.retain(|c| c != '=');
    let padding = (4 - normalized.len() % 4) % 4;
    normalized.push_str(&"=".repeat(padding));
    base64::engine::general_purpose::STANDARD
        .decode(normalized.as_bytes())
        .map_err(|e| PaseoError::Offer(format!("bad base64url: {e}")))
}

pub fn parse_offer_url(url: &str) -> Result<ConnectionOffer> {
    let idx = url
        .find(OFFER_MARKER)
        .ok_or_else(|| PaseoError::Offer("missing #offer= fragment".into()))?;
    let encoded = url[idx + OFFER_MARKER.len()..].trim();
    let json = base64url_decode(encoded)?;
    let offer: ConnectionOffer = serde_json::from_slice(&json)
        .map_err(|e| PaseoError::Offer(format!("bad offer json: {e}")))?;
    if offer.server_id.is_empty() || offer.daemon_public_key_b64.is_empty() {
        return Err(PaseoError::Offer(
            "offer missing serverId/daemonPublicKeyB64".into(),
        ));
    }
    Ok(offer)
}

pub fn decode_daemon_public_key(offer: &ConnectionOffer) -> Result<[u8; 32]> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(offer.daemon_public_key_b64.as_bytes())
        .map_err(|e| PaseoError::Offer(format!("bad daemon key base64: {e}")))?;
    let arr: [u8; 32] = bytes
        .as_slice()
        .try_into()
        .map_err(|_| PaseoError::Offer("daemon key is not 32 bytes".into()))?;
    Ok(arr)
}

pub fn build_relay_ws_url(endpoint: &str, use_tls: bool, server_id: &str) -> Result<String> {
    let scheme = if use_tls { "wss" } else { "ws" };
    let mut url = Url::parse(&format!("{scheme}://{endpoint}/ws"))
        .map_err(|e| PaseoError::Offer(format!("bad relay endpoint {endpoint}: {e}")))?;
    url.query_pairs_mut()
        .append_pair("serverId", server_id)
        .append_pair("role", "client")
        .append_pair("v", "2");
    Ok(url.into())
}

pub fn build_daemon_ws_url(host_port: &str, use_tls: bool) -> Result<String> {
    let scheme = if use_tls { "wss" } else { "ws" };
    let url = Url::parse(&format!("{scheme}://{host_port}/ws"))
        .map_err(|e| PaseoError::Offer(format!("bad daemon endpoint {host_port}: {e}")))?;
    Ok(url.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_offer_url() -> String {
        let json = r#"{"v":2,"serverId":"srv123","daemonPublicKeyB64":"AAAA","relay":{"endpoint":"relay.paseo.sh:443","useTls":true}}"#;
        let b64 = base64::engine::general_purpose::STANDARD.encode(json.as_bytes());
        let b64url = b64.replace('+', "-").replace('/', "_").replace('=', "");
        format!("https://app.paseo.sh/#offer={b64url}")
    }

    #[test]
    fn parses_offer_from_fragment() {
        let offer = parse_offer_url(&sample_offer_url()).expect("parse");
        assert_eq!(offer.v, 2);
        assert_eq!(offer.server_id, "srv123");
        assert_eq!(offer.relay.endpoint, "relay.paseo.sh:443");
        assert!(offer.use_tls());
    }

    #[test]
    fn missing_marker_is_error() {
        assert!(parse_offer_url("https://app.paseo.sh/").is_err());
    }

    #[test]
    fn relay_url_has_client_role_and_v2() {
        let url = build_relay_ws_url("relay.paseo.sh:443", true, "srv123").expect("url");
        assert!(url.starts_with("wss://relay.paseo.sh/ws?"));
        assert!(url.contains("serverId=srv123"));
        assert!(url.contains("role=client"));
        assert!(url.contains("v=2"));

        let custom = build_relay_ws_url("relay.example:8443", true, "s").expect("url");
        assert!(custom.starts_with("wss://relay.example:8443/ws?"));
    }

    #[test]
    fn daemon_url_defaults_to_ws_path() {
        let url = build_daemon_ws_url("127.0.0.1:6767", false).expect("url");
        assert_eq!(url, "ws://127.0.0.1:6767/ws");
    }
}
