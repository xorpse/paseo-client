pub mod binary_frames;
pub mod client;
pub mod e2ee;
pub mod envelope;
pub mod error;
pub mod events;
pub mod offer;
pub mod protocol;
pub mod transport;

pub use client::{CreatedWorkspace, PaseoClient, TerminalHandle, TerminalWriter};
pub use error::{PaseoError, Result};
pub use events::{ConnectionState, DaemonEvent, TerminalStreamEvent};
pub use offer::{parse_offer_url, ConnectionOffer};
pub use protocol::agents::{
    AgentFeature, AgentListEntry, AgentMode, ModelDefinition, PermissionRequest,
    PermissionResponse, SelectOption, SlashCommand,
};
pub use protocol::diff::{CheckoutDiff, CheckoutStatus, DiffFile, DiffHunk, DiffLine};
pub use protocol::stream::AgentStreamEvent;
pub use protocol::terminals::CreateTerminalOpts;
pub use protocol::timeline::{TimelineItem, ToolCallDetail};
pub use protocol::workspaces::Workspace;
pub use protocol::{AgentSnapshot, ServerInfo, TerminalInfo};
