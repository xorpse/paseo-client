use serde_json::Value;

use crate::protocol::diff::CheckoutDiff;
use crate::protocol::{AgentSnapshot, AgentStreamEvent, PermissionRequest, TerminalInfo};

#[derive(Clone, Debug)]
pub enum ConnectionState {
    Connecting,
    Handshaking,
    Connected,
    Disconnected(String),
}

#[derive(Clone, Debug)]
pub enum TerminalStreamEvent {
    Output(Vec<u8>),
    Restore(Vec<u8>),
    Snapshot(Value),
}

#[derive(Clone, Debug)]
pub enum DaemonEvent {
    AgentUpsert(Box<AgentSnapshot>),
    AgentRemove(String),
    AgentStream {
        agent_id: String,
        event: Box<AgentStreamEvent>,
    },
    PermissionRequest {
        agent_id: String,
        request: Box<PermissionRequest>,
    },
    TerminalsChanged {
        terminals: Vec<TerminalInfo>,
    },
    TerminalExit(String),
    CheckoutDiff(Box<CheckoutDiff>),
    Disconnected,
}
