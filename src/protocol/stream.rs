use serde::Deserialize;

use crate::protocol::agents::{
    AgentUsage, AttentionNotification, AttentionReason, PermissionRequest, PermissionResponse,
};
use crate::protocol::timeline::TimelineItem;

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AgentStreamEvent {
    AttentionRequired(AttentionRequiredEvent),
    PermissionRequested(PermissionRequestedEvent),
    PermissionResolved(PermissionResolvedEvent),
    ThreadStarted(ThreadStartedEvent),
    Timeline(TimelineEvent),
    TurnCanceled(TurnCanceledEvent),
    TurnCompleted(TurnCompletedEvent),
    TurnFailed(TurnFailedEvent),
    TurnStarted(TurnStartedEvent),
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AttentionRequiredEvent {
    provider: String,
    reason: AttentionReason,
    timestamp: String,
    should_notify: bool,
    #[serde(default)]
    notification: Option<AttentionNotification>,
}

impl AttentionRequiredEvent {
    pub fn provider(&self) -> &str {
        &self.provider
    }

    pub fn reason(&self) -> AttentionReason {
        self.reason
    }

    pub fn timestamp(&self) -> &str {
        &self.timestamp
    }

    pub fn should_notify(&self) -> bool {
        self.should_notify
    }

    pub fn notification(&self) -> Option<&AttentionNotification> {
        self.notification.as_ref()
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct PermissionRequestedEvent {
    provider: String,
    request: Box<PermissionRequest>,
}

impl PermissionRequestedEvent {
    pub fn provider(&self) -> &str {
        &self.provider
    }

    pub fn request(&self) -> &PermissionRequest {
        &self.request
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PermissionResolvedEvent {
    provider: String,
    request_id: String,
    resolution: PermissionResponse,
}

impl PermissionResolvedEvent {
    pub fn provider(&self) -> &str {
        &self.provider
    }

    pub fn request_id(&self) -> &str {
        &self.request_id
    }

    pub fn resolution(&self) -> &PermissionResponse {
        &self.resolution
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ThreadStartedEvent {
    provider: String,
    session_id: String,
}

impl ThreadStartedEvent {
    pub fn provider(&self) -> &str {
        &self.provider
    }

    pub fn session_id(&self) -> &str {
        &self.session_id
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TimelineEvent {
    provider: String,
    item: TimelineItem,
    #[serde(default)]
    turn_id: Option<String>,
}

impl TimelineEvent {
    pub fn provider(&self) -> &str {
        &self.provider
    }

    pub fn item(&self) -> &TimelineItem {
        &self.item
    }

    pub fn turn_id(&self) -> Option<&str> {
        self.turn_id.as_deref()
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TurnCanceledEvent {
    provider: String,
    #[serde(default)]
    turn_id: Option<String>,
    reason: String,
}

impl TurnCanceledEvent {
    pub fn provider(&self) -> &str {
        &self.provider
    }

    pub fn turn_id(&self) -> Option<&str> {
        self.turn_id.as_deref()
    }

    pub fn reason(&self) -> &str {
        &self.reason
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TurnCompletedEvent {
    provider: String,
    #[serde(default)]
    turn_id: Option<String>,
    #[serde(default)]
    usage: Option<AgentUsage>,
}

impl TurnCompletedEvent {
    pub fn provider(&self) -> &str {
        &self.provider
    }

    pub fn turn_id(&self) -> Option<&str> {
        self.turn_id.as_deref()
    }

    pub fn usage(&self) -> Option<&AgentUsage> {
        self.usage.as_ref()
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TurnFailedEvent {
    provider: String,
    #[serde(default)]
    turn_id: Option<String>,
    error: String,
    #[serde(default)]
    code: Option<String>,
    #[serde(default)]
    diagnostic: Option<String>,
}

impl TurnFailedEvent {
    pub fn provider(&self) -> &str {
        &self.provider
    }

    pub fn turn_id(&self) -> Option<&str> {
        self.turn_id.as_deref()
    }

    pub fn error(&self) -> &str {
        &self.error
    }

    pub fn code(&self) -> Option<&str> {
        self.code.as_deref()
    }

    pub fn diagnostic(&self) -> Option<&str> {
        self.diagnostic.as_deref()
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TurnStartedEvent {
    provider: String,
    #[serde(default)]
    turn_id: Option<String>,
}

impl TurnStartedEvent {
    pub fn provider(&self) -> &str {
        &self.provider
    }

    pub fn turn_id(&self) -> Option<&str> {
        self.turn_id.as_deref()
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AgentStreamMessage {
    agent_id: String,
    event: AgentStreamEvent,
    timestamp: String,
    #[serde(default)]
    seq: Option<u64>,
    #[serde(default)]
    epoch: Option<String>,
}

impl AgentStreamMessage {
    pub fn agent_id(&self) -> &str {
        &self.agent_id
    }

    pub fn event(&self) -> &AgentStreamEvent {
        &self.event
    }

    pub fn timestamp(&self) -> &str {
        &self.timestamp
    }

    pub fn seq(&self) -> Option<u64> {
        self.seq
    }

    pub fn epoch(&self) -> Option<&str> {
        self.epoch.as_deref()
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TimelineReplacement {
    agent_id: String,
    epoch: String,
}

impl TimelineReplacement {
    pub fn agent_id(&self) -> &str {
        &self.agent_id
    }

    pub fn epoch(&self) -> &str {
        &self.epoch
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(tag = "type", content = "payload")]
pub enum TimelineUpdate {
    #[serde(rename = "agent.timeline.replacement")]
    Replacement(TimelineReplacement),
    #[serde(rename = "agent_stream")]
    Stream(Box<AgentStreamMessage>),
}
