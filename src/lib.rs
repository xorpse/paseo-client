mod binary_frames;
mod channel;
mod client;
mod e2ee;
mod endpoint;
mod envelope;
mod error;
mod events;
mod offer;
pub mod protocol;
mod rpc;
mod transport;

pub use binary_frames::ResizeIntent;
pub use channel::{
    ChannelReader, ChannelWriter, NetChannel, NetHandle, NetParts, ProcessChannel, ProcessHandle,
    ProcessParts,
};
pub use client::{
    ConnectOptions, PaseoClient, Subscription, TerminalHandle, TerminalSlot, TerminalWriter,
};
pub use endpoint::{
    ConnectTarget, DEFAULT_SSH_DAEMON_PORT, Endpoint, HostPort, SshTarget, TcpEndpoint,
};
pub use error::{
    ArgumentError, CryptoError, EndpointError, HelloRejection, PaseoError, ProtocolViolation,
    RequestFailure, RpcError,
};
pub use events::{EventKind, SessionEvent};
pub use offer::{ConnectionOffer, RelayInfo};
pub use rpc::AgentCreation;
