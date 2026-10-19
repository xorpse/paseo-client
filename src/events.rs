use serde::{Deserialize, Serialize};

use crate::protocol::agents::{AgentAttention, PermissionRequested, PermissionResolved};
use crate::protocol::diff::CheckoutStatus;
use crate::protocol::providers::ProvidersSnapshot;
use crate::protocol::subagents::SubagentUpdate;
use crate::protocol::terminals::TerminalAttentionRequired;
use crate::protocol::workspaces::{ProjectUpdate, ScriptStatusUpdate, WorkspaceSetupProgress};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize)]
pub enum EventKind {
    #[serde(rename = "agent_attention_required")]
    AgentAttention,
    #[serde(rename = "agent_permission_request")]
    AgentPermissionRequest,
    #[serde(rename = "agent_permission_resolved")]
    AgentPermissionResolved,
    #[serde(rename = "checkout_status_update")]
    CheckoutStatus,
    #[serde(rename = "project.update")]
    Project,
    #[serde(rename = "providers_snapshot_update")]
    ProvidersSnapshot,
    #[serde(rename = "script_status_update")]
    ScriptStatus,
    #[serde(rename = "agent.provider_subagents.update")]
    Subagent,
    #[serde(rename = "terminal_attention_required")]
    TerminalAttention,
    #[serde(rename = "workspace_setup_progress")]
    WorkspaceSetup,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(tag = "type", content = "payload")]
pub enum SessionEvent {
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
    #[serde(rename = "agent.provider_subagents.update")]
    Subagent(SubagentUpdate),
    #[serde(rename = "terminal_attention_required")]
    TerminalAttention(TerminalAttentionRequired),
    #[serde(rename = "workspace_setup_progress")]
    WorkspaceSetup(Box<WorkspaceSetupProgress>),
}
