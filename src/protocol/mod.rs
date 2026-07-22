pub mod agents;
pub mod diff;
pub mod stream;
pub mod subagents;
pub mod terminals;
pub mod timeline;
pub mod workspaces;

use std::collections::HashMap;

pub use agents::{AgentListEntry, AgentSnapshot, PermissionRequest, PermissionResponse};
use serde::Deserialize;
use serde_json::Value;
pub use stream::{AgentStreamEvent, AgentUpdate};
pub use subagents::{ProviderSubagent, SubagentUpdate};
pub use terminals::TerminalInfo;
pub use timeline::{TimelineItem, ToolCallDetail};

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerInfo {
    #[serde(default)]
    pub server_id: String,
    #[serde(default)]
    pub hostname: Option<String>,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub desktop_managed: bool,
    #[serde(default)]
    pub features: HashMap<String, Value>,
    #[serde(default)]
    pub capabilities: HashMap<String, Value>,
}

impl ServerInfo {
    pub fn feature_enabled(&self, name: &str) -> bool {
        self.features
            .get(name)
            .and_then(Value::as_bool)
            .unwrap_or(false)
    }
}
