use std::collections::HashMap;

use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{Map, Value};

use crate::protocol::number::WireNumber;
use crate::protocol::providers::ProviderAvailability;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DaemonFeature {
    AgentForkFromPrompt,
    AgentHistorySearch,
    AgentOrganization,
    AgentProfiles,
    AgentReview,
    AgentReviewRemovedLines,
    ApnsPush,
    Bots,
    CheckoutCommitBefore,
    CheckoutDiffAll,
    CheckoutDiffCommitBase,
    CheckoutDiscardChanges,
    CheckoutFileAtRevision,
    CheckoutForgeSetAutoMerge,
    CheckoutGithubSetAutoMerge,
    CheckoutRefresh,
    CommitsList,
    CommitsPagination,
    ConnectedHosts,
    CreationLifecycle,
    DaemonConfigReload,
    DaemonDiagnostics,
    DaemonSelfUpdate,
    DaemonStatusRpc,
    ExplicitEventSubscriptions,
    FileByteWrites,
    ForgeCheckDetails,
    ForgeProviders,
    ForgeSearch,
    FsEntryDuplicate,
    FsEntryOps,
    GithubCheckDetails,
    ImportSessionSearch,
    OwnedSubscriptions,
    ProjectCustomIcon,
    ProjectRemove,
    ProviderInstallations,
    ProviderRemoval,
    ProviderSetup,
    ProviderUsageList,
    PushTokenRevocation,
    RelayProtectedSecurity,
    SkillManagement,
    StreamChannelUnixSockets,
    StreamChannels,
    UsageSources,
    WorkspaceFileEditing,
    WorkspaceGithubRepositorySearch,
    WorkspaceLabels,
    WorkspaceMarkUnread,
    WorkspacePinning,
    WorkspaceRecovery,
    WorkspaceSetupObservation,
    WorkspaceSetupRun,
}

impl DaemonFeature {
    pub fn wire_name(self) -> &'static str {
        match self {
            DaemonFeature::AgentForkFromPrompt => "agentForkFromPrompt",
            DaemonFeature::AgentHistorySearch => "agentHistorySearch",
            DaemonFeature::AgentOrganization => "agentOrganization",
            DaemonFeature::AgentProfiles => "agentProfiles",
            DaemonFeature::AgentReview => "agentReview",
            DaemonFeature::AgentReviewRemovedLines => "agentReviewRemovedLines",
            DaemonFeature::ApnsPush => "apnsPush",
            DaemonFeature::Bots => "bots",
            DaemonFeature::CheckoutCommitBefore => "checkoutCommitBefore",
            DaemonFeature::CheckoutDiffAll => "checkoutDiffAll",
            DaemonFeature::CheckoutDiffCommitBase => "checkoutDiffCommitBase",
            DaemonFeature::CheckoutDiscardChanges => "checkoutDiscardChanges",
            DaemonFeature::CheckoutFileAtRevision => "checkoutFileAtRevision",
            DaemonFeature::CheckoutForgeSetAutoMerge => "checkoutForgeSetAutoMerge",
            DaemonFeature::CheckoutGithubSetAutoMerge => "checkoutGithubSetAutoMerge",
            DaemonFeature::CheckoutRefresh => "checkoutRefresh",
            DaemonFeature::CommitsList => "commitsList",
            DaemonFeature::CommitsPagination => "commitsPagination",
            DaemonFeature::ConnectedHosts => "connectedHosts",
            DaemonFeature::CreationLifecycle => "creationLifecycle",
            DaemonFeature::DaemonConfigReload => "daemonConfigReload",
            DaemonFeature::DaemonDiagnostics => "daemonDiagnostics",
            DaemonFeature::DaemonSelfUpdate => "daemonSelfUpdate",
            DaemonFeature::DaemonStatusRpc => "daemonStatusRpc",
            DaemonFeature::ExplicitEventSubscriptions => "explicitEventSubscriptions",
            DaemonFeature::FileByteWrites => "fileByteWrites",
            DaemonFeature::ForgeCheckDetails => "forgeCheckDetails",
            DaemonFeature::ForgeProviders => "forgeProviders",
            DaemonFeature::ForgeSearch => "forgeSearch",
            DaemonFeature::FsEntryDuplicate => "fsEntryDuplicate",
            DaemonFeature::FsEntryOps => "fsEntryOps",
            DaemonFeature::GithubCheckDetails => "githubCheckDetails",
            DaemonFeature::ImportSessionSearch => "importSessionSearch",
            DaemonFeature::OwnedSubscriptions => "ownedSubscriptions",
            DaemonFeature::ProjectCustomIcon => "projectCustomIcon",
            DaemonFeature::ProjectRemove => "projectRemove",
            DaemonFeature::ProviderInstallations => "providerInstallations",
            DaemonFeature::ProviderRemoval => "providerRemoval",
            DaemonFeature::ProviderSetup => "providerSetup",
            DaemonFeature::ProviderUsageList => "providerUsageList",
            DaemonFeature::PushTokenRevocation => "pushTokenRevocation",
            DaemonFeature::RelayProtectedSecurity => "relayProtectedSecurity",
            DaemonFeature::SkillManagement => "skillManagement",
            DaemonFeature::StreamChannelUnixSockets => "streamChannelUnixSockets",
            DaemonFeature::StreamChannels => "streamChannels",
            DaemonFeature::UsageSources => "usageSources",
            DaemonFeature::WorkspaceFileEditing => "workspaceFileEditing",
            DaemonFeature::WorkspaceGithubRepositorySearch => "workspaceGithubRepositorySearch",
            DaemonFeature::WorkspaceLabels => "workspaceLabels",
            DaemonFeature::WorkspaceMarkUnread => "workspaceMarkUnread",
            DaemonFeature::WorkspacePinning => "workspacePinning",
            DaemonFeature::WorkspaceRecovery => "workspaceRecovery",
            DaemonFeature::WorkspaceSetupObservation => "workspaceSetupObservation",
            DaemonFeature::WorkspaceSetupRun => "workspaceSetupRun",
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

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DeviceType {
    Mobile,
    Web,
}

/// What the daemon needs to pick the one client that notifies about an agent: whether the user is
/// present, whether the app is visible, and which agent it shows.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientHeartbeat {
    device_type: DeviceType,
    focused_agent_id: Option<String>,
    focused_terminal_id: Option<String>,
    last_activity_at: String,
    app_visible: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    app_visibility_changed_at: Option<String>,
}

impl ClientHeartbeat {
    pub fn new(
        device_type: DeviceType,
        focused_agent_id: Option<String>,
        last_activity_at: String,
        app_visible: bool,
    ) -> ClientHeartbeat {
        ClientHeartbeat {
            device_type,
            focused_agent_id,
            focused_terminal_id: None,
            last_activity_at,
            app_visible,
            app_visibility_changed_at: None,
        }
    }

    pub fn device_type(&self) -> DeviceType {
        self.device_type
    }

    pub fn focused_agent_id(&self) -> Option<&str> {
        self.focused_agent_id.as_deref()
    }

    pub fn focused_terminal_id(&self) -> Option<&str> {
        self.focused_terminal_id.as_deref()
    }

    pub fn set_focused_terminal_id(&mut self, focused_terminal_id: impl Into<String>) {
        self.focused_terminal_id = Some(focused_terminal_id.into());
    }

    pub fn with_focused_terminal_id(
        mut self,
        focused_terminal_id: impl Into<String>,
    ) -> ClientHeartbeat {
        self.set_focused_terminal_id(focused_terminal_id);
        self
    }

    pub fn last_activity_at(&self) -> &str {
        &self.last_activity_at
    }

    pub fn app_visible(&self) -> bool {
        self.app_visible
    }

    pub fn app_visibility_changed_at(&self) -> Option<&str> {
        self.app_visibility_changed_at.as_deref()
    }

    pub fn set_app_visibility_changed_at(&mut self, changed_at: impl Into<String>) {
        self.app_visibility_changed_at = Some(changed_at.into());
    }

    pub fn with_app_visibility_changed_at(
        mut self,
        changed_at: impl Into<String>,
    ) -> ClientHeartbeat {
        self.set_app_visibility_changed_at(changed_at);
        self
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PairingOffer {
    url: String,
    #[serde(default)]
    qr: Option<String>,
    relay_enabled: bool,
}

impl PairingOffer {
    pub fn url(&self) -> Option<&str> {
        Some(self.url.as_str()).filter(|url| !url.is_empty())
    }

    pub fn qr(&self) -> Option<&str> {
        self.qr.as_deref()
    }

    pub fn relay_enabled(&self) -> bool {
        self.relay_enabled
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum DaemonUpdatePhase {
    Complete,
    Downloading,
    Installing,
    Starting,
    #[serde(other)]
    Unknown,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DaemonUpdated {
    previous_version: Option<String>,
    new_version: Option<String>,
}

impl DaemonUpdated {
    pub fn previous_version(&self) -> Option<&str> {
        self.previous_version.as_deref()
    }

    pub fn new_version(&self) -> Option<&str> {
        self.new_version.as_deref()
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ActivityKind {
    Assistant,
    Error,
    System,
    ToolCall,
    ToolResult,
    Transcript,
    #[serde(other)]
    Unknown,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct ActivityLogEntry {
    id: String,
    timestamp: String,
    #[serde(rename = "type")]
    kind: ActivityKind,
    content: String,
    #[serde(default)]
    metadata: Map<String, Value>,
}

impl ActivityLogEntry {
    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn timestamp(&self) -> &str {
        &self.timestamp
    }

    pub fn kind(&self) -> ActivityKind {
        self.kind
    }

    pub fn content(&self) -> &str {
        &self.content
    }

    pub fn metadata(&self) -> &Map<String, Value> {
        &self.metadata
    }
}
