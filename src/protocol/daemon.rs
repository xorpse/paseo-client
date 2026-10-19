use std::collections::HashMap;

use serde::{Deserialize, Deserializer};
use serde_json::Value;

use crate::protocol::number::WireNumber;
use crate::protocol::providers::ProviderAvailability;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DaemonFeature {
    DaemonStatusRpc,
    ExplicitEventSubscriptions,
    OwnedSubscriptions,
    StreamChannels,
}

impl DaemonFeature {
    pub fn wire_name(self) -> &'static str {
        match self {
            DaemonFeature::DaemonStatusRpc => "daemonStatusRpc",
            DaemonFeature::ExplicitEventSubscriptions => "explicitEventSubscriptions",
            DaemonFeature::OwnedSubscriptions => "ownedSubscriptions",
            DaemonFeature::StreamChannels => "streamChannels",
        }
    }
}

fn trimmed<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Option::<Value>::deserialize(deserializer)?;
    Ok(match value {
        Some(Value::String(text)) => Some(text.trim().to_owned()).filter(|text| !text.is_empty()),
        _ => None,
    })
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ServerInfo {
    server_id: String,
    #[serde(default, deserialize_with = "trimmed")]
    hostname: Option<String>,
    #[serde(default, deserialize_with = "trimmed")]
    version: Option<String>,
    #[serde(default)]
    protocol_version: Option<i64>,
    #[serde(default)]
    desktop_managed: bool,
    #[serde(default)]
    features: HashMap<String, Value>,
}

impl ServerInfo {
    pub fn server_id(&self) -> &str {
        &self.server_id
    }

    pub fn hostname(&self) -> Option<&str> {
        self.hostname.as_deref()
    }

    pub fn version(&self) -> Option<&str> {
        self.version.as_deref()
    }

    pub fn protocol_version(&self) -> Option<i64> {
        self.protocol_version
    }

    pub fn desktop_managed(&self) -> bool {
        self.desktop_managed
    }

    pub fn supports(&self, feature: DaemonFeature) -> bool {
        self.features.get(feature.wire_name()) == Some(&Value::Bool(true))
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RelayStatus {
    enabled: bool,
    endpoint: String,
    public_endpoint: String,
    use_tls: bool,
    public_use_tls: bool,
}

impl RelayStatus {
    pub fn enabled(&self) -> bool {
        self.enabled
    }

    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }

    pub fn public_endpoint(&self) -> &str {
        &self.public_endpoint
    }

    pub fn use_tls(&self) -> bool {
        self.use_tls
    }

    pub fn public_use_tls(&self) -> bool {
        self.public_use_tls
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DaemonStatus {
    server_id: String,
    #[serde(default)]
    version: Option<String>,
    pid: WireNumber,
    node_path: String,
    #[serde(default)]
    started_at: Option<String>,
    listen: Option<String>,
    #[serde(default)]
    relay: Option<RelayStatus>,
    providers: Vec<ProviderAvailability>,
}

impl DaemonStatus {
    pub fn server_id(&self) -> &str {
        &self.server_id
    }

    pub fn version(&self) -> Option<&str> {
        self.version.as_deref()
    }

    pub fn pid(&self) -> Option<u32> {
        self.pid.whole_u32()
    }

    pub fn node_path(&self) -> &str {
        &self.node_path
    }

    pub fn started_at(&self) -> Option<&str> {
        self.started_at.as_deref()
    }

    pub fn listen(&self) -> Option<&str> {
        self.listen.as_deref()
    }

    pub fn relay(&self) -> Option<&RelayStatus> {
        self.relay.as_ref()
    }

    pub fn providers(&self) -> &[ProviderAvailability] {
        &self.providers
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct UploadedFile {
    id: String,
    file_name: String,
    path: String,
}

impl UploadedFile {
    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn file_name(&self) -> &str {
        &self.file_name
    }

    pub fn path(&self) -> &str {
        &self.path
    }
}
