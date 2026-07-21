use serde::Deserialize;

use crate::protocol::agents::{AgentSnapshot, PermissionRequest};
use crate::protocol::timeline::TimelineItem;

#[derive(Clone, Debug, Deserialize)]
pub struct AgentStreamEvent {
    #[serde(rename = "type", default)]
    pub kind: String,
    #[serde(default)]
    pub item: Option<TimelineItem>,
    #[serde(default)]
    pub request: Option<PermissionRequest>,
    #[serde(default)]
    pub reason: Option<String>,
    #[serde(default)]
    pub provider: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct AgentUpdate {
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub agent: Option<AgentSnapshot>,
    #[serde(rename = "agentId", default)]
    pub agent_id: Option<String>,
}
