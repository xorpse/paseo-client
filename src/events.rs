use serde::{Deserialize, Serialize};

use crate::protocol::agents::{AgentAttention, PermissionRequested, PermissionResolved};
use crate::protocol::config::DaemonConfig;
use crate::protocol::daemon::{ActivityLogEntry, ServerInfo};
use crate::protocol::diff::CheckoutStatus;
use crate::protocol::providers::ProvidersSnapshot;
use crate::protocol::subagents::SubagentUpdate;
use crate::protocol::terminals::TerminalAttentionRequired;
use crate::protocol::workspaces::{ProjectUpdate, ScriptStatusUpdate, WorkspaceSetupProgress};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize)]
pub enum EventKind {
    #[serde(rename = "activity_log")]
    ActivityLog,
    #[serde(rename = "agent_attention_required")]
    AgentAttention,
    #[serde(rename = "agent_permission_request")]
    AgentPermissionRequest,
    #[serde(rename = "agent_permission_resolved")]
    AgentPermissionResolved,
    #[serde(rename = "checkout_status_update")]
    CheckoutStatus,
    #[serde(rename = "status.daemon_config_changed")]
    DaemonConfigChanged,
    #[serde(rename = "project.update")]
    Project,
    #[serde(rename = "providers_snapshot_update")]
    ProvidersSnapshot,
    #[serde(rename = "script_status_update")]
    ScriptStatus,
    #[serde(rename = "status.server_info")]
    ServerInfo,
    #[serde(rename = "agent.provider_subagents.update")]
    Subagent,
    #[serde(rename = "terminal_attention_required")]
    TerminalAttention,
    #[serde(rename = "workspace_setup_progress")]
    WorkspaceSetup,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum StatusEvent {
    DaemonConfigChanged {
        config: Box<DaemonConfig>,
    },
    ServerInfo(Box<ServerInfo>),
    #[serde(other)]
    Unknown,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(tag = "type", content = "payload")]
pub enum SessionEvent {
    #[serde(rename = "activity_log")]
    ActivityLog(ActivityLogEntry),
    #[serde(rename = "agent_attention_required")]
    AgentAttention(AgentAttention),
    #[serde(rename = "agent_permission_request")]
    AgentPermissionRequest(Box<PermissionRequested>),
    #[serde(rename = "agent_permission_resolved")]
    AgentPermissionResolved(PermissionResolved),
    #[serde(rename = "checkout_status_update")]
    CheckoutStatus(CheckoutStatus),
    #[serde(rename = "project.update")]
    Project(ProjectUpdate),
    #[serde(rename = "providers_snapshot_update")]
    ProvidersSnapshot(ProvidersSnapshot),
    #[serde(rename = "script_status_update")]
    ScriptStatus(ScriptStatusUpdate),
    #[serde(rename = "status")]
    Status(StatusEvent),
    #[serde(rename = "agent.provider_subagents.update")]
    Subagent(SubagentUpdate),
    #[serde(rename = "terminal_attention_required")]
    TerminalAttention(TerminalAttentionRequired),
    #[serde(rename = "workspace_setup_progress")]
    WorkspaceSetup(Box<WorkspaceSetupProgress>),
}
