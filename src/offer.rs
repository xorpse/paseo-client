use base64::Engine;
use base64::engine::general_purpose::{STANDARD, STANDARD_NO_PAD};
use serde::Deserialize;
use url::Url;

use crate::endpoint::HostPort;
use crate::error::{CryptoError, EndpointError};

const OFFER_MARKER: &str = "#offer=";

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RelayInfo {
    endpoint: String,
    #[serde(default)]
    use_tls: Option<bool>,
}

impl RelayInfo {
    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }

    pub fn use_tls(&self) -> bool {
        self.use_tls.unwrap_or(true)
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionOffer {
    server_id: String,
    daemon_public_key_b64: String,
    relay: RelayInfo,
}

impl ConnectionOffer {
    pub fn server_id(&self) -> &str {
        &self.server_id
    }

    pub fn relay(&self) -> &RelayInfo {
        &self.relay
    }

    pub fn parse_url(input: &str) -> Result<ConnectionOffer, EndpointError> {
        let input = input.trim();
        let start = input
            .find(OFFER_MARKER)
            .ok_or_else(|| EndpointError::UnsupportedScheme {
                input: input.to_owned(),
            })?;
        let encoded = input[start + OFFER_MARKER.len()..]
            .trim()
            .trim_end_matches('=')
            .replace('-', "+")
            .replace('_', "/");
        let json = STANDARD_NO_PAD.decode(encoded)?;
        let offer =
            serde_json::from_slice::<ConnectionOffer>(&json).map_err(EndpointError::OfferJson)?;
        if offer.server_id.is_empty() || offer.daemon_public_key_b64.is_empty() {
            return Err(EndpointError::OfferIncomplete);
        }
        Ok(offer)
    }

    pub(crate) fn from_relay_uri(url: &Url, input: &str) -> Result<ConnectionOffer, EndpointError> {
        if !url.username().is_empty() || url.password().is_some() {
            return Err(EndpointError::UnsupportedPart {
                input: input.to_owned(),
                part: "userinfo".to_owned(),
            });
        }
        let host = url.host_str().ok_or_else(|| EndpointError::MissingHost {
            input: input.to_owned(),
        })?;
        let port = url.port().ok_or_else(|| EndpointError::MissingPort {
            input: input.to_owned(),
        })?;
        let server_id = percent_encoding::percent_decode_str(url.path().trim_start_matches('/'))
            .decode_utf8_lossy()
            .into_owned();
        let key = url
            .query_pairs()
            .find(|(name, _)| name == "key")
            .map(|(_, value)| value.into_owned())
            .unwrap_or_default();
        if server_id.is_empty() || server_id.contains('/') || key.is_empty() {
            return Err(EndpointError::OfferIncomplete);
        }
        let use_tls = url
            .query_pairs()
            .any(|(name, value)| name == "ssl" && value == "true");
        let endpoint = HostPort::new(host.trim_start_matches('[').trim_end_matches(']'), port);
        Ok(ConnectionOffer {
            server_id,
            daemon_public_key_b64: key,
            relay: RelayInfo {
                endpoint: endpoint.to_string(),
                use_tls: Some(use_tls),
            },
        })
    }

    pub(crate) fn daemon_public_key(&self) -> Result<[u8; 32], CryptoError> {
        let bytes = STANDARD.decode(self.daemon_public_key_b64.as_bytes())?;
        bytes
            .as_slice()
            .try_into()
            .map_err(|_| CryptoError::PublicKeyLength)
    }

    pub(crate) fn relay_ws_url(&self) -> Result<String, EndpointError> {
        let endpoint = HostPort::parse(&self.relay.endpoint)?;
        let scheme = if self.relay.use_tls() { "wss" } else { "ws" };
        let mut url = Url::parse(&format!("{scheme}://{endpoint}/ws")).map_err(|source| {
            EndpointError::Url {
                input: self.relay.endpoint.clone(),
                source,
            }
        })?;
        url.query_pairs_mut()
            .append_pair("serverId", &self.server_id)
            .append_pair("role", "client")
            .append_pair("v", "2");
        Ok(url.into())
    }
}
