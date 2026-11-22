use std::collections::HashMap;
use std::num::NonZeroU64;
use std::pin::pin;
use std::sync::Arc;
use std::time::Duration;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use futures::channel::oneshot;
use futures::future::{self, Either};
use serde::de::{DeserializeOwned, Error as _};
use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;
use serde_json::{Map, Value};

use crate::binary_frames::FileTransferFrame;
use crate::channel::{Channel, ChannelLease, NetChannel, ProcessChannel, Termination};
use crate::client::{
    CONNECT_TIMEOUT, ConnectOptions, Download, Inner, PaseoClient, Payload, PendingReply,
    REQUEST_TIMEOUT, Request, Routed, Subscription, TerminalHandle, TerminalSlot, Tracking, timed,
};
use crate::envelope::{NoFields, PayloadHead, session_message};
use crate::error::{ArgumentError, PaseoError, ProtocolViolation, RequestFailure};
use crate::events::{EventKind, SessionEvent};
use crate::protocol::agents::{
    AgentActionOutcome, AgentAttachment, AgentConfigPatch, AgentCreateRequest, AgentDirectoryEntry,
    AgentDirectoryPage, AgentDirectoryQuery, AgentFeature, AgentMessage, AgentMode,
    AgentPersistenceHandle, AgentSessionConfig, AgentSnapshot, AgentUpdate, CreationKind,
    CreationSnapshot, DraftConfig, ForkedContext, ImageAttachment, ImportAgentRequest,
    ModelDefinition, PermissionResponse, PromptPreview, ProviderNotice, RewindMode, SlashCommand,
    WaitOutcome,
};
use crate::protocol::channels::{NetClosed, ProcessExit, ProcessSpawn};
use crate::protocol::checkout::{
    BranchSource, BranchValidation, CheckDetails, CheckDetailsQuery, CommitFile, CommitList,
    CommitPage, CreatedPullRequest, ForgeAuthState, ForgeItem, ForgeItemKind, ForgeSearchResults,
    ForgeStatus, MergeMethod, MergeStrategy, PullRequestTimeline, RepositorySearch, StashEntry,
};
use crate::protocol::config::{
    ConfigReload, DaemonConfig, DaemonConfigPatch, ProjectConfig, ProjectConfigError,
    ProjectConfigRevision,
};
use crate::protocol::daemon::{
    ClientHeartbeat, DaemonFeature, DaemonStatus, DaemonUpdatePhase, DaemonUpdated, PairingOffer,
    UploadedFile,
};
use crate::protocol::diff::{
    CheckoutDiff, CheckoutError, CheckoutStatus, DiffCompare, DiffFile, DiffMode,
};
use crate::protocol::files::{
    ContentEncoding, DirectoryListing, DownloadToken, ExplorerFile, FileContent, FileEntry,
    FileRevision, FileState, FileVersion, FileWrite, FileWriteOutcome,
};
use crate::protocol::hosts::{HostAdd, HostDirectory, HostSummary, HostUpdate};
use crate::protocol::labels::{
    EditedLabel, LabelAssignment, LabelCatalogue, LabelCursor, LabelEdit, LabelUpdate,
    WorkspaceLabel,
};
use crate::protocol::providers::{
    ProviderAvailability, ProviderInstallations, ProviderSetup, ProviderSetupAction,
    ProvidersSnapshot, RecentProviderSessions, RecentSessionsQuery,
};
use crate::protocol::recognised::recognised_entries;
use crate::protocol::review::{
    AgentReview, CommentSide, FileReference, ReviewCommentInput, SeenFile,
};
use crate::protocol::schedules::{
    Schedule, ScheduleCreate, ScheduleDetail, ScheduleRun, ScheduleUpdate,
};
use crate::protocol::skills::{ImportedSkills, SavedSkills, SkillSelection, SkillsStatus};
use crate::protocol::stream::TimelineUpdate;
use crate::protocol::subagents::{ProviderSubagent, SubagentTimelinePage, SubagentTimelineQuery};
use crate::protocol::terminals::{
    CreateTerminalOptions, TerminalInfo, TerminalRestore, TerminalStreamEvent, TerminalsChanged,
};
use crate::protocol::timeline::{AgentTimelinePage, TimelineCursor, TimelineQuery, TimelineSearch};
use crate::protocol::usage::{ProviderUsageList, UsageReportEntry, UsageReportsQuery};
use crate::protocol::workspaces::{
    AttentionCleared, ClosedItems, CreatedWorkspace, DirectorySuggestion,
    DirectorySuggestionsQuery, EntryKind, PaseoWorktree, ProjectDescriptor, ProjectIcon,
    ProjectPlacement, WorkspaceCreated, WorkspaceDescriptor, WorkspaceDirectoryPage,
    WorkspaceDirectoryQuery, WorkspaceRecovery, WorkspaceScript, WorkspaceSetupProgress,
    WorkspaceSetupSnapshot, WorkspaceSource, WorkspaceUpdate, WorktreeAction,
};
use crate::transport::{Connection, Frame, RoutedSocket, Socket};

const GITHUB_CLONE_TIMEOUT: Duration = Duration::from_secs(5 * 60);
const PROVIDER_CATALOGUE_TIMEOUT: Duration = Duration::from_secs(90);
const MAX_WAIT_FOR_FINISH_MS: u64 = 2_147_483_647;
const UPLOAD_CHUNK_BYTES: usize = 128 * 1024;
const FILE_TRANSFER_TIMEOUT: Duration = Duration::from_secs(5 * 60);
const FORGE_SEARCH_MAX_LIMIT: u32 = 50;
const CHECKOUT_MUTATION_TIMEOUT: Duration = Duration::from_secs(5 * 60);
const DAEMON_UPDATE_PROGRESS: &str = "daemon.update.progress";
const USAGE_REPORT_UPDATE: &str = "usage.list_reports.update";

#[derive(Serialize)]
#[serde(untagged)]
enum NetTarget<'a> {
    Tcp { host: &'a str, port: u16 },
    Unix { path: &'a str },
}

#[derive(Clone, Copy)]
struct Rpc {
    kind: &'static str,
    response: &'static str,
}

impl Rpc {
    const fn new(kind: &'static str, response: &'static str) -> Rpc {
        Rpc { kind, response }
    }

    fn required<T>(self, value: Option<T>, result: &'static str) -> Result<T, PaseoError> {
        value.ok_or_else(|| PaseoError::request(self.kind, RequestFailure::MissingResult(result)))
    }

    fn accepted(self, accepted: bool) -> Result<(), PaseoError> {
        if accepted {
            return Ok(());
        }
        Err(PaseoError::request(self.kind, RequestFailure::Declined))
    }
}

trait Response: DeserializeOwned {
    fn failure(&self) -> Option<&str> {
        None
    }

    fn rejection(&self) -> Option<&CheckoutError> {
        None
    }
}

macro_rules! response {
    ($($name:ident { $($(#[$attribute:meta])* $field:ident: $ty:ty),* $(,)? })*) => {
        $(
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct $name {
                $(#[serde(default)] $(#[$attribute])* $field: $ty,)*
                #[serde(default)]
                error: Option<String>,
            }

            impl Response for $name {
                fn failure(&self) -> Option<&str> {
                    self.error.as_deref()
                }
            }
        )*
    };
}

response! {
    Accepted { accepted: bool }
    ActionResponse { agent_id: String, accepted: bool, notice: Option<ProviderNotice> }
    AgentArchived { archived_at: String }
    AgentCreated { agent: Option<AgentSnapshot> }
    AgentFetched { agent: Option<AgentSnapshot>, project: Option<ProjectPlacement> }
    AttentionClearedAll { success: bool, results: Vec<AttentionCleared> }
    BranchSuggestions { branches: Vec<String> }
    CancelResponse {}
    CommandsListed { commands: Vec<SlashCommand> }
    ConfigFetched { config: Option<DaemonConfig> }
    CreationSubscribed { snapshot: Option<CreationSnapshot> }
    Diagnosed { diagnostic: Option<String> }
    DirectoryCreated { directory_path: Option<String> }
    DirectorySuggestions { entries: Vec<DirectorySuggestion> }
    EntryCreated { path: Option<String>, success: bool }
    EntryDeleted { success: bool }
    EntryDuplicated { duplicated_path: Option<String>, success: bool }
    EntryRenamed { renamed_path: Option<String>, success: bool }
    Explored { directory: Option<DirectoryListing>, file: Option<ExplorerFile> }
    FileUploaded { file: Option<UploadedFile> }
    ForgeSearched {
        #[serde(deserialize_with = "recognised_entries")]
        items: Vec<ForgeItem>,
        auth_state: Option<ForgeAuthState>,
        features_enabled: Option<bool>,
        github_features_enabled: Option<bool>,
    }
    GithubCloned { checkout_path: Option<String>, project: Option<ProjectDescriptor> }
    HostSaved { host: Option<HostSummary> }
    LabelCount { affected_workspace_count: u64 }
    MarkedUnread { success: bool, marked_agent_id: Option<String> }
    ModelsListed { models: Vec<ModelDefinition> }
    ModesListed { modes: Vec<AgentMode> }
    Pinned { accepted: bool, pinned_at: Option<String> }
    ProjectAdded { project: Option<ProjectDescriptor> }
    ProjectIconFetched { icon: Option<WireProjectIcon> }
    ProjectOpened { workspace: Option<WorkspaceDescriptor> }
    ProjectRemoved { accepted: bool, removed_workspace_ids: Vec<String> }
    ProjectRenamed { accepted: bool, custom_name: Option<String> }
    PromptsListed { prompts: Vec<PromptPreview> }
    ProvidersListed { providers: Vec<ProviderAvailability> }
    RecoveryInspected { state: Option<WorkspaceRecovery> }
    RewindResponse { ok: bool }
    ScheduleFetched { schedule: Option<Schedule> }
    ScheduleInspected { schedule: Option<ScheduleDetail> }
    ScheduleRunsListed { runs: Vec<ScheduleRun> }
    SchedulesListed { schedules: Vec<Schedule> }
    ScriptsListed { scripts: Vec<WorkspaceScript> }
    SendResponse { accepted: bool }
    SetupStarted { started: bool }
    SetupStatus { snapshot: Option<WorkspaceSetupSnapshot> }
    SubagentsListed { subagents: Vec<ProviderSubagent> }
    TerminalCreated { terminal: Option<TerminalInfo> }
    TerminalRenamed { success: bool }
    TitleSet { accepted: bool }
    WorkspaceArchived { archived_at: Option<String> }
}

impl Response for AgentDirectoryPage {}
impl Response for WorkspaceDirectoryPage {}
impl Response for ProvidersSnapshot {}
impl Response for ProviderInstallations {}
impl Response for ProviderSetup {}
impl Response for TerminalsChanged {}
impl Response for TerminalSlot {}
impl Response for DaemonStatus {}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct StatusPayload {
    status: String,
    #[serde(default)]
    agent: Option<AgentSnapshot>,
    #[serde(default)]
    error: Option<String>,
}

impl Response for StatusPayload {
    fn failure(&self) -> Option<&str> {
        self.error.as_deref()
    }
}

#[derive(Clone, Copy, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
enum ServerStatus {
    RestartRequested,
    ShutdownRequested,
    #[serde(other)]
    Unrecognised,
}

#[derive(Deserialize)]
struct ServerStatusReply {
    status: ServerStatus,
}

impl Response for ServerStatusReply {}

impl ServerStatusReply {
    fn confirm(&self, rpc: Rpc, expected: ServerStatus) -> Result<(), PaseoError> {
        if self.status == expected {
            return Ok(());
        }
        Err(PaseoError::request(
            rpc.kind,
            RequestFailure::UnrecognisedOutcome,
        ))
    }
}

#[derive(Deserialize)]
struct Failure {
    #[serde(default)]
    error: Option<String>,
}

impl Response for Failure {
    fn failure(&self) -> Option<&str> {
        self.error.as_deref()
    }
}

#[derive(Deserialize)]
struct CheckoutDone {
    #[serde(default)]
    success: bool,
}

#[derive(Deserialize)]
struct PullRequestCreated {
    #[serde(default)]
    url: Option<String>,
    #[serde(default)]
    number: Option<u64>,
}

#[derive(Deserialize)]
struct ReviewSubscribed {
    review: AgentReview,
}

#[derive(Deserialize)]
struct ReviewUpdate {
    review: AgentReview,
}

impl Response for CheckoutDiff {}
impl Response for CheckoutStatus {}
impl Response for ForgeStatus {}
impl Response for PullRequestTimeline {}
impl Response for RecentProviderSessions {}
impl Response for RepositorySearch {}
impl Response for WaitOutcome {}

#[derive(Deserialize)]
struct ProjectsListed {
    projects: Vec<ProjectDescriptor>,
}

impl Response for ProjectsListed {}

#[derive(Deserialize)]
struct TerminalsListed {
    terminals: Vec<TerminalInfo>,
}

impl Response for TerminalsListed {}

#[derive(Deserialize)]
struct Acknowledged {}

impl Response for Acknowledged {}

#[derive(Deserialize)]
struct RefreshAcknowledged {
    acknowledged: bool,
}

impl Response for RefreshAcknowledged {}

impl Response for WorkspaceCreated {
    fn failure(&self) -> Option<&str> {
        self.error.as_deref()
    }
}

#[derive(Deserialize)]
struct FeaturesListed {
    #[serde(default, deserialize_with = "recognised_entries")]
    features: Vec<AgentFeature>,
    #[serde(default)]
    error: Option<String>,
}

impl Response for FeaturesListed {
    fn failure(&self) -> Option<&str> {
        self.error.as_deref()
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WireProjectIcon {
    data: String,
    mime_type: String,
}

#[derive(Deserialize)]
struct FileWritten {
    result: FileWriteOutcome,
}

impl Response for FileWritten {}

#[derive(Deserialize)]
struct FileSubscribed {
    initial: FileVersion,
}

#[derive(Deserialize)]
struct HostsListed {
    directory: HostDirectory,
}

impl Response for HostsListed {}

#[derive(Deserialize)]
struct Checked<T> {
    #[serde(flatten)]
    value: T,
    #[serde(default)]
    error: Option<String>,
}

impl<T: DeserializeOwned> Response for Checked<T> {
    fn failure(&self) -> Option<&str> {
        self.error.as_deref()
    }
}

#[derive(Deserialize)]
struct CheckoutResult<T> {
    #[serde(flatten)]
    value: T,
    #[serde(default)]
    error: Option<CheckoutError>,
}

impl<T: DeserializeOwned> Response for CheckoutResult<T> {
    fn rejection(&self) -> Option<&CheckoutError> {
        self.error.as_ref()
    }
}

#[derive(Deserialize)]
struct CheckDetailsFetched {
    #[serde(default)]
    details: Option<CheckDetails>,
}

#[derive(Deserialize)]
struct CommitFound {
    #[serde(default)]
    commit: Option<String>,
}

#[derive(Deserialize)]
struct FileAtRevisionFetched {
    #[serde(default)]
    content: Option<String>,
}

#[derive(Deserialize)]
struct FileDiffFetched {
    #[serde(default)]
    file: Option<DiffFile>,
}

#[derive(Deserialize)]
struct BranchSwitched {
    #[serde(default)]
    success: bool,
    #[serde(default)]
    source: Option<BranchSource>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct BranchRenamed {
    #[serde(default)]
    success: bool,
    #[serde(default)]
    current_branch: Option<String>,
}

#[derive(Deserialize)]
struct StashesListed {
    #[serde(default)]
    entries: Vec<StashEntry>,
}

#[derive(Deserialize)]
struct ProjectConfigReply {
    #[serde(flatten)]
    config: ProjectConfig,
    #[serde(default)]
    error: Option<ProjectConfigError>,
}

impl Response for ProjectConfigReply {}

#[derive(Deserialize)]
struct DaemonUpdateFinished {
    #[serde(default)]
    success: bool,
    #[serde(flatten)]
    versions: DaemonUpdated,
    #[serde(default)]
    error: Option<String>,
}

impl Response for DaemonUpdateFinished {
    fn failure(&self) -> Option<&str> {
        self.error.as_deref()
    }
}

#[derive(Deserialize)]
struct FileUpdated {
    version: FileVersion,
}

#[derive(Deserialize)]
struct HostsChanged {
    directory: HostDirectory,
}

#[derive(Deserialize)]
struct UsageReported {
    report: UsageReportEntry,
}

#[derive(Deserialize)]
struct UpdateProgressed {
    phase: DaemonUpdatePhase,
}

impl Routed for FileVersion {
    fn decode(kind: &str, message: &RawValue) -> Result<FileVersion, PaseoError> {
        decode_payload::<FileUpdated>("fs.file.update", kind, message).map(|update| update.version)
    }
}

impl Routed for LabelUpdate {
    fn decode(kind: &str, message: &RawValue) -> Result<LabelUpdate, PaseoError> {
        decode_payload("workspace.label.update", kind, message)
    }
}

impl Routed for WorkspaceSetupProgress {
    fn decode(kind: &str, message: &RawValue) -> Result<WorkspaceSetupProgress, PaseoError> {
        decode_payload("workspace_setup_progress", kind, message)
    }
}

impl Routed for CreationSnapshot {
    fn decode(kind: &str, message: &RawValue) -> Result<CreationSnapshot, PaseoError> {
        let snapshot = serde_json::from_str::<Payload<CreationSnapshot>>(message.get())
            .map_err(|error| PaseoError::decode(kind, error))?
            .payload;
        if snapshot.kind().update_type() != Some(kind) {
            return Err(foreign_message(kind));
        }
        Ok(snapshot)
    }
}

impl Routed for HostDirectory {
    fn decode(kind: &str, message: &RawValue) -> Result<HostDirectory, PaseoError> {
        decode_payload::<HostsChanged>("hosts.changed", kind, message)
            .map(|changed| changed.directory)
    }
}

impl Routed for UsageReportEntry {
    fn decode(kind: &str, message: &RawValue) -> Result<UsageReportEntry, PaseoError> {
        decode_payload::<UsageReported>(USAGE_REPORT_UPDATE, kind, message)
            .map(|reported| reported.report)
    }
}

impl Routed for DaemonUpdatePhase {
    fn decode(kind: &str, message: &RawValue) -> Result<DaemonUpdatePhase, PaseoError> {
        decode_payload::<UpdateProgressed>(DAEMON_UPDATE_PROGRESS, kind, message)
            .map(|progressed| progressed.phase)
    }
}

fn decode_payload<T: DeserializeOwned>(
    expected: &str,
    kind: &str,
    message: &RawValue,
) -> Result<T, PaseoError> {
    if kind != expected {
        return Err(foreign_message(kind));
    }
    serde_json::from_str::<Payload<T>>(message.get())
        .map(|message| message.payload)
        .map_err(|error| PaseoError::decode(kind, error))
}

fn foreign_message(kind: &str) -> PaseoError {
    PaseoError::decode(
        kind,
        serde_json::Error::custom("message type does not belong to this subscription"),
    )
}

fn check_limit(name: &'static str, value: u32, max: u32) -> Result<(), ArgumentError> {
    if !(1..=max).contains(&value) {
        return Err(ArgumentError::LimitOutOfRange { name, value, max });
    }
    Ok(())
}

impl Routed for AgentUpdate {
    fn decode(kind: &str, message: &RawValue) -> Result<AgentUpdate, PaseoError> {
        decode_payload("agent_update", kind, message)
    }
}

impl Routed for WorkspaceUpdate {
    fn decode(kind: &str, message: &RawValue) -> Result<WorkspaceUpdate, PaseoError> {
        decode_payload("workspace_update", kind, message)
    }
}

impl Routed for TerminalsChanged {
    fn decode(kind: &str, message: &RawValue) -> Result<TerminalsChanged, PaseoError> {
        decode_payload("terminals_changed", kind, message)
    }
}

impl Routed for CheckoutDiff {
    fn decode(kind: &str, message: &RawValue) -> Result<CheckoutDiff, PaseoError> {
        decode_payload("checkout_diff_update", kind, message)
    }
}

impl Routed for AgentReview {
    fn decode(kind: &str, message: &RawValue) -> Result<AgentReview, PaseoError> {
        decode_payload::<ReviewUpdate>("agent.review.update", kind, message)
            .map(|update| update.review)
    }
}

impl Routed for TimelineUpdate {
    fn decode(kind: &str, message: &RawValue) -> Result<TimelineUpdate, PaseoError> {
        serde_json::from_str(message.get()).map_err(|error| PaseoError::decode(kind, error))
    }
}

impl Routed for SessionEvent {
    fn decode(kind: &str, message: &RawValue) -> Result<SessionEvent, PaseoError> {
        serde_json::from_str(message.get()).map_err(|error| PaseoError::decode(kind, error))
    }
}

#[derive(Deserialize)]
struct TerminalExit {
    #[serde(default)]
    error: Option<String>,
}

impl Routed for TerminalStreamEvent {
    fn decode(kind: &str, message: &RawValue) -> Result<TerminalStreamEvent, PaseoError> {
        let exit = decode_payload::<TerminalExit>("terminal_stream_exit", kind, message)?;
        Ok(TerminalStreamEvent::Exit(exit.error))
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AgentTarget<'a> {
    agent_id: &'a str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct WorkspaceScriptTarget<'a> {
    workspace_id: &'a str,
    script_name: &'a str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ScheduleIdTarget<'a> {
    schedule_id: &'a str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct WorkspaceTarget<'a> {
    workspace_id: &'a str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TerminalTarget<'a> {
    terminal_id: &'a str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CwdTarget<'a> {
    cwd: &'a str,
}

#[derive(Serialize)]
struct PathTarget<'a> {
    cwd: &'a str,
    path: &'a str,
}

#[derive(Serialize)]
struct BranchTarget<'a> {
    cwd: &'a str,
    branch: &'a str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ProjectTarget<'a> {
    project_id: &'a str,
}

#[derive(Serialize)]
struct NameTarget<'a> {
    name: &'a str,
}

#[derive(Serialize)]
struct Observed<'a, Q> {
    #[serde(flatten)]
    query: &'a Q,
    subscribe: NoFields,
}

struct PendingCreation {
    registration: CreationRegistration,
    progress: flume::Receiver<Result<CreationSnapshot, PaseoError>>,
    pending: PendingReply,
}

pub struct AgentCreation {
    creation: PendingCreation,
}

pub struct WorkspaceCreation {
    creation: PendingCreation,
}

struct CreationRegistration {
    idempotency_key: String,
    client: PaseoClient,
}

impl Drop for CreationRegistration {
    fn drop(&mut self) {
        self.client.inner.unregister_creation(&self.idempotency_key);
    }
}

impl PendingCreation {
    async fn finish(self) -> Result<Box<RawValue>, PaseoError> {
        let PendingCreation {
            registration: _registration,
            pending,
            ..
        } = self;
        pending.wait(None).await
    }
}

impl AgentCreation {
    pub fn idempotency_key(&self) -> &str {
        &self.creation.registration.idempotency_key
    }

    pub fn progress(&self) -> &flume::Receiver<Result<CreationSnapshot, PaseoError>> {
        &self.creation.progress
    }

    pub async fn finish(self) -> Result<AgentSnapshot, PaseoError> {
        let payload = self.creation.finish().await?;
        let created = PaseoClient::parse::<AgentCreated>(AGENT_CREATE, &payload)?;
        AGENT_CREATE.required(created.agent, "agent")
    }
}

impl WorkspaceCreation {
    pub fn idempotency_key(&self) -> &str {
        &self.creation.registration.idempotency_key
    }

    pub fn progress(&self) -> &flume::Receiver<Result<CreationSnapshot, PaseoError>> {
        &self.creation.progress
    }

    pub async fn finish(self) -> Result<CreatedWorkspace, PaseoError> {
        let payload = self.creation.finish().await?;
        let created = PaseoClient::parse::<WorkspaceCreated>(WORKSPACE_CREATE, &payload)?;
        PaseoClient::created_workspace(WORKSPACE_CREATE, created)
    }
}

pub struct DaemonUpdate {
    phases: flume::Receiver<Result<DaemonUpdatePhase, PaseoError>>,
    pending: PendingReply,
}

impl DaemonUpdate {
    pub fn phases(&self) -> &flume::Receiver<Result<DaemonUpdatePhase, PaseoError>> {
        &self.phases
    }

    pub async fn finish(self) -> Result<DaemonUpdated, PaseoError> {
        let payload = self.pending.wait(None).await?;
        let finished = PaseoClient::parse::<DaemonUpdateFinished>(DAEMON_UPDATE, &payload)?;
        DAEMON_UPDATE.accepted(finished.success)?;
        Ok(finished.versions)
    }
}

pub struct UsageReportFetch {
    reports: flume::Receiver<Result<UsageReportEntry, PaseoError>>,
    pending: PendingReply,
}

impl UsageReportFetch {
    pub fn reports(&self) -> &flume::Receiver<Result<UsageReportEntry, PaseoError>> {
        &self.reports
    }

    pub async fn finish(self) -> Result<(), PaseoError> {
        let payload = self.pending.wait(Some(PROVIDER_CATALOGUE_TIMEOUT)).await?;
        PaseoClient::parse::<Failure>(USAGE_REPORTS, &payload)?;
        Ok(())
    }
}

const AGENT_CREATE: Rpc = Rpc::new("agent.create.request", "agent.create.response");
const DAEMON_UPDATE: Rpc = Rpc::new("daemon.update.request", "daemon.update.response");
const FILE_ENTRY_CREATE: Rpc = Rpc::new("fs.entry.create.request", "fs.entry.create.response");
const FILE_EXPLORER: Rpc = Rpc::new("file_explorer_request", "file_explorer_response");
const FILE_WRITE: Rpc = Rpc::new("fs.file.write.request", "fs.file.write.response");
const HOSTS_LIST: Rpc = Rpc::new("hosts.list.request", "hosts.list.response");
const USAGE_REPORTS: Rpc = Rpc::new("usage.list_reports.request", "usage.list_reports.response");
const WORKSPACE_LABEL_LIST: Rpc = Rpc::new(
    "workspace.label.list.request",
    "workspace.label.list.response",
);
const WORKSPACE_CREATE: Rpc = Rpc::new("workspace.create.request", "workspace.create.response");
const PROCESS_SPAWN: Rpc = Rpc::new("process.spawn.request", "process.spawn.response");
const NET_CONNECT: Rpc = Rpc::new("net.connect.request", "net.connect.response");

#[derive(Deserialize)]
struct ProcessOpened {
    pid: NonZeroU64,
}

impl Response for ProcessOpened {}

struct OpenedChannel<T: Termination> {
    lease: ChannelLease<T>,
    payload: Box<RawValue>,
}

impl PaseoClient {
    pub async fn spawn_process(&self, spawn: &ProcessSpawn) -> Result<ProcessChannel, PaseoError> {
        self.require(DaemonFeature::StreamChannels)?;
        let pseudo_terminal = spawn.pty().is_some();
        let channel = Arc::new(Channel::<ProcessExit>::new(
            Arc::downgrade(&self.inner),
            !pseudo_terminal,
        ));
        let opened = self.open_channel(PROCESS_SPAWN, spawn, channel).await?;
        let process = Self::parse::<ProcessOpened>(PROCESS_SPAWN, &opened.payload)?;
        Ok(ProcessChannel::new(
            opened.lease,
            process.pid.get(),
            pseudo_terminal,
        ))
    }

    pub async fn connect_net(&self, host: &str, port: u16) -> Result<NetChannel, PaseoError> {
        self.require(DaemonFeature::StreamChannels)?;
        self.open_net(&NetTarget::Tcp { host, port }).await
    }

    pub async fn connect_unix_socket(&self, path: &str) -> Result<NetChannel, PaseoError> {
        self.require(DaemonFeature::StreamChannelUnixSockets)?;
        self.open_net(&NetTarget::Unix { path }).await
    }

    async fn open_net(&self, target: &NetTarget<'_>) -> Result<NetChannel, PaseoError> {
        let channel = Arc::new(Channel::<NetClosed>::new(Arc::downgrade(&self.inner), true));
        let opened = self.open_channel(NET_CONNECT, target, channel).await?;
        Ok(NetChannel::new(opened.lease))
    }

    async fn open_channel<B: Serialize, T: Termination>(
        &self,
        rpc: Rpc,
        body: &B,
        channel: Arc<Channel<T>>,
    ) -> Result<OpenedChannel<T>, PaseoError> {
        let registration =
            Inner::channel_registration(channel.clone(), channel.events(), rpc.kind, rpc.response);
        let request = Request {
            kind: rpc.kind,
            response_type: rpc.response,
            body,
        };
        let payload = self
            .inner
            .submit(&request, Some(registration), None)?
            .wait(Some(REQUEST_TIMEOUT))
            .await?;
        let subscription_id = Self::subscription_id(rpc, &payload)?;
        Ok(OpenedChannel {
            lease: ChannelLease::new(subscription_id, channel),
            payload,
        })
    }

    fn require(&self, feature: DaemonFeature) -> Result<(), PaseoError> {
        if self.server_info().supports(feature) {
            return Ok(());
        }
        Err(PaseoError::UnsupportedDaemon {
            feature: feature.wire_name(),
        })
    }

    fn parse<R: Response>(rpc: Rpc, payload: &RawValue) -> Result<R, PaseoError> {
        let response =
            serde_json::from_str::<R>(payload.get()).map_err(
                |error| match serde_json::from_str::<Failure>(payload.get()) {
                    Ok(Failure {
                        error: Some(failure),
                    }) => PaseoError::rpc(rpc.kind, failure),
                    _ => PaseoError::decode(rpc.response, error),
                },
            )?;
        if let Some(error) = response.failure() {
            return Err(PaseoError::rpc(rpc.kind, error));
        }
        if let Some(error) = response.rejection() {
            return Err(PaseoError::checkout(rpc.kind, error.clone()));
        }
        Ok(response)
    }

    async fn call<B: Serialize, R: Response>(&self, rpc: Rpc, body: &B) -> Result<R, PaseoError> {
        self.call_within(rpc, body, Some(REQUEST_TIMEOUT)).await
    }

    async fn call_within<B: Serialize, R: Response>(
        &self,
        rpc: Rpc,
        body: &B,
        timeout: Option<Duration>,
    ) -> Result<R, PaseoError> {
        let request = Request {
            kind: rpc.kind,
            response_type: rpc.response,
            body,
        };
        let payload = self
            .inner
            .submit(&request, None, None)?
            .wait(timeout)
            .await?;
        Self::parse(rpc, &payload)
    }

    async fn call_checked<B: Serialize, T: DeserializeOwned>(
        &self,
        rpc: Rpc,
        body: &B,
    ) -> Result<T, PaseoError> {
        Ok(self.call::<_, Checked<T>>(rpc, body).await?.value)
    }

    async fn observe<B: Serialize, S: Response, T, U: Routed>(
        &self,
        rpc: Rpc,
        body: &B,
        snapshot: impl FnOnce(S) -> T,
    ) -> Result<Subscription<T, U>, PaseoError> {
        let (updates, receiver) = flume::unbounded();
        let request = Request {
            kind: rpc.kind,
            response_type: rpc.response,
            body,
        };
        let registration = Inner::route_registration(updates);
        let payload = self
            .inner
            .submit(&request, Some(registration), None)?
            .wait(Some(REQUEST_TIMEOUT))
            .await?;
        let response = match Self::parse::<S>(rpc, &payload) {
            Ok(response) => response,
            Err(error) => {
                if let Ok(id) = Self::subscription_id(rpc, &payload) {
                    self.inner.release(&id);
                }
                return Err(error);
            }
        };
        let id = Self::subscription_id(rpc, &payload)?;
        Ok(Subscription::new(
            id,
            snapshot(response),
            receiver,
            &self.inner,
        ))
    }

    fn subscription_id(rpc: Rpc, payload: &RawValue) -> Result<String, PaseoError> {
        let head = serde_json::from_str::<PayloadHead<'_>>(payload.get())?;
        head.subscription_id
            .map(|id| id.into_owned())
            .ok_or_else(|| {
                PaseoError::from(ProtocolViolation::MissingSubscriptionId {
                    response_type: rpc.response,
                })
            })
    }

    pub async fn observe_agents(
        &self,
        query: &AgentDirectoryQuery,
    ) -> Result<Subscription<AgentDirectoryPage, AgentUpdate>, PaseoError> {
        let rpc = Rpc::new("fetch_agents_request", "fetch_agents_response");
        self.observe(
            rpc,
            &Observed {
                query,
                subscribe: NoFields {},
            },
            |page| page,
        )
        .await
    }

    pub async fn observe_workspaces(
        &self,
        query: &WorkspaceDirectoryQuery,
    ) -> Result<Subscription<WorkspaceDirectoryPage, WorkspaceUpdate>, PaseoError> {
        let rpc = Rpc::new("fetch_workspaces_request", "fetch_workspaces_response");
        self.observe(
            rpc,
            &Observed {
                query,
                subscribe: NoFields {},
            },
            |page| page,
        )
        .await
    }

    pub async fn observe_timeline(
        &self,
        agent_ids: &[String],
    ) -> Result<Subscription<(), TimelineUpdate>, PaseoError> {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Body<'a> {
            agent_ids: &'a [String],
        }
        let rpc = Rpc::new(
            "agent.timeline.set_subscription.request",
            "agent.timeline.set_subscription.response",
        );
        self.observe(rpc, &Body { agent_ids }, |_: Acknowledged| ())
            .await
    }

    pub async fn observe_events(
        &self,
        events: &[EventKind],
        notifications: bool,
    ) -> Result<Subscription<(), SessionEvent>, PaseoError> {
        #[derive(Serialize)]
        struct Body<'a> {
            events: &'a [EventKind],
            notifications: bool,
        }
        let rpc = Rpc::new(
            "session.events.set_subscription.request",
            "session.events.set_subscription.response",
        );
        self.observe(
            rpc,
            &Body {
                events,
                notifications,
            },
            |_: Acknowledged| (),
        )
        .await
    }

    pub async fn observe_terminals(
        &self,
        cwd: &str,
        workspace_id: Option<&str>,
    ) -> Result<Subscription<TerminalsChanged, TerminalsChanged>, PaseoError> {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Body<'a> {
            cwd: &'a str,
            #[serde(skip_serializing_if = "Option::is_none")]
            workspace_id: Option<&'a str>,
        }
        let rpc = Rpc::new("subscribe_terminals_request", "terminals_changed");
        self.observe(rpc, &Body { cwd, workspace_id }, |terminals| terminals)
            .await
    }

    pub async fn observe_checkout_diff(
        &self,
        cwd: &str,
        compare: &DiffCompare,
    ) -> Result<Subscription<CheckoutDiff, CheckoutDiff>, PaseoError> {
        #[derive(Serialize)]
        struct Body<'a> {
            cwd: &'a str,
            compare: &'a DiffCompare,
        }
        self.require_diff_mode(compare)?;
        let rpc = Rpc::new(
            "subscribe_checkout_diff_request",
            "subscribe_checkout_diff_response",
        );
        self.observe(rpc, &Body { cwd, compare }, |diff| diff).await
    }

    pub async fn subscribe_terminal(
        &self,
        terminal_id: &str,
        restore: &TerminalRestore,
    ) -> Result<TerminalHandle, PaseoError> {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Body<'a> {
            terminal_id: &'a str,
            restore: &'a TerminalRestore,
        }
        let rpc = Rpc::new("subscribe_terminal_request", "subscribe_terminal_response");
        let (events, receiver) = flume::unbounded();
        let request = Request {
            kind: rpc.kind,
            response_type: rpc.response,
            body: &Body {
                terminal_id,
                restore,
            },
        };
        let payload = self
            .inner
            .submit(&request, Some(Inner::terminal_registration(events)), None)?
            .wait(Some(REQUEST_TIMEOUT))
            .await?;
        let id = Self::subscription_id(rpc, &payload)?;
        match Self::parse::<TerminalSlot>(rpc, &payload) {
            Ok(slot) => Ok(TerminalHandle::new(
                Subscription::new(id, slot, receiver, &self.inner),
                &self.inner,
            )),
            Err(error) => {
                self.inner.release(&id);
                Err(error)
            }
        }
    }

    pub async fn fetch_agents(
        &self,
        query: &AgentDirectoryQuery,
    ) -> Result<AgentDirectoryPage, PaseoError> {
        self.call(
            Rpc::new("fetch_agents_request", "fetch_agents_response"),
            query,
        )
        .await
    }

    pub async fn fetch_agent_history(
        &self,
        query: &AgentDirectoryQuery,
        search: Option<&str>,
    ) -> Result<AgentDirectoryPage, PaseoError> {
        #[derive(Serialize)]
        struct Body<'a> {
            #[serde(flatten)]
            query: &'a AgentDirectoryQuery,
            #[serde(skip_serializing_if = "Option::is_none")]
            search: Option<&'a str>,
        }
        let rpc = Rpc::new(
            "fetch_agent_history_request",
            "fetch_agent_history_response",
        );
        self.call(rpc, &Body { query, search }).await
    }

    pub async fn fetch_agent(&self, agent_id: &str) -> Result<AgentDirectoryEntry, PaseoError> {
        let rpc = Rpc::new("fetch_agent_request", "fetch_agent_response");
        let fetched = self
            .call::<_, AgentFetched>(rpc, &AgentTarget { agent_id })
            .await?;
        match (fetched.agent, fetched.project) {
            (Some(agent), Some(project)) => Ok(AgentDirectoryEntry::new(agent, project)),
            _ => Err(PaseoError::request(rpc.kind, RequestFailure::NotFound)),
        }
    }

    pub async fn fetch_agent_timeline(
        &self,
        query: &TimelineQuery,
    ) -> Result<AgentTimelinePage, PaseoError> {
        let rpc = Rpc::new(
            "fetch_agent_timeline_request",
            "fetch_agent_timeline_response",
        );
        self.call_checked(rpc, query).await
    }

    pub fn create_agent(&self, request: &AgentCreateRequest) -> Result<AgentCreation, PaseoError> {
        #[derive(Serialize)]
        struct Body<'a> {
            #[serde(flatten)]
            request: &'a AgentCreateRequest,
            subscribe: bool,
        }
        let body = Body {
            request,
            subscribe: true,
        };
        let creation = self.submit_creation(AGENT_CREATE, request.idempotency_key(), &body)?;
        Ok(AgentCreation { creation })
    }

    /// Creates a workspace from `source` together with its first agent, as one creation keyed by
    /// the agent request's idempotency key. The request's workspace and worktree are ignored.
    pub fn create_workspace_with_agent(
        &self,
        source: &WorkspaceSource,
        agent: &AgentCreateRequest,
    ) -> Result<WorkspaceCreation, PaseoError> {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct InitialAgent<'a> {
            #[serde(skip_serializing_if = "Option::is_none")]
            agent_id: Option<&'a str>,
            config: &'a AgentSessionConfig,
            #[serde(skip_serializing_if = "HashMap::is_empty")]
            env: &'a HashMap<String, String>,
            #[serde(skip_serializing_if = "Option::is_none")]
            initial_prompt: Option<&'a str>,
            #[serde(skip_serializing_if = "Option::is_none")]
            client_message_id: Option<&'a str>,
            #[serde(skip_serializing_if = "<[_]>::is_empty")]
            images: &'a [ImageAttachment],
            #[serde(skip_serializing_if = "<[_]>::is_empty")]
            attachments: &'a [AgentAttachment],
            labels: &'a HashMap<String, String>,
        }
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Body<'a> {
            source: &'a WorkspaceSource,
            idempotency_key: &'a str,
            agent: InitialAgent<'a>,
            subscribe: bool,
        }
        let body = Body {
            source,
            idempotency_key: agent.idempotency_key(),
            agent: InitialAgent {
                agent_id: agent.agent_id(),
                config: agent.config(),
                env: agent.env(),
                initial_prompt: agent.initial_prompt(),
                client_message_id: agent.client_message_id(),
                images: agent.images(),
                attachments: agent.attachments(),
                labels: agent.labels(),
            },
            subscribe: true,
        };
        let creation = self.submit_creation(WORKSPACE_CREATE, agent.idempotency_key(), &body)?;
        Ok(WorkspaceCreation { creation })
    }

    fn submit_creation<B: Serialize>(
        &self,
        rpc: Rpc,
        idempotency_key: &str,
        body: &B,
    ) -> Result<PendingCreation, PaseoError> {
        let (progress_tx, progress) = flume::unbounded();
        self.inner
            .register_creation(idempotency_key.to_owned(), progress_tx);
        let registration = CreationRegistration {
            idempotency_key: idempotency_key.to_owned(),
            client: self.clone(),
        };
        let pending = self.inner.submit(
            &Request {
                kind: rpc.kind,
                response_type: rpc.response,
                body,
            },
            None,
            None,
        )?;
        Ok(PendingCreation {
            registration,
            progress,
            pending,
        })
    }

    pub async fn send_agent_message(
        &self,
        agent_id: &str,
        message: &AgentMessage,
    ) -> Result<(), PaseoError> {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Body<'a> {
            agent_id: &'a str,
            #[serde(flatten)]
            message: &'a AgentMessage,
        }
        let rpc = Rpc::new("send_agent_message_request", "send_agent_message_response");
        let response = self
            .call::<_, SendResponse>(rpc, &Body { agent_id, message })
            .await?;
        rpc.accepted(response.accepted)
    }

    pub async fn cancel_agent(&self, agent_id: &str) -> Result<(), PaseoError> {
        let rpc = Rpc::new("cancel_agent_request", "cancel_agent_response");
        self.call::<_, CancelResponse>(rpc, &AgentTarget { agent_id })
            .await?;
        Ok(())
    }

    pub async fn archive_agent(&self, agent_id: &str) -> Result<String, PaseoError> {
        let rpc = Rpc::new("archive_agent_request", "agent_archived");
        Ok(self
            .call::<_, AgentArchived>(rpc, &AgentTarget { agent_id })
            .await?
            .archived_at)
    }

    pub async fn delete_agent(&self, agent_id: &str) -> Result<(), PaseoError> {
        let rpc = Rpc::new("delete_agent_request", "agent_deleted");
        self.call::<_, Acknowledged>(rpc, &AgentTarget { agent_id })
            .await?;
        Ok(())
    }

    async fn agent_action<B: Serialize>(
        &self,
        rpc: Rpc,
        body: &B,
    ) -> Result<AgentActionOutcome, PaseoError> {
        let response = self.call::<_, ActionResponse>(rpc, body).await?;
        rpc.accepted(response.accepted)?;
        Ok(AgentActionOutcome::new(response.agent_id, response.notice))
    }

    pub async fn set_agent_mode(
        &self,
        agent_id: &str,
        mode_id: &str,
    ) -> Result<AgentActionOutcome, PaseoError> {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Body<'a> {
            agent_id: &'a str,
            mode_id: &'a str,
        }
        let rpc = Rpc::new("set_agent_mode_request", "set_agent_mode_response");
        self.agent_action(rpc, &Body { agent_id, mode_id }).await
    }

    pub async fn set_agent_model(
        &self,
        agent_id: &str,
        model_id: Option<&str>,
    ) -> Result<AgentActionOutcome, PaseoError> {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Body<'a> {
            agent_id: &'a str,
            model_id: Option<&'a str>,
        }
        let rpc = Rpc::new("set_agent_model_request", "set_agent_model_response");
        self.agent_action(rpc, &Body { agent_id, model_id }).await
    }

    pub async fn set_agent_thinking(
        &self,
        agent_id: &str,
        thinking_option_id: Option<&str>,
    ) -> Result<AgentActionOutcome, PaseoError> {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Body<'a> {
            agent_id: &'a str,
            thinking_option_id: Option<&'a str>,
        }
        let rpc = Rpc::new("set_agent_thinking_request", "set_agent_thinking_response");
        self.agent_action(
            rpc,
            &Body {
                agent_id,
                thinking_option_id,
            },
        )
        .await
    }

    pub async fn set_agent_feature(
        &self,
        agent_id: &str,
        feature_id: &str,
        value: Value,
    ) -> Result<AgentActionOutcome, PaseoError> {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Body<'a> {
            agent_id: &'a str,
            feature_id: &'a str,
            value: Value,
        }
        let rpc = Rpc::new("set_agent_feature_request", "set_agent_feature_response");
        self.agent_action(
            rpc,
            &Body {
                agent_id,
                feature_id,
                value,
            },
        )
        .await
    }

    pub async fn apply_agent_config(
        &self,
        agent_id: &str,
        config: &AgentConfigPatch,
    ) -> Result<AgentActionOutcome, PaseoError> {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Body<'a> {
            agent_id: &'a str,
            config: &'a AgentConfigPatch,
        }
        let rpc = Rpc::new("agent.config.apply.request", "agent.config.apply.response");
        self.agent_action(rpc, &Body { agent_id, config }).await
    }

    pub async fn update_agent(
        &self,
        agent_id: &str,
        name: Option<&str>,
        labels: Option<&HashMap<String, String>>,
    ) -> Result<AgentActionOutcome, PaseoError> {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Body<'a> {
            agent_id: &'a str,
            #[serde(skip_serializing_if = "Option::is_none")]
            name: Option<&'a str>,
            #[serde(skip_serializing_if = "Option::is_none")]
            labels: Option<&'a HashMap<String, String>>,
        }
        let rpc = Rpc::new("update_agent_request", "update_agent_response");
        self.agent_action(
            rpc,
            &Body {
                agent_id,
                name,
                labels,
            },
        )
        .await
    }

    /// Sets or removes labels: a `None` value removes that label.
    pub async fn update_agent_labels(
        &self,
        agent_id: &str,
        labels: &HashMap<String, Option<String>>,
    ) -> Result<AgentActionOutcome, PaseoError> {
        if labels.values().any(Option::is_none) {
            self.require(DaemonFeature::AgentOrganization)?;
        }
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Body<'a> {
            agent_id: &'a str,
            labels: &'a HashMap<String, Option<String>>,
        }
        let rpc = Rpc::new("update_agent_request", "update_agent_response");
        self.agent_action(rpc, &Body { agent_id, labels }).await
    }

    /// Pins or unpins an agent, returning when it was pinned.
    pub async fn set_agent_pinned(
        &self,
        agent_id: &str,
        pinned: bool,
    ) -> Result<Option<String>, PaseoError> {
        self.require(DaemonFeature::AgentOrganization)?;
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Body<'a> {
            agent_id: &'a str,
            pinned: bool,
        }
        let rpc = Rpc::new("agent.pin.set.request", "agent.pin.set.response");
        let response = self
            .call::<_, Pinned>(rpc, &Body { agent_id, pinned })
            .await?;
        rpc.accepted(response.accepted)?;
        Ok(response.pinned_at)
    }

    pub async fn detach_agent(&self, agent_id: &str) -> Result<AgentActionOutcome, PaseoError> {
        let rpc = Rpc::new("agent.detach.request", "agent.detach.response");
        self.agent_action(rpc, &AgentTarget { agent_id }).await
    }

    pub async fn rewind_agent(
        &self,
        agent_id: &str,
        message_id: &str,
        mode: RewindMode,
    ) -> Result<(), PaseoError> {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Body<'a> {
            agent_id: &'a str,
            message_id: &'a str,
            mode: RewindMode,
        }
        let rpc = Rpc::new("agent.rewind.request", "agent.rewind.response");
        let response = self
            .call::<_, RewindResponse>(
                rpc,
                &Body {
                    agent_id,
                    message_id,
                    mode,
                },
            )
            .await?;
        rpc.accepted(response.ok)
    }

    async fn agent_status(
        &self,
        rpc: Rpc,
        body: &impl Serialize,
        expected: &str,
    ) -> Result<Option<AgentSnapshot>, PaseoError> {
        let response = self.call::<_, StatusPayload>(rpc, body).await?;
        if response.status != expected {
            return Err(PaseoError::request(
                rpc.kind,
                RequestFailure::UnexpectedStatus(response.status),
            ));
        }
        Ok(response.agent)
    }

    pub async fn resume_agent(
        &self,
        handle: &AgentPersistenceHandle,
        overrides: Option<&AgentSessionConfig>,
    ) -> Result<AgentSnapshot, PaseoError> {
        #[derive(Serialize)]
        struct Body<'a> {
            handle: &'a AgentPersistenceHandle,
            #[serde(skip_serializing_if = "Option::is_none")]
            overrides: Option<&'a AgentSessionConfig>,
        }
        let rpc = Rpc::new("resume_agent_request", "status");
        let agent = self
            .agent_status(rpc, &Body { handle, overrides }, "agent_resumed")
            .await?;
        rpc.required(agent, "agent")
    }

    pub async fn import_agent(
        &self,
        request: &ImportAgentRequest,
    ) -> Result<AgentSnapshot, PaseoError> {
        let rpc = Rpc::new("import_agent_request", "status");
        let agent = self.agent_status(rpc, request, "agent_resumed").await?;
        rpc.required(agent, "agent")
    }

    pub async fn refresh_agent(&self, agent_id: &str) -> Result<(), PaseoError> {
        let rpc = Rpc::new("refresh_agent_request", "status");
        self.agent_status(rpc, &AgentTarget { agent_id }, "agent_refreshed")
            .await?;
        Ok(())
    }

    pub async fn wait_for_finish(
        &self,
        agent_id: &str,
        timeout: Option<Duration>,
    ) -> Result<WaitOutcome, PaseoError> {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Body<'a> {
            agent_id: &'a str,
            #[serde(skip_serializing_if = "Option::is_none")]
            timeout_ms: Option<u64>,
        }
        let timeout_ms = timeout.map(|timeout| {
            u64::try_from(timeout.as_millis())
                .unwrap_or(MAX_WAIT_FOR_FINISH_MS)
                .clamp(1, MAX_WAIT_FOR_FINISH_MS)
        });
        let rpc = Rpc::new("wait_for_finish_request", "wait_for_finish_response");
        self.call_within(
            rpc,
            &Body {
                agent_id,
                timeout_ms,
            },
            timeout.map(|timeout| timeout.saturating_add(REQUEST_TIMEOUT)),
        )
        .await
    }

    pub async fn observe_agent_review(
        &self,
        agent_id: &str,
    ) -> Result<Subscription<AgentReview, AgentReview>, PaseoError> {
        self.require(DaemonFeature::AgentReview)?;
        let rpc = Rpc::new(
            "agent.review.subscribe.request",
            "agent.review.subscribe.response",
        );
        self.observe(
            rpc,
            &AgentTarget { agent_id },
            |subscribed: Checked<ReviewSubscribed>| subscribed.value.review,
        )
        .await
    }

    pub async fn mark_review_seen(
        &self,
        agent_id: &str,
        files: &[SeenFile],
    ) -> Result<(), PaseoError> {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Body<'a> {
            agent_id: &'a str,
            files: &'a [SeenFile],
        }
        self.require(DaemonFeature::AgentReview)?;
        let rpc = Rpc::new(
            "agent.review.mark_seen.request",
            "agent.review.mark_seen.response",
        );
        self.call::<_, Failure>(rpc, &Body { agent_id, files })
            .await?;
        Ok(())
    }

    pub async fn mark_review_unseen(
        &self,
        agent_id: &str,
        files: &[FileReference],
    ) -> Result<(), PaseoError> {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Body<'a> {
            agent_id: &'a str,
            files: &'a [FileReference],
        }
        self.require(DaemonFeature::AgentReview)?;
        let rpc = Rpc::new(
            "agent.review.mark_unseen.request",
            "agent.review.mark_unseen.response",
        );
        self.call::<_, Failure>(rpc, &Body { agent_id, files })
            .await?;
        Ok(())
    }

    pub async fn save_review_comment(
        &self,
        agent_id: &str,
        comment: &ReviewCommentInput,
    ) -> Result<(), PaseoError> {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Body<'a> {
            agent_id: &'a str,
            comment: &'a ReviewCommentInput,
        }
        self.require(DaemonFeature::AgentReview)?;
        if comment.side() == CommentSide::Old {
            self.require(DaemonFeature::AgentReviewRemovedLines)?;
        }
        let rpc = Rpc::new(
            "agent.review.save_comment.request",
            "agent.review.save_comment.response",
        );
        self.call::<_, Failure>(rpc, &Body { agent_id, comment })
            .await?;
        Ok(())
    }

    pub async fn delete_review_comment(
        &self,
        agent_id: &str,
        comment_id: &str,
    ) -> Result<(), PaseoError> {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Body<'a> {
            agent_id: &'a str,
            comment_id: &'a str,
        }
        self.require(DaemonFeature::AgentReview)?;
        let rpc = Rpc::new(
            "agent.review.delete_comment.request",
            "agent.review.delete_comment.response",
        );
        self.call::<_, Failure>(
            rpc,
            &Body {
                agent_id,
                comment_id,
            },
        )
        .await?;
        Ok(())
    }

    pub async fn list_prompts(&self, agent_id: &str) -> Result<Vec<PromptPreview>, PaseoError> {
        let rpc = Rpc::new(
            "agent.timeline.list_prompts.request",
            "agent.timeline.list_prompts.response",
        );
        Ok(self
            .call::<_, PromptsListed>(rpc, &AgentTarget { agent_id })
            .await?
            .prompts)
    }

    pub async fn fork_context(
        &self,
        agent_id: &str,
        boundary_message_id: Option<&str>,
        boundary_cursor: Option<&TimelineCursor>,
    ) -> Result<ForkedContext, PaseoError> {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Body<'a> {
            agent_id: &'a str,
            #[serde(skip_serializing_if = "Option::is_none")]
            boundary_message_id: Option<&'a str>,
            #[serde(skip_serializing_if = "Option::is_none")]
            boundary_cursor: Option<&'a TimelineCursor>,
        }
        let rpc = Rpc::new("agent.fork_context.request", "agent.fork_context.response");
        let body = Body {
            agent_id,
            boundary_message_id,
            boundary_cursor,
        };
        self.call_checked(rpc, &body).await
    }

    pub fn respond_permission(
        &self,
        agent_id: &str,
        request_id: &str,
        response: &PermissionResponse,
    ) -> Result<(), PaseoError> {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Body<'a> {
            agent_id: &'a str,
            request_id: &'a str,
            response: &'a PermissionResponse,
        }
        let text = session_message(
            "agent_permission_response",
            None,
            &Body {
                agent_id,
                request_id,
                response,
            },
        )?;
        self.inner.send(Frame::Text(text))
    }

    /// Tells the daemon whether the user is here and what they're looking at. Only clients that
    /// send heartbeats are picked to notify about agents.
    pub fn send_heartbeat(&self, heartbeat: &ClientHeartbeat) -> Result<(), PaseoError> {
        let text = session_message("client_heartbeat", None, heartbeat)?;
        self.inner.send(Frame::Text(text))
    }

    pub async fn clear_agent_attention(&self, agent_ids: &[String]) -> Result<(), PaseoError> {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Body<'a> {
            agent_id: &'a [String],
        }
        let rpc = Rpc::new("clear_agent_attention", "clear_agent_attention_response");
        self.call::<_, Acknowledged>(
            rpc,
            &Body {
                agent_id: agent_ids,
            },
        )
        .await?;
        Ok(())
    }

    pub async fn list_commands(
        &self,
        agent_id: &str,
        draft: Option<&AgentSessionConfig>,
    ) -> Result<Vec<SlashCommand>, PaseoError> {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Body<'a> {
            agent_id: &'a str,
            #[serde(skip_serializing_if = "Option::is_none")]
            draft_config: Option<DraftConfig>,
        }
        let rpc = Rpc::new("list_commands_request", "list_commands_response");
        let body = Body {
            agent_id,
            draft_config: draft.map(DraftConfig::from),
        };
        Ok(self.call::<_, CommandsListed>(rpc, &body).await?.commands)
    }

    pub async fn list_provider_models(
        &self,
        provider: &str,
        cwd: Option<&str>,
    ) -> Result<Vec<ModelDefinition>, PaseoError> {
        let rpc = Rpc::new(
            "list_provider_models_request",
            "list_provider_models_response",
        );
        let response = self
            .call_within::<_, ModelsListed>(
                rpc,
                &ProviderQuery { provider, cwd },
                Some(PROVIDER_CATALOGUE_TIMEOUT),
            )
            .await?;
        Ok(response.models)
    }

    pub async fn list_provider_modes(
        &self,
        provider: &str,
        cwd: Option<&str>,
    ) -> Result<Vec<AgentMode>, PaseoError> {
        let rpc = Rpc::new(
            "list_provider_modes_request",
            "list_provider_modes_response",
        );
        let response = self
            .call_within::<_, ModesListed>(
                rpc,
                &ProviderQuery { provider, cwd },
                Some(PROVIDER_CATALOGUE_TIMEOUT),
            )
            .await?;
        Ok(response.modes)
    }

    pub async fn list_provider_features(
        &self,
        draft: &AgentSessionConfig,
    ) -> Result<Vec<AgentFeature>, PaseoError> {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Body {
            draft_config: DraftConfig,
        }
        let rpc = Rpc::new(
            "list_provider_features_request",
            "list_provider_features_response",
        );
        let body = Body {
            draft_config: DraftConfig::from(draft),
        };
        let response = self
            .call_within::<_, FeaturesListed>(rpc, &body, Some(PROVIDER_CATALOGUE_TIMEOUT))
            .await?;
        Ok(response.features)
    }

    pub async fn list_available_providers(&self) -> Result<Vec<ProviderAvailability>, PaseoError> {
        let rpc = Rpc::new(
            "list_available_providers_request",
            "list_available_providers_response",
        );
        Ok(self
            .call::<_, ProvidersListed>(rpc, &NoFields {})
            .await?
            .providers)
    }

    pub async fn providers_snapshot(
        &self,
        cwd: Option<&str>,
    ) -> Result<ProvidersSnapshot, PaseoError> {
        #[derive(Serialize)]
        struct Body<'a> {
            #[serde(skip_serializing_if = "Option::is_none")]
            cwd: Option<&'a str>,
        }
        let rpc = Rpc::new(
            "get_providers_snapshot_request",
            "get_providers_snapshot_response",
        );
        self.call(rpc, &Body { cwd }).await
    }

    pub async fn refresh_providers_snapshot(
        &self,
        cwd: Option<&str>,
        providers: &[String],
    ) -> Result<(), PaseoError> {
        #[derive(Serialize)]
        struct Body<'a> {
            #[serde(skip_serializing_if = "Option::is_none")]
            cwd: Option<&'a str>,
            #[serde(skip_serializing_if = "<[String]>::is_empty")]
            providers: &'a [String],
        }
        let rpc = Rpc::new(
            "refresh_providers_snapshot_request",
            "refresh_providers_snapshot_response",
        );
        let refreshed = self
            .call::<_, RefreshAcknowledged>(rpc, &Body { cwd, providers })
            .await?;
        rpc.accepted(refreshed.acknowledged)
    }

    pub async fn list_provider_subagents(
        &self,
        parent_agent_id: &str,
    ) -> Result<Vec<ProviderSubagent>, PaseoError> {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Body<'a> {
            parent_agent_id: &'a str,
        }
        let rpc = Rpc::new(
            "agent.provider_subagents.list.request",
            "agent.provider_subagents.list.response",
        );
        Ok(self
            .call::<_, SubagentsListed>(rpc, &Body { parent_agent_id })
            .await?
            .subagents)
    }

    pub async fn fetch_provider_subagent_timeline(
        &self,
        query: &SubagentTimelineQuery,
    ) -> Result<SubagentTimelinePage, PaseoError> {
        let rpc = Rpc::new(
            "agent.provider_subagents.timeline.get.request",
            "agent.provider_subagents.timeline.get.response",
        );
        self.call_checked(rpc, query).await
    }

    pub async fn fetch_workspaces(
        &self,
        query: &WorkspaceDirectoryQuery,
    ) -> Result<WorkspaceDirectoryPage, PaseoError> {
        self.call(
            Rpc::new("fetch_workspaces_request", "fetch_workspaces_response"),
            query,
        )
        .await
    }

    pub async fn list_projects(&self) -> Result<Vec<ProjectDescriptor>, PaseoError> {
        let rpc = Rpc::new("project.list.request", "project.list.response");
        Ok(self
            .call::<_, ProjectsListed>(rpc, &NoFields {})
            .await?
            .projects)
    }

    pub async fn open_project(&self, cwd: &str) -> Result<WorkspaceDescriptor, PaseoError> {
        let rpc = Rpc::new("open_project_request", "open_project_response");
        let opened = self
            .call::<_, ProjectOpened>(rpc, &CwdTarget { cwd })
            .await?;
        rpc.required(opened.workspace, "workspace")
    }

    pub async fn add_project(&self, cwd: &str) -> Result<ProjectDescriptor, PaseoError> {
        let rpc = Rpc::new("project.add.request", "project.add.response");
        let added = self
            .call::<_, ProjectAdded>(rpc, &CwdTarget { cwd })
            .await?;
        rpc.required(added.project, "project")
    }

    pub async fn create_project_directory(
        &self,
        parent_path: &str,
        name: &str,
    ) -> Result<String, PaseoError> {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Body<'a> {
            parent_path: &'a str,
            name: &'a str,
        }
        let rpc = Rpc::new(
            "project.create_directory.request",
            "project.create_directory.response",
        );
        let created = self
            .call::<_, DirectoryCreated>(rpc, &Body { parent_path, name })
            .await?;
        rpc.required(created.directory_path, "directory")
    }

    pub async fn clone_github_project(
        &self,
        repo: &str,
        target_directory: &str,
    ) -> Result<String, PaseoError> {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Body<'a> {
            repo: &'a str,
            target_directory: &'a str,
        }
        let rpc = Rpc::new(
            "project.github.clone.request",
            "project.github.clone.response",
        );
        let response = self
            .call_within::<_, GithubCloned>(
                rpc,
                &Body {
                    repo,
                    target_directory,
                },
                Some(GITHUB_CLONE_TIMEOUT),
            )
            .await?;
        let checkout_path = response.checkout_path.or_else(|| {
            response
                .project
                .map(|project| project.project_root_path().to_owned())
        });
        rpc.required(checkout_path, "project")
    }

    pub async fn set_workspace_title(
        &self,
        workspace_id: &str,
        title: Option<&str>,
    ) -> Result<(), PaseoError> {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Body<'a> {
            workspace_id: &'a str,
            title: Option<&'a str>,
        }
        let rpc = Rpc::new(
            "workspace.title.set.request",
            "workspace.title.set.response",
        );
        let response = self
            .call::<_, TitleSet>(
                rpc,
                &Body {
                    workspace_id,
                    title,
                },
            )
            .await?;
        rpc.accepted(response.accepted)
    }

    pub async fn archive_workspace(
        &self,
        workspace_id: &str,
    ) -> Result<Option<String>, PaseoError> {
        let rpc = Rpc::new("archive_workspace_request", "archive_workspace_response");
        Ok(self
            .call::<_, WorkspaceArchived>(rpc, &WorkspaceTarget { workspace_id })
            .await?
            .archived_at)
    }

    pub async fn workspace_setup_status(
        &self,
        workspace_id: &str,
    ) -> Result<Option<WorkspaceSetupSnapshot>, PaseoError> {
        let rpc = Rpc::new(
            "workspace_setup_status_request",
            "workspace_setup_status_response",
        );
        Ok(self
            .call::<_, SetupStatus>(rpc, &WorkspaceTarget { workspace_id })
            .await?
            .snapshot)
    }

    pub async fn create_workspace(
        &self,
        source: &WorkspaceSource,
        title: Option<&str>,
    ) -> Result<CreatedWorkspace, PaseoError> {
        #[derive(Serialize)]
        struct Body<'a> {
            source: &'a WorkspaceSource,
            #[serde(skip_serializing_if = "Option::is_none")]
            title: Option<&'a str>,
        }
        let creation = self
            .call::<_, WorkspaceCreated>(WORKSPACE_CREATE, &Body { source, title })
            .await?;
        Self::created_workspace(WORKSPACE_CREATE, creation)
    }

    pub async fn list_paseo_worktrees(&self, cwd: &str) -> Result<Vec<PaseoWorktree>, PaseoError> {
        let rpc = Rpc::new(
            "paseo_worktree_list_request",
            "paseo_worktree_list_response",
        );
        #[derive(Deserialize)]
        struct Listed {
            worktrees: Vec<PaseoWorktree>,
        }
        Ok(self
            .call::<_, CheckoutResult<Listed>>(rpc, &CwdTarget { cwd })
            .await?
            .value
            .worktrees)
    }

    pub async fn create_paseo_worktree(
        &self,
        cwd: &str,
        action: WorktreeAction,
        ref_name: Option<&str>,
        worktree_slug: Option<&str>,
        first_prompt: Option<&str>,
    ) -> Result<CreatedWorkspace, PaseoError> {
        #[derive(Serialize)]
        struct FirstAgentContext<'a> {
            prompt: &'a str,
        }
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Body<'a> {
            cwd: &'a str,
            action: WorktreeAction,
            #[serde(skip_serializing_if = "Option::is_none")]
            ref_name: Option<&'a str>,
            #[serde(skip_serializing_if = "Option::is_none")]
            worktree_slug: Option<&'a str>,
            #[serde(skip_serializing_if = "Option::is_none")]
            first_agent_context: Option<FirstAgentContext<'a>>,
        }
        let rpc = Rpc::new(
            "create_paseo_worktree_request",
            "create_paseo_worktree_response",
        );
        let body = Body {
            cwd,
            action,
            ref_name,
            worktree_slug,
            first_agent_context: first_prompt.map(|prompt| FirstAgentContext { prompt }),
        };
        let creation = self.call::<_, WorkspaceCreated>(rpc, &body).await?;
        Self::created_workspace(rpc, creation)
    }

    fn created_workspace(
        rpc: Rpc,
        creation: WorkspaceCreated,
    ) -> Result<CreatedWorkspace, PaseoError> {
        let workspace = rpc.required(creation.workspace, "workspace")?;
        Ok(CreatedWorkspace::new(
            workspace,
            creation.agent,
            creation.setup_terminal_id,
            creation.setup_skipped_reason,
        ))
    }

    pub async fn directory_suggestions(
        &self,
        query: &DirectorySuggestionsQuery,
    ) -> Result<Vec<DirectorySuggestion>, PaseoError> {
        let rpc = Rpc::new(
            "directory_suggestions_request",
            "directory_suggestions_response",
        );
        Ok(self
            .call::<_, DirectorySuggestions>(rpc, query)
            .await?
            .entries)
    }

    pub async fn branch_suggestions(
        &self,
        cwd: &str,
        query: &str,
        limit: u32,
    ) -> Result<Vec<String>, PaseoError> {
        #[derive(Serialize)]
        struct Body<'a> {
            cwd: &'a str,
            query: &'a str,
            limit: u32,
        }
        check_limit("branch suggestion limit", limit, 200)?;
        let rpc = Rpc::new("branch_suggestions_request", "branch_suggestions_response");
        Ok(self
            .call::<_, BranchSuggestions>(rpc, &Body { cwd, query, limit })
            .await?
            .branches)
    }

    pub async fn checkout_status(&self, cwd: &str) -> Result<CheckoutStatus, PaseoError> {
        let rpc = Rpc::new("checkout_status_request", "checkout_status_response");
        self.call(rpc, &CwdTarget { cwd }).await
    }

    async fn checkout_mutation<B: Serialize>(&self, rpc: Rpc, body: &B) -> Result<(), PaseoError> {
        let done = self
            .call_within::<_, CheckoutResult<CheckoutDone>>(
                rpc,
                body,
                Some(CHECKOUT_MUTATION_TIMEOUT),
            )
            .await?;
        rpc.accepted(done.value.success)
    }

    /// Merges the checkout's branch into its base (or `base_ref`).
    pub async fn checkout_merge(
        &self,
        cwd: &str,
        base_ref: Option<&str>,
        strategy: MergeStrategy,
        require_clean_target: bool,
    ) -> Result<(), PaseoError> {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Body<'a> {
            cwd: &'a str,
            #[serde(skip_serializing_if = "Option::is_none")]
            base_ref: Option<&'a str>,
            strategy: MergeStrategy,
            require_clean_target: bool,
        }
        let rpc = Rpc::new("checkout_merge_request", "checkout_merge_response");
        self.checkout_mutation(
            rpc,
            &Body {
                cwd,
                base_ref,
                strategy,
                require_clean_target,
            },
        )
        .await
    }

    /// Merges the checkout's base (or `base_ref`) into its branch.
    pub async fn checkout_merge_from_base(
        &self,
        cwd: &str,
        base_ref: Option<&str>,
        require_clean_target: bool,
    ) -> Result<(), PaseoError> {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Body<'a> {
            cwd: &'a str,
            #[serde(skip_serializing_if = "Option::is_none")]
            base_ref: Option<&'a str>,
            require_clean_target: bool,
        }
        let rpc = Rpc::new(
            "checkout_merge_from_base_request",
            "checkout_merge_from_base_response",
        );
        self.checkout_mutation(
            rpc,
            &Body {
                cwd,
                base_ref,
                require_clean_target,
            },
        )
        .await
    }

    pub async fn checkout_push(&self, cwd: &str) -> Result<(), PaseoError> {
        let rpc = Rpc::new("checkout_push_request", "checkout_push_response");
        self.checkout_mutation(rpc, &CwdTarget { cwd }).await
    }

    /// Opens a pull request for the checkout's branch. The daemon writes a missing title or body.
    pub async fn create_pull_request(
        &self,
        cwd: &str,
        title: Option<&str>,
        body: Option<&str>,
        base_ref: Option<&str>,
    ) -> Result<CreatedPullRequest, PaseoError> {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Body<'a> {
            cwd: &'a str,
            #[serde(skip_serializing_if = "Option::is_none")]
            title: Option<&'a str>,
            #[serde(skip_serializing_if = "Option::is_none")]
            body: Option<&'a str>,
            #[serde(skip_serializing_if = "Option::is_none")]
            base_ref: Option<&'a str>,
        }
        let rpc = Rpc::new("checkout_pr_create_request", "checkout_pr_create_response");
        let created = self
            .call_within::<_, CheckoutResult<PullRequestCreated>>(
                rpc,
                &Body {
                    cwd,
                    title,
                    body,
                    base_ref,
                },
                Some(CHECKOUT_MUTATION_TIMEOUT),
            )
            .await?
            .value;
        let url = rpc.required(created.url, "pull request url")?;
        Ok(CreatedPullRequest::new(url, created.number))
    }

    /// The pull request for the checkout's branch and whether its forge can be used.
    pub async fn pull_request_status(&self, cwd: &str) -> Result<ForgeStatus, PaseoError> {
        let rpc = Rpc::new("checkout_pr_status_request", "checkout_pr_status_response");
        self.call(rpc, &CwdTarget { cwd }).await
    }

    pub async fn pull_request_timeline(
        &self,
        cwd: &str,
        pr_number: u64,
        repo_owner: &str,
        repo_name: &str,
    ) -> Result<PullRequestTimeline, PaseoError> {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Body<'a> {
            cwd: &'a str,
            pr_number: u64,
            repo_owner: &'a str,
            repo_name: &'a str,
        }
        let rpc = Rpc::new(
            "pull_request_timeline_request",
            "pull_request_timeline_response",
        );
        self.call(
            rpc,
            &Body {
                cwd,
                pr_number,
                repo_owner,
                repo_name,
            },
        )
        .await
    }

    pub async fn provider_usage(&self) -> Result<ProviderUsageList, PaseoError> {
        self.require(DaemonFeature::ProviderUsageList)?;
        let rpc = Rpc::new(
            "provider.usage.list.request",
            "provider.usage.list.response",
        );
        self.call_checked(rpc, &NoFields {}).await
    }

    pub async fn list_workspace_scripts(
        &self,
        workspace_id: &str,
    ) -> Result<Vec<WorkspaceScript>, PaseoError> {
        let rpc = Rpc::new(
            "workspace.script.list.request",
            "workspace.script.list.response",
        );
        Ok(self
            .call::<_, ScriptsListed>(rpc, &WorkspaceTarget { workspace_id })
            .await?
            .scripts)
    }

    pub async fn start_workspace_script(
        &self,
        workspace_id: &str,
        script_name: &str,
    ) -> Result<(), PaseoError> {
        let rpc = Rpc::new(
            "workspace.script.start.request",
            "workspace.script.start.response",
        );
        self.call::<_, Failure>(
            rpc,
            &WorkspaceScriptTarget {
                workspace_id,
                script_name,
            },
        )
        .await?;
        Ok(())
    }

    pub async fn stop_workspace_script(
        &self,
        workspace_id: &str,
        script_name: &str,
    ) -> Result<(), PaseoError> {
        let rpc = Rpc::new(
            "workspace.script.stop.request",
            "workspace.script.stop.response",
        );
        self.call::<_, Failure>(
            rpc,
            &WorkspaceScriptTarget {
                workspace_id,
                script_name,
            },
        )
        .await?;
        Ok(())
    }

    /// Allows and runs a workspace setup that the daemon blocked, such as one from an untrusted
    /// pull request source. Returns whether setup started: the daemon starts nothing for a
    /// workspace whose setup wasn't blocked.
    pub async fn run_workspace_setup(&self, workspace_id: &str) -> Result<bool, PaseoError> {
        self.require(DaemonFeature::WorkspaceSetupRun)?;
        let rpc = Rpc::new(
            "workspace.setup.run.request",
            "workspace.setup.run.response",
        );
        let setup = self
            .call::<_, SetupStarted>(rpc, &WorkspaceTarget { workspace_id })
            .await?;
        Ok(setup.started)
    }

    pub async fn list_schedules(&self) -> Result<Vec<Schedule>, PaseoError> {
        let rpc = Rpc::new("schedule/list", "schedule/list/response");
        Ok(self
            .call::<_, SchedulesListed>(rpc, &NoFields {})
            .await?
            .schedules)
    }

    pub async fn create_schedule(&self, schedule: &ScheduleCreate) -> Result<Schedule, PaseoError> {
        let rpc = Rpc::new("schedule/create", "schedule/create/response");
        let created = self.call::<_, ScheduleFetched>(rpc, schedule).await?;
        rpc.required(created.schedule, "schedule")
    }

    async fn schedule_change(
        &self,
        rpc: Rpc,
        schedule_id: &str,
        timeout: Option<Duration>,
    ) -> Result<Schedule, PaseoError> {
        let changed = self
            .call_within::<_, ScheduleFetched>(rpc, &ScheduleIdTarget { schedule_id }, timeout)
            .await?;
        rpc.required(changed.schedule, "schedule")
    }

    pub async fn pause_schedule(&self, schedule_id: &str) -> Result<Schedule, PaseoError> {
        let rpc = Rpc::new("schedule/pause", "schedule/pause/response");
        self.schedule_change(rpc, schedule_id, Some(REQUEST_TIMEOUT))
            .await
    }

    pub async fn resume_schedule(&self, schedule_id: &str) -> Result<Schedule, PaseoError> {
        let rpc = Rpc::new("schedule/resume", "schedule/resume/response");
        self.schedule_change(rpc, schedule_id, Some(REQUEST_TIMEOUT))
            .await
    }

    /// Runs a schedule now. The daemon answers only once the run has finished, so this waits
    /// without a timeout.
    pub async fn run_schedule_once(&self, schedule_id: &str) -> Result<(), PaseoError> {
        let rpc = Rpc::new("schedule/run-once", "schedule/run-once/response");
        self.schedule_change(rpc, schedule_id, None).await?;
        Ok(())
    }

    pub async fn delete_schedule(&self, schedule_id: &str) -> Result<(), PaseoError> {
        let rpc = Rpc::new("schedule/delete", "schedule/delete/response");
        self.call::<_, Failure>(rpc, &ScheduleIdTarget { schedule_id })
            .await?;
        Ok(())
    }

    pub async fn schedule_runs(&self, schedule_id: &str) -> Result<Vec<ScheduleRun>, PaseoError> {
        let rpc = Rpc::new("schedule/logs", "schedule/logs/response");
        Ok(self
            .call::<_, ScheduleRunsListed>(rpc, &ScheduleIdTarget { schedule_id })
            .await?
            .runs)
    }

    pub async fn write_text_file(
        &self,
        cwd: &str,
        parent_path: &str,
        name: &str,
        content: &str,
    ) -> Result<String, PaseoError> {
        let parents = parent_path
            .split('/')
            .filter(|segment| !matches!(*segment, "" | "."))
            .collect::<Vec<_>>();
        if parent_path.starts_with('/') || parents.contains(&"..") {
            return Err(ArgumentError::PathOutsideWorkspace {
                path: parent_path.to_owned(),
            }
            .into());
        }
        if matches!(name, "" | "." | "..") || name.contains('/') {
            return Err(ArgumentError::FileNameInvalid {
                name: name.to_owned(),
            }
            .into());
        }
        let path = parents
            .iter()
            .copied()
            .chain([name])
            .collect::<Vec<_>>()
            .join("/");
        let mut expected = FileRevision::new(String::new(), None);
        let mut created = false;
        let mut refreshed = false;
        loop {
            let write = FileWrite::text(path.as_str(), content, &expected);
            let conflict = match self.write_file(cwd, &write).await? {
                FileWriteOutcome::Written(_) => return Ok(path),
                FileWriteOutcome::Error { error } => {
                    return Err(PaseoError::rpc(FILE_WRITE.kind, error));
                }
                FileWriteOutcome::Conflict { version } => version,
                FileWriteOutcome::Unknown => {
                    return Err(PaseoError::request(
                        FILE_WRITE.kind,
                        RequestFailure::UnrecognisedOutcome,
                    ));
                }
            };
            match conflict.state() {
                FileState::Error { error } => {
                    return Err(PaseoError::rpc(FILE_WRITE.kind, error.as_str()));
                }
                FileState::Missing if !created => {
                    self.create_file(cwd, &parents, name).await?;
                    created = true;
                }
                FileState::Ready(ready) if !refreshed => {
                    expected = ready.revision().clone();
                    refreshed = true;
                }
                FileState::Missing | FileState::Ready(_) | FileState::Unknown => {
                    return Err(PaseoError::request(
                        FILE_WRITE.kind,
                        RequestFailure::FileChanged,
                    ));
                }
            }
        }
    }

    async fn create_file(&self, cwd: &str, parents: &[&str], name: &str) -> Result<(), PaseoError> {
        let mut parent = String::from(".");
        for segment in parents {
            let listing = self.list_directory(cwd, &parent).await?;
            let existing = listing
                .entries()
                .iter()
                .find(|entry| entry.name() == *segment)
                .map(FileEntry::kind);
            match existing {
                Some(EntryKind::Directory) => {}
                Some(EntryKind::File | EntryKind::Unknown) => {
                    return Err(PaseoError::request(
                        FILE_ENTRY_CREATE.kind,
                        RequestFailure::NotADirectory {
                            path: format!("{parent}/{segment}"),
                        },
                    ));
                }
                None => {
                    self.create_file_entry(cwd, &parent, segment, EntryKind::Directory)
                        .await?;
                }
            }
            parent = match parent.as_str() {
                "." => (*segment).to_owned(),
                _ => format!("{parent}/{segment}"),
            };
        }
        self.create_file_entry(cwd, &parent, name, EntryKind::File)
            .await?;
        Ok(())
    }

    pub async fn list_terminals(
        &self,
        cwd: Option<&str>,
        workspace_id: Option<&str>,
    ) -> Result<Vec<TerminalInfo>, PaseoError> {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Body<'a> {
            #[serde(skip_serializing_if = "Option::is_none")]
            cwd: Option<&'a str>,
            #[serde(skip_serializing_if = "Option::is_none")]
            workspace_id: Option<&'a str>,
        }
        let rpc = Rpc::new("list_terminals_request", "list_terminals_response");
        Ok(self
            .call::<_, TerminalsListed>(rpc, &Body { cwd, workspace_id })
            .await?
            .terminals)
    }

    pub async fn create_terminal(
        &self,
        cwd: &str,
        options: &CreateTerminalOptions,
    ) -> Result<TerminalInfo, PaseoError> {
        #[derive(Serialize)]
        struct Body<'a> {
            cwd: &'a str,
            #[serde(flatten)]
            options: &'a CreateTerminalOptions,
        }
        let rpc = Rpc::new("create_terminal_request", "create_terminal_response");
        let created = self
            .call::<_, TerminalCreated>(rpc, &Body { cwd, options })
            .await?;
        rpc.required(created.terminal, "terminal")
    }

    pub async fn rename_terminal(&self, terminal_id: &str, title: &str) -> Result<(), PaseoError> {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Body<'a> {
            terminal_id: &'a str,
            title: &'a str,
        }
        let rpc = Rpc::new("terminal.rename.request", "terminal.rename.response");
        let response = self
            .call::<_, TerminalRenamed>(rpc, &Body { terminal_id, title })
            .await?;
        rpc.accepted(response.success)
    }

    pub async fn kill_terminal(&self, terminal_id: &str) -> Result<(), PaseoError> {
        #[derive(Deserialize)]
        struct Killed {
            success: bool,
        }
        impl Response for Killed {}
        let rpc = Rpc::new("kill_terminal_request", "kill_terminal_response");
        let killed = self
            .call::<_, Killed>(rpc, &TerminalTarget { terminal_id })
            .await?;
        rpc.accepted(killed.success)
    }

    /// The terminal's lines, without escape sequences. Unlike [`Self::subscribe_terminal`],
    /// this works on daemons without owned subscriptions.
    pub async fn capture_terminal(&self, terminal_id: &str) -> Result<Vec<String>, PaseoError> {
        #[derive(Deserialize)]
        struct Captured {
            lines: Vec<String>,
        }
        impl Response for Captured {}
        let rpc = Rpc::new("capture_terminal_request", "capture_terminal_response");
        Ok(self
            .call::<_, Captured>(rpc, &TerminalTarget { terminal_id })
            .await?
            .lines)
    }

    /// Types `data` into the terminal without subscribing to it.
    pub fn send_terminal_input(&self, terminal_id: &str, data: &str) -> Result<(), PaseoError> {
        #[derive(Serialize)]
        #[serde(tag = "type", rename = "input")]
        struct Input<'a> {
            data: &'a str,
        }
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Body<'a> {
            terminal_id: &'a str,
            message: Input<'a>,
        }
        let text = session_message(
            "terminal_input",
            None,
            &Body {
                terminal_id,
                message: Input { data },
            },
        )?;
        self.inner.send(Frame::Text(text))
    }

    /// Uploads `bytes` byte for byte into the daemon's `$PASEO_HOME/uploads`.
    pub async fn upload_file(
        &self,
        file_name: &str,
        mime_type: &str,
        modified_at: &str,
        bytes: &[u8],
    ) -> Result<UploadedFile, PaseoError> {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Body<'a> {
            file_name: &'a str,
            mime_type: &'a str,
            size: usize,
            modified_at: &'a str,
        }
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Metadata<'a> {
            mime: &'a str,
            size: usize,
            encoding: &'static str,
            modified_at: &'a str,
            file_name: &'a str,
        }
        let rpc = Rpc::new("file.upload.request", "file.upload.response");
        let request = Request {
            kind: rpc.kind,
            response_type: rpc.response,
            body: &Body {
                file_name,
                mime_type,
                size: bytes.len(),
                modified_at,
            },
        };
        let pending = self.inner.submit(&request, None, None)?;
        let metadata = serde_json::to_vec(&Metadata {
            mime: mime_type,
            size: bytes.len(),
            encoding: "binary",
            modified_at,
            file_name,
        })?;
        let request_id = pending.request_id();
        let frames = std::iter::once(FileTransferFrame::Begin {
            request_id,
            metadata: &metadata,
        })
        .chain(
            bytes
                .chunks(UPLOAD_CHUNK_BYTES)
                .map(|payload| FileTransferFrame::Chunk {
                    request_id,
                    payload,
                }),
        )
        .chain(std::iter::once(FileTransferFrame::End { request_id }));
        for frame in frames {
            let frame = frame
                .encode()
                .ok_or_else(|| ArgumentError::FileNameInvalid {
                    name: file_name.to_owned(),
                })?;
            self.inner.send(Frame::Binary(frame))?;
        }
        let payload = pending.wait(Some(FILE_TRANSFER_TIMEOUT)).await?;
        let uploaded = Self::parse::<FileUploaded>(rpc, &payload)?;
        rpc.required(uploaded.file, "file")
    }

    /// Asks the daemon's supervisor to replace its worker. The supervisor keeps running, so the
    /// new worker loads the daemon's code from disk again.
    pub async fn restart_server(&self, reason: &str) -> Result<(), PaseoError> {
        #[derive(Serialize)]
        struct Body<'a> {
            reason: &'a str,
        }
        let rpc = Rpc::new("restart_server_request", "status");
        let response = self
            .call::<_, ServerStatusReply>(rpc, &Body { reason })
            .await?;
        response.confirm(rpc, ServerStatus::RestartRequested)
    }

    pub async fn daemon_status(&self) -> Result<DaemonStatus, PaseoError> {
        self.require(DaemonFeature::DaemonStatusRpc)?;
        self.call(
            Rpc::new("daemon.get_status.request", "daemon.get_status.response"),
            &NoFields {},
        )
        .await
    }

    pub async fn list_directory(
        &self,
        cwd: &str,
        path: &str,
    ) -> Result<DirectoryListing, PaseoError> {
        #[derive(Serialize)]
        struct Body<'a> {
            cwd: &'a str,
            path: &'a str,
            mode: &'static str,
        }
        let listed = self
            .call::<_, Explored>(
                FILE_EXPLORER,
                &Body {
                    cwd,
                    path,
                    mode: "list",
                },
            )
            .await?;
        FILE_EXPLORER.required(listed.directory, "directory")
    }

    pub async fn read_file(
        &self,
        cwd: &str,
        path: &str,
        max_bytes: Option<NonZeroU64>,
    ) -> Result<FileContent, PaseoError> {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Body<'a> {
            cwd: &'a str,
            path: &'a str,
            mode: &'static str,
            accept_binary: bool,
            #[serde(skip_serializing_if = "Option::is_none")]
            max_bytes: Option<NonZeroU64>,
        }
        let (finished, downloaded) = oneshot::channel();
        let request = Request {
            kind: FILE_EXPLORER.kind,
            response_type: FILE_EXPLORER.response,
            body: &Body {
                cwd,
                path,
                mode: "file",
                accept_binary: true,
                max_bytes,
            },
        };
        let pending = self.inner.submit(
            &request,
            None,
            Some(Tracking::Download(Download::new(path, finished))),
        )?;
        let reply = pin!(pending.wait(Some(FILE_TRANSFER_TIMEOUT)));
        let payload = match future::select(downloaded, reply).await {
            Either::Left((Ok(content), _)) => return content,
            Either::Left((Err(_), reply)) => reply.await?,
            Either::Right((reply, _)) => reply?,
        };
        let explored = Self::parse::<Explored>(FILE_EXPLORER, &payload)?;
        let file = FILE_EXPLORER.required(explored.file, "file")?;
        file.into_content().map_err(|error| {
            PaseoError::decode(FILE_EXPLORER.response, serde_json::Error::custom(error))
        })
    }

    pub async fn observe_file(
        &self,
        cwd: &str,
        path: &str,
    ) -> Result<Subscription<FileVersion, FileVersion>, PaseoError> {
        #[derive(Serialize)]
        struct Body<'a> {
            cwd: &'a str,
            path: &'a str,
        }
        self.require(DaemonFeature::WorkspaceFileEditing)?;
        let rpc = Rpc::new("fs.file.subscribe.request", "fs.file.subscribe.response");
        self.observe(
            rpc,
            &Body { cwd, path },
            |subscribed: Checked<FileSubscribed>| subscribed.value.initial,
        )
        .await
    }

    pub async fn write_file(
        &self,
        cwd: &str,
        write: &FileWrite,
    ) -> Result<FileWriteOutcome, PaseoError> {
        #[derive(Serialize)]
        struct Body<'a> {
            cwd: &'a str,
            #[serde(flatten)]
            write: &'a FileWrite,
        }
        self.require(DaemonFeature::WorkspaceFileEditing)?;
        if write.content_encoding() == ContentEncoding::Base64 {
            self.require(DaemonFeature::FileByteWrites)?;
        }
        Ok(self
            .call::<_, FileWritten>(FILE_WRITE, &Body { cwd, write })
            .await?
            .result)
    }

    pub async fn create_file_entry(
        &self,
        cwd: &str,
        parent_path: &str,
        name: &str,
        kind: EntryKind,
    ) -> Result<String, PaseoError> {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Body<'a> {
            cwd: &'a str,
            parent_path: &'a str,
            name: &'a str,
            kind: EntryKind,
        }
        self.require(DaemonFeature::FsEntryOps)?;
        let body = Body {
            cwd,
            parent_path,
            name,
            kind,
        };
        let created = self
            .call::<_, EntryCreated>(FILE_ENTRY_CREATE, &body)
            .await?;
        FILE_ENTRY_CREATE.accepted(created.success)?;
        FILE_ENTRY_CREATE.required(created.path, "path")
    }

    pub async fn rename_file_entry(
        &self,
        cwd: &str,
        path: &str,
        name: &str,
    ) -> Result<String, PaseoError> {
        #[derive(Serialize)]
        struct Body<'a> {
            cwd: &'a str,
            path: &'a str,
            name: &'a str,
        }
        self.require(DaemonFeature::FsEntryOps)?;
        let rpc = Rpc::new("fs.entry.rename.request", "fs.entry.rename.response");
        let renamed = self
            .call::<_, EntryRenamed>(rpc, &Body { cwd, path, name })
            .await?;
        rpc.accepted(renamed.success)?;
        rpc.required(renamed.renamed_path, "path")
    }

    pub async fn duplicate_file_entry(&self, cwd: &str, path: &str) -> Result<String, PaseoError> {
        self.require(DaemonFeature::FsEntryDuplicate)?;
        let rpc = Rpc::new("fs.entry.duplicate.request", "fs.entry.duplicate.response");
        let duplicated = self
            .call::<_, EntryDuplicated>(rpc, &PathTarget { cwd, path })
            .await?;
        rpc.accepted(duplicated.success)?;
        rpc.required(duplicated.duplicated_path, "path")
    }

    pub async fn delete_file_entry(&self, cwd: &str, path: &str) -> Result<(), PaseoError> {
        self.require(DaemonFeature::FsEntryOps)?;
        let rpc = Rpc::new("fs.entry.delete.request", "fs.entry.delete.response");
        let deleted = self
            .call::<_, EntryDeleted>(rpc, &PathTarget { cwd, path })
            .await?;
        rpc.accepted(deleted.success)
    }

    pub async fn file_download_token(
        &self,
        cwd: &str,
        path: &str,
    ) -> Result<DownloadToken, PaseoError> {
        let rpc = Rpc::new(
            "file_download_token_request",
            "file_download_token_response",
        );
        self.call_checked(rpc, &PathTarget { cwd, path }).await
    }

    pub async fn rename_project(
        &self,
        project_id: &str,
        custom_name: Option<&str>,
    ) -> Result<Option<String>, PaseoError> {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Body<'a> {
            project_id: &'a str,
            custom_name: Option<&'a str>,
        }
        let rpc = Rpc::new("project.rename.request", "project.rename.response");
        let renamed = self
            .call::<_, ProjectRenamed>(
                rpc,
                &Body {
                    project_id,
                    custom_name,
                },
            )
            .await?;
        rpc.accepted(renamed.accepted)?;
        Ok(renamed.custom_name)
    }

    pub async fn remove_project(&self, project_id: &str) -> Result<Vec<String>, PaseoError> {
        self.require(DaemonFeature::ProjectRemove)?;
        let rpc = Rpc::new("project.remove.request", "project.remove.response");
        let removed = self
            .call::<_, ProjectRemoved>(rpc, &ProjectTarget { project_id })
            .await?;
        rpc.accepted(removed.accepted)?;
        Ok(removed.removed_workspace_ids)
    }

    pub async fn project_icon(&self, project_id: &str) -> Result<Option<ProjectIcon>, PaseoError> {
        self.require(DaemonFeature::ProjectCustomIcon)?;
        let rpc = Rpc::new("project.icon.get.request", "project.icon.get.response");
        let fetched = self
            .call::<_, ProjectIconFetched>(rpc, &ProjectTarget { project_id })
            .await?;
        fetched
            .icon
            .map(|icon| {
                STANDARD
                    .decode(icon.data)
                    .map(|bytes| ProjectIcon::new(icon.mime_type, bytes))
                    .map_err(|error| {
                        PaseoError::decode(rpc.response, serde_json::Error::custom(error))
                    })
            })
            .transpose()
    }

    pub async fn set_project_icon(
        &self,
        project_id: &str,
        icon: Option<&[u8]>,
    ) -> Result<(), PaseoError> {
        #[derive(Serialize)]
        #[serde(tag = "type", rename_all = "snake_case")]
        enum Source {
            Automatic,
            Upload { data: String },
        }
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Body<'a> {
            project_id: &'a str,
            source: Source,
        }
        self.require(DaemonFeature::ProjectCustomIcon)?;
        let source = match icon {
            Some(bytes) => Source::Upload {
                data: STANDARD.encode(bytes),
            },
            None => Source::Automatic,
        };
        let rpc = Rpc::new("project.icon.set.request", "project.icon.set.response");
        let set = self
            .call::<_, Accepted>(rpc, &Body { project_id, source })
            .await?;
        rpc.accepted(set.accepted)
    }

    pub async fn read_project_config(&self, repo_root: &str) -> Result<ProjectConfig, PaseoError> {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Body<'a> {
            repo_root: &'a str,
        }
        let rpc = Rpc::new(
            "read_project_config_request",
            "read_project_config_response",
        );
        Self::project_config(rpc, self.call(rpc, &Body { repo_root }).await?)
    }

    pub async fn write_project_config(
        &self,
        repo_root: &str,
        config: &Map<String, Value>,
        expected: Option<ProjectConfigRevision>,
    ) -> Result<ProjectConfig, PaseoError> {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Body<'a> {
            repo_root: &'a str,
            config: &'a Map<String, Value>,
            expected_revision: Option<ProjectConfigRevision>,
        }
        let rpc = Rpc::new(
            "write_project_config_request",
            "write_project_config_response",
        );
        let body = Body {
            repo_root,
            config,
            expected_revision: expected,
        };
        Self::project_config(rpc, self.call(rpc, &body).await?)
    }

    fn project_config(rpc: Rpc, reply: ProjectConfigReply) -> Result<ProjectConfig, PaseoError> {
        match reply.error {
            Some(error) => Err(PaseoError::request(
                rpc.kind,
                RequestFailure::ProjectConfig(error),
            )),
            None => Ok(reply.config),
        }
    }

    pub async fn list_workspace_labels(&self) -> Result<LabelCatalogue, PaseoError> {
        self.require(DaemonFeature::WorkspaceLabels)?;
        self.call_checked(WORKSPACE_LABEL_LIST, &NoFields {}).await
    }

    pub async fn observe_workspace_labels(
        &self,
        since: Option<&LabelCursor>,
    ) -> Result<Subscription<LabelCatalogue, LabelUpdate>, PaseoError> {
        #[derive(Serialize)]
        struct Body<'a> {
            subscribe: NoFields,
            #[serde(skip_serializing_if = "Option::is_none")]
            sync: Option<&'a LabelCursor>,
        }
        self.require(DaemonFeature::WorkspaceLabels)?;
        let body = Body {
            subscribe: NoFields {},
            sync: since,
        };
        self.observe(
            WORKSPACE_LABEL_LIST,
            &body,
            |listed: Checked<LabelCatalogue>| listed.value,
        )
        .await
    }

    pub async fn set_workspace_label(
        &self,
        workspace_id: &str,
        label: &WorkspaceLabel,
        assigned: bool,
    ) -> Result<LabelAssignment, PaseoError> {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Body<'a> {
            workspace_id: &'a str,
            label: &'a WorkspaceLabel,
            assigned: bool,
        }
        self.require(DaemonFeature::WorkspaceLabels)?;
        let rpc = Rpc::new(
            "workspace.label.assignment.set.request",
            "workspace.label.assignment.set.response",
        );
        let body = Body {
            workspace_id,
            label,
            assigned,
        };
        self.call_checked(rpc, &body).await
    }

    pub async fn edit_workspace_label(&self, edit: &LabelEdit) -> Result<EditedLabel, PaseoError> {
        self.require(DaemonFeature::WorkspaceLabels)?;
        let rpc = Rpc::new(
            "workspace.label.update.request",
            "workspace.label.update.response",
        );
        self.call_checked(rpc, edit).await
    }

    pub async fn delete_workspace_label(&self, name: &str) -> Result<u64, PaseoError> {
        self.require(DaemonFeature::WorkspaceLabels)?;
        let rpc = Rpc::new(
            "workspace.label.delete.request",
            "workspace.label.delete.response",
        );
        Ok(self
            .call::<_, LabelCount>(rpc, &NameTarget { name })
            .await?
            .affected_workspace_count)
    }

    pub async fn inspect_label_deletion(&self, name: &str) -> Result<u64, PaseoError> {
        self.require(DaemonFeature::WorkspaceLabels)?;
        let rpc = Rpc::new(
            "workspace.label.delete.inspect.request",
            "workspace.label.delete.inspect.response",
        );
        Ok(self
            .call::<_, LabelCount>(rpc, &NameTarget { name })
            .await?
            .affected_workspace_count)
    }

    pub async fn set_workspace_pinned(
        &self,
        workspace_id: &str,
        pinned: bool,
    ) -> Result<Option<String>, PaseoError> {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Body<'a> {
            workspace_id: &'a str,
            pinned: bool,
        }
        self.require(DaemonFeature::WorkspacePinning)?;
        let rpc = Rpc::new("workspace.pin.set.request", "workspace.pin.set.response");
        let response = self
            .call::<_, Pinned>(
                rpc,
                &Body {
                    workspace_id,
                    pinned,
                },
            )
            .await?;
        rpc.accepted(response.accepted)?;
        Ok(response.pinned_at)
    }

    pub async fn mark_workspace_unread(
        &self,
        workspace_id: &str,
    ) -> Result<Option<String>, PaseoError> {
        self.require(DaemonFeature::WorkspaceMarkUnread)?;
        let rpc = Rpc::new(
            "workspace.mark_unread.request",
            "workspace.mark_unread.response",
        );
        let marked = self
            .call::<_, MarkedUnread>(rpc, &WorkspaceTarget { workspace_id })
            .await?;
        rpc.accepted(marked.success)?;
        Ok(marked.marked_agent_id)
    }

    pub async fn clear_workspace_attention(
        &self,
        workspace_ids: &[String],
    ) -> Result<Vec<AttentionCleared>, PaseoError> {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Body<'a> {
            workspace_id: &'a [String],
        }
        let rpc = Rpc::new(
            "workspace.clear_attention.request",
            "workspace.clear_attention.response",
        );
        let cleared = self
            .call::<_, AttentionClearedAll>(
                rpc,
                &Body {
                    workspace_id: workspace_ids,
                },
            )
            .await?;
        rpc.accepted(cleared.success)?;
        Ok(cleared.results)
    }

    pub async fn inspect_workspace_recovery(
        &self,
        workspace_id: &str,
    ) -> Result<WorkspaceRecovery, PaseoError> {
        self.require(DaemonFeature::WorkspaceRecovery)?;
        let rpc = Rpc::new(
            "workspace.recovery.inspect.request",
            "workspace.recovery.inspect.response",
        );
        let inspected = self
            .call::<_, RecoveryInspected>(rpc, &WorkspaceTarget { workspace_id })
            .await?;
        rpc.required(inspected.state, "recovery state")
    }

    pub async fn restore_workspace(&self, workspace_id: &str) -> Result<(), PaseoError> {
        self.require(DaemonFeature::WorkspaceRecovery)?;
        let rpc = Rpc::new(
            "workspace.recovery.restore.request",
            "workspace.recovery.restore.response",
        );
        let restored = self
            .call::<_, Accepted>(rpc, &WorkspaceTarget { workspace_id })
            .await?;
        rpc.accepted(restored.accepted)
    }

    pub async fn observe_workspace_setup(
        &self,
        workspace_id: &str,
    ) -> Result<Subscription<Option<WorkspaceSetupSnapshot>, WorkspaceSetupProgress>, PaseoError>
    {
        self.require(DaemonFeature::WorkspaceSetupObservation)?;
        let rpc = Rpc::new(
            "workspace.setup.subscribe.request",
            "workspace.setup.subscribe.response",
        );
        self.observe(
            rpc,
            &WorkspaceTarget { workspace_id },
            |status: SetupStatus| status.snapshot,
        )
        .await
    }

    pub async fn search_github_repositories(
        &self,
        query: &str,
        limit: Option<u32>,
    ) -> Result<RepositorySearch, PaseoError> {
        #[derive(Serialize)]
        struct Body<'a> {
            query: &'a str,
            #[serde(skip_serializing_if = "Option::is_none")]
            limit: Option<u32>,
        }
        self.require(DaemonFeature::WorkspaceGithubRepositorySearch)?;
        if let Some(limit) = limit {
            check_limit("repository search limit", limit, FORGE_SEARCH_MAX_LIMIT)?;
        }
        let rpc = Rpc::new(
            "workspace.github.search_repositories.request",
            "workspace.github.search_repositories.response",
        );
        self.call(rpc, &Body { query, limit }).await
    }

    pub async fn close_items(
        &self,
        agent_ids: &[String],
        terminal_ids: &[String],
    ) -> Result<ClosedItems, PaseoError> {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Body<'a> {
            agent_ids: &'a [String],
            terminal_ids: &'a [String],
        }
        let rpc = Rpc::new("close_items_request", "close_items_response");
        self.call_checked(
            rpc,
            &Body {
                agent_ids,
                terminal_ids,
            },
        )
        .await
    }

    pub async fn observe_creation(
        &self,
        kind: CreationKind,
        idempotency_key: &str,
    ) -> Result<Subscription<Option<CreationSnapshot>, CreationSnapshot>, PaseoError> {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Body<'a> {
            kind: CreationKind,
            idempotency_key: &'a str,
            subscribe: bool,
        }
        self.require(DaemonFeature::CreationLifecycle)?;
        let rpc = Rpc::new("creation.subscribe.request", "creation.subscribe.response");
        let body = Body {
            kind,
            idempotency_key,
            subscribe: true,
        };
        self.observe(rpc, &body, |subscribed: CreationSubscribed| {
            subscribed.snapshot
        })
        .await
    }

    pub async fn checkout_diff(
        &self,
        cwd: &str,
        compare: &DiffCompare,
    ) -> Result<CheckoutDiff, PaseoError> {
        #[derive(Serialize)]
        struct Body<'a> {
            cwd: &'a str,
            compare: &'a DiffCompare,
        }
        self.require_diff_mode(compare)?;
        let rpc = Rpc::new("checkout.diff.get.request", "checkout.diff.get.response");
        self.call(rpc, &Body { cwd, compare }).await
    }

    fn require_diff_mode(&self, compare: &DiffCompare) -> Result<(), PaseoError> {
        if compare.mode() == DiffMode::All {
            self.require(DaemonFeature::CheckoutDiffAll)?;
        }
        Ok(())
    }

    pub async fn checkout_commit(
        &self,
        cwd: &str,
        message: Option<&str>,
        add_all: bool,
    ) -> Result<(), PaseoError> {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Body<'a> {
            cwd: &'a str,
            #[serde(skip_serializing_if = "Option::is_none")]
            message: Option<&'a str>,
            add_all: bool,
        }
        let rpc = Rpc::new("checkout_commit_request", "checkout_commit_response");
        self.checkout_mutation(
            rpc,
            &Body {
                cwd,
                message,
                add_all,
            },
        )
        .await
    }

    pub async fn checkout_pull(&self, cwd: &str) -> Result<(), PaseoError> {
        let rpc = Rpc::new("checkout_pull_request", "checkout_pull_response");
        self.checkout_mutation(rpc, &CwdTarget { cwd }).await
    }

    pub async fn checkout_refresh(&self, cwd: &str) -> Result<(), PaseoError> {
        self.require(DaemonFeature::CheckoutRefresh)?;
        let rpc = Rpc::new("checkout.refresh.request", "checkout.refresh.response");
        self.checkout_mutation(rpc, &CwdTarget { cwd }).await
    }

    pub async fn discard_changes(&self, cwd: &str, paths: &[String]) -> Result<(), PaseoError> {
        #[derive(Serialize)]
        struct Body<'a> {
            cwd: &'a str,
            paths: &'a [String],
        }
        self.require(DaemonFeature::CheckoutDiscardChanges)?;
        let rpc = Rpc::new(
            "checkout.discard_changes.request",
            "checkout.discard_changes.response",
        );
        self.checkout_mutation(rpc, &Body { cwd, paths }).await
    }

    pub async fn merge_pull_request(
        &self,
        cwd: &str,
        method: MergeMethod,
    ) -> Result<(), PaseoError> {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Body<'a> {
            cwd: &'a str,
            merge_method: MergeMethod,
        }
        let rpc = Rpc::new("checkout_pr_merge_request", "checkout_pr_merge_response");
        self.checkout_mutation(
            rpc,
            &Body {
                cwd,
                merge_method: method,
            },
        )
        .await
    }

    pub async fn set_pull_request_auto_merge(
        &self,
        cwd: &str,
        enabled: bool,
        method: Option<MergeMethod>,
    ) -> Result<(), PaseoError> {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Body<'a> {
            cwd: &'a str,
            enabled: bool,
            #[serde(skip_serializing_if = "Option::is_none")]
            merge_method: Option<MergeMethod>,
        }
        let rpc = self.forge_rpc(
            DaemonFeature::CheckoutForgeSetAutoMerge,
            Rpc::new(
                "checkout.forge.set_auto_merge.request",
                "checkout.forge.set_auto_merge.response",
            ),
            DaemonFeature::CheckoutGithubSetAutoMerge,
            Rpc::new(
                "checkout.github.set_auto_merge.request",
                "checkout.github.set_auto_merge.response",
            ),
        )?;
        let body = Body {
            cwd,
            enabled,
            merge_method: method,
        };
        self.checkout_mutation(rpc, &body).await
    }

    pub async fn check_details(
        &self,
        cwd: &str,
        query: &CheckDetailsQuery,
    ) -> Result<Option<CheckDetails>, PaseoError> {
        #[derive(Serialize)]
        struct Body<'a> {
            cwd: &'a str,
            #[serde(flatten)]
            query: &'a CheckDetailsQuery,
        }
        let rpc = self.forge_rpc(
            DaemonFeature::ForgeCheckDetails,
            Rpc::new(
                "checkout.forge.get_check_details.request",
                "checkout.forge.get_check_details.response",
            ),
            DaemonFeature::GithubCheckDetails,
            Rpc::new(
                "checkout.github.get_check_details.request",
                "checkout.github.get_check_details.response",
            ),
        )?;
        Ok(self
            .call::<_, CheckoutResult<CheckDetailsFetched>>(rpc, &Body { cwd, query })
            .await?
            .value
            .details)
    }

    fn forge_rpc(
        &self,
        forge: DaemonFeature,
        forge_rpc: Rpc,
        github: DaemonFeature,
        github_rpc: Rpc,
    ) -> Result<Rpc, PaseoError> {
        let info = self.server_info();
        if !info.supports(forge) && info.supports(github) {
            return Ok(github_rpc);
        }
        self.require(forge)?;
        Ok(forge_rpc)
    }

    pub async fn list_commits(&self, cwd: &str) -> Result<CommitList, PaseoError> {
        self.require(DaemonFeature::CommitsList)?;
        let rpc = Rpc::new(
            "checkout.commits.list.request",
            "checkout.commits.list.response",
        );
        Ok(self
            .call::<_, CheckoutResult<CommitList>>(rpc, &CwdTarget { cwd })
            .await?
            .value)
    }

    pub async fn list_commit_page(
        &self,
        cwd: &str,
        cursor: Option<&str>,
        limit: u16,
    ) -> Result<CommitPage, PaseoError> {
        #[derive(Serialize)]
        struct Body<'a> {
            cwd: &'a str,
            cursor: Option<&'a str>,
            limit: u16,
        }
        self.require(DaemonFeature::CommitsPagination)?;
        let rpc = Rpc::new(
            "checkout.commits.page.request",
            "checkout.commits.page.response",
        );
        Ok(self
            .call::<_, CheckoutResult<CommitPage>>(rpc, &Body { cwd, cursor, limit })
            .await?
            .value)
    }

    pub async fn commit_files(&self, cwd: &str, sha: &str) -> Result<Vec<CommitFile>, PaseoError> {
        #[derive(Serialize)]
        struct Body<'a> {
            cwd: &'a str,
            sha: &'a str,
        }
        #[derive(Deserialize)]
        struct FilesFetched {
            files: Vec<CommitFile>,
        }
        self.require(DaemonFeature::CommitsPagination)?;
        let rpc = Rpc::new(
            "checkout.commits.files.request",
            "checkout.commits.files.response",
        );
        Ok(self
            .call::<_, CheckoutResult<FilesFetched>>(rpc, &Body { cwd, sha })
            .await?
            .value
            .files)
    }

    pub async fn commit_file_diff(
        &self,
        cwd: &str,
        sha: &str,
        path: &str,
    ) -> Result<Option<DiffFile>, PaseoError> {
        #[derive(Serialize)]
        struct Body<'a> {
            cwd: &'a str,
            sha: &'a str,
            path: &'a str,
        }
        self.require(DaemonFeature::CommitsList)?;
        let rpc = Rpc::new(
            "checkout.commits.file_diff.request",
            "checkout.commits.file_diff.response",
        );
        Ok(self
            .call::<_, CheckoutResult<FileDiffFetched>>(rpc, &Body { cwd, sha, path })
            .await?
            .value
            .file)
    }

    pub async fn commit_before(
        &self,
        cwd: &str,
        before: &str,
    ) -> Result<Option<String>, PaseoError> {
        #[derive(Serialize)]
        struct Body<'a> {
            cwd: &'a str,
            before: &'a str,
        }
        self.require(DaemonFeature::CheckoutCommitBefore)?;
        let rpc = Rpc::new(
            "checkout.commit_before.request",
            "checkout.commit_before.response",
        );
        Ok(self
            .call::<_, CheckoutResult<CommitFound>>(rpc, &Body { cwd, before })
            .await?
            .value
            .commit)
    }

    pub async fn file_at_revision(
        &self,
        cwd: &str,
        path: &str,
        revision: &str,
        max_bytes: Option<NonZeroU64>,
    ) -> Result<String, PaseoError> {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Body<'a> {
            cwd: &'a str,
            path: &'a str,
            revision: &'a str,
            #[serde(skip_serializing_if = "Option::is_none")]
            max_bytes: Option<NonZeroU64>,
        }
        self.require(DaemonFeature::CheckoutFileAtRevision)?;
        let rpc = Rpc::new(
            "checkout.file_at_revision.request",
            "checkout.file_at_revision.response",
        );
        let body = Body {
            cwd,
            path,
            revision,
            max_bytes,
        };
        let fetched = self
            .call::<_, CheckoutResult<FileAtRevisionFetched>>(rpc, &body)
            .await?
            .value;
        rpc.required(fetched.content, "content")
    }

    pub async fn switch_branch(
        &self,
        cwd: &str,
        branch: &str,
    ) -> Result<Option<BranchSource>, PaseoError> {
        let rpc = Rpc::new(
            "checkout_switch_branch_request",
            "checkout_switch_branch_response",
        );
        let switched = self
            .call_within::<_, CheckoutResult<BranchSwitched>>(
                rpc,
                &BranchTarget { cwd, branch },
                Some(CHECKOUT_MUTATION_TIMEOUT),
            )
            .await?
            .value;
        rpc.accepted(switched.success)?;
        Ok(switched.source)
    }

    pub async fn rename_branch(
        &self,
        cwd: &str,
        branch: &str,
    ) -> Result<Option<String>, PaseoError> {
        let rpc = Rpc::new(
            "checkout.rename_branch.request",
            "checkout.rename_branch.response",
        );
        let renamed = self
            .call::<_, CheckoutResult<BranchRenamed>>(rpc, &BranchTarget { cwd, branch })
            .await?
            .value;
        rpc.accepted(renamed.success)?;
        Ok(renamed.current_branch)
    }

    pub async fn validate_branch(
        &self,
        cwd: &str,
        branch_name: &str,
    ) -> Result<BranchValidation, PaseoError> {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Body<'a> {
            cwd: &'a str,
            branch_name: &'a str,
        }
        let rpc = Rpc::new("validate_branch_request", "validate_branch_response");
        self.call_checked(rpc, &Body { cwd, branch_name }).await
    }

    pub async fn stash_save(&self, cwd: &str, branch: Option<&str>) -> Result<(), PaseoError> {
        #[derive(Serialize)]
        struct Body<'a> {
            cwd: &'a str,
            #[serde(skip_serializing_if = "Option::is_none")]
            branch: Option<&'a str>,
        }
        let rpc = Rpc::new("stash_save_request", "stash_save_response");
        self.checkout_mutation(rpc, &Body { cwd, branch }).await
    }

    pub async fn stash_pop(&self, cwd: &str, index: u32) -> Result<(), PaseoError> {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Body<'a> {
            cwd: &'a str,
            stash_index: u32,
        }
        let rpc = Rpc::new("stash_pop_request", "stash_pop_response");
        self.checkout_mutation(
            rpc,
            &Body {
                cwd,
                stash_index: index,
            },
        )
        .await
    }

    pub async fn list_stashes(
        &self,
        cwd: &str,
        paseo_only: bool,
    ) -> Result<Vec<StashEntry>, PaseoError> {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Body<'a> {
            cwd: &'a str,
            paseo_only: bool,
        }
        let rpc = Rpc::new("stash_list_request", "stash_list_response");
        Ok(self
            .call::<_, CheckoutResult<StashesListed>>(rpc, &Body { cwd, paseo_only })
            .await?
            .value
            .entries)
    }

    pub async fn search_forge(
        &self,
        cwd: &str,
        query: &str,
        kinds: &[ForgeItemKind],
        limit: Option<u32>,
    ) -> Result<ForgeSearchResults, PaseoError> {
        #[derive(Serialize)]
        struct Body<'a, K> {
            cwd: &'a str,
            query: &'a str,
            #[serde(skip_serializing_if = "Option::is_none")]
            limit: Option<u32>,
            #[serde(skip_serializing_if = "<[_]>::is_empty")]
            kinds: &'a [K],
        }
        #[derive(Clone, Copy, Serialize)]
        enum GithubKind {
            #[serde(rename = "github-issue")]
            Issue,
            #[serde(rename = "github-pr")]
            PullRequest,
        }
        if let Some(limit) = limit {
            check_limit("forge search limit", limit, FORGE_SEARCH_MAX_LIMIT)?;
        }
        let (searched, unreported) = if self.server_info().supports(DaemonFeature::ForgeSearch) {
            let rpc = Rpc::new("forge.search.request", "forge.search.response");
            let body = Body {
                cwd,
                query,
                limit,
                kinds,
            };
            let searched = self.call::<_, ForgeSearched>(rpc, &body).await?;
            (searched, ForgeAuthState::Unauthenticated)
        } else {
            let rpc = Rpc::new("github_search_request", "github_search_response");
            let kinds = kinds
                .iter()
                .map(|kind| match kind {
                    ForgeItemKind::ChangeRequest => GithubKind::PullRequest,
                    ForgeItemKind::Issue => GithubKind::Issue,
                })
                .collect::<Vec<_>>();
            let body = Body {
                cwd,
                query,
                limit,
                kinds: &kinds,
            };
            let searched = self.call::<_, ForgeSearched>(rpc, &body).await?;
            let enabled = searched
                .features_enabled
                .or(searched.github_features_enabled)
                .unwrap_or(true);
            (searched, ForgeAuthState::unreported(enabled))
        };
        let auth_state = searched.auth_state.unwrap_or(unreported);
        Ok(ForgeSearchResults::new(searched.items, auth_state))
    }

    pub async fn daemon_config(&self) -> Result<DaemonConfig, PaseoError> {
        let rpc = Rpc::new("get_daemon_config_request", "get_daemon_config_response");
        let fetched = self.call::<_, ConfigFetched>(rpc, &NoFields {}).await?;
        rpc.required(fetched.config, "config")
    }

    pub async fn set_daemon_config(
        &self,
        patch: &DaemonConfigPatch,
    ) -> Result<DaemonConfig, PaseoError> {
        #[derive(Serialize)]
        struct Body<'a> {
            config: &'a DaemonConfigPatch,
        }
        if !patch.removed_providers().is_empty() {
            self.require(DaemonFeature::ProviderRemoval)?;
        }
        if patch.agent_profiles().is_some() {
            self.require(DaemonFeature::AgentProfiles)?;
        }
        let rpc = Rpc::new("set_daemon_config_request", "set_daemon_config_response");
        let set = self
            .call::<_, ConfigFetched>(rpc, &Body { config: patch })
            .await?;
        rpc.required(set.config, "config")
    }

    pub async fn reload_daemon_config(&self) -> Result<ConfigReload, PaseoError> {
        self.require(DaemonFeature::DaemonConfigReload)?;
        let rpc = Rpc::new(
            "daemon.config.reload.request",
            "daemon.config.reload.response",
        );
        self.call_checked(rpc, &NoFields {}).await
    }

    pub async fn pairing_offer(&self) -> Result<PairingOffer, PaseoError> {
        let rpc = Rpc::new(
            "daemon.get_pairing_offer.request",
            "daemon.get_pairing_offer.response",
        );
        self.call_checked(rpc, &NoFields {}).await
    }

    pub async fn daemon_diagnostics(&self) -> Result<String, PaseoError> {
        self.require(DaemonFeature::DaemonDiagnostics)?;
        let rpc = Rpc::new("diagnostics.request", "diagnostics.response");
        let diagnosed = self.call::<_, Diagnosed>(rpc, &NoFields {}).await?;
        rpc.required(diagnosed.diagnostic, "diagnostic")
    }

    pub fn update_daemon(&self) -> Result<DaemonUpdate, PaseoError> {
        self.require(DaemonFeature::DaemonSelfUpdate)?;
        let (phases_tx, phases) = flume::unbounded();
        let pending = self.inner.submit(
            &Request {
                kind: DAEMON_UPDATE.kind,
                response_type: DAEMON_UPDATE.response,
                body: &NoFields {},
            },
            None,
            Some(Tracking::Progress {
                kind: DAEMON_UPDATE_PROGRESS,
                deliver: Inner::deliver(phases_tx),
            }),
        )?;
        Ok(DaemonUpdate { phases, pending })
    }

    pub fn register_push_token(&self, token: &str) -> Result<(), PaseoError> {
        #[derive(Serialize)]
        struct Body<'a> {
            token: &'a str,
        }
        let text = session_message("register_push_token", None, &Body { token })?;
        self.inner.send(Frame::Text(text))
    }

    pub async fn unregister_push_token(&self, token: &str) -> Result<(), PaseoError> {
        #[derive(Serialize)]
        struct Body<'a> {
            token: &'a str,
        }
        self.require(DaemonFeature::PushTokenRevocation)?;
        let rpc = Rpc::new("push.unregister.request", "push.unregister.response");
        self.call::<_, Acknowledged>(rpc, &Body { token }).await?;
        Ok(())
    }

    pub async fn shutdown_server(&self) -> Result<(), PaseoError> {
        let rpc = Rpc::new("shutdown_server_request", "status");
        let response = self.call::<_, ServerStatusReply>(rpc, &NoFields {}).await?;
        response.confirm(rpc, ServerStatus::ShutdownRequested)
    }

    pub async fn start_provider_setup(
        &self,
        provider: &str,
        action: ProviderSetupAction,
        workspace_id: &str,
    ) -> Result<ProviderSetup, PaseoError> {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Body<'a> {
            provider: &'a str,
            action: ProviderSetupAction,
            workspace_id: &'a str,
        }
        self.require(DaemonFeature::ProviderSetup)?;
        let rpc = Rpc::new("provider.setup.request", "provider.setup.response");
        self.call_within(
            rpc,
            &Body {
                provider,
                action,
                workspace_id,
            },
            Some(PROVIDER_CATALOGUE_TIMEOUT),
        )
        .await
    }

    pub async fn provider_installations(
        &self,
        provider: &str,
    ) -> Result<ProviderInstallations, PaseoError> {
        #[derive(Serialize)]
        struct Body<'a> {
            provider: &'a str,
        }
        self.require(DaemonFeature::ProviderInstallations)?;
        let rpc = Rpc::new(
            "provider.installations.request",
            "provider.installations.response",
        );
        self.call_within(rpc, &Body { provider }, Some(PROVIDER_CATALOGUE_TIMEOUT))
            .await
    }

    pub async fn provider_diagnostic(&self, provider: &str) -> Result<String, PaseoError> {
        #[derive(Serialize)]
        struct Body<'a> {
            provider: &'a str,
        }
        let rpc = Rpc::new(
            "provider_diagnostic_request",
            "provider_diagnostic_response",
        );
        let diagnosed = self
            .call_within::<_, Diagnosed>(rpc, &Body { provider }, Some(PROVIDER_CATALOGUE_TIMEOUT))
            .await?;
        rpc.required(diagnosed.diagnostic, "diagnostic")
    }

    pub fn usage_reports(&self, query: &UsageReportsQuery) -> Result<UsageReportFetch, PaseoError> {
        self.require(DaemonFeature::UsageSources)?;
        let (reports_tx, reports) = flume::unbounded();
        let pending = self.inner.submit(
            &Request {
                kind: USAGE_REPORTS.kind,
                response_type: USAGE_REPORTS.response,
                body: query,
            },
            None,
            Some(Tracking::Progress {
                kind: USAGE_REPORT_UPDATE,
                deliver: Inner::deliver(reports_tx),
            }),
        )?;
        Ok(UsageReportFetch { reports, pending })
    }

    pub async fn recent_provider_sessions(
        &self,
        query: &RecentSessionsQuery,
    ) -> Result<RecentProviderSessions, PaseoError> {
        if query.query().is_some() {
            self.require(DaemonFeature::ImportSessionSearch)?;
        }
        let rpc = Rpc::new(
            "fetch_recent_provider_sessions_request",
            "fetch_recent_provider_sessions_response",
        );
        self.call_within(rpc, query, Some(PROVIDER_CATALOGUE_TIMEOUT))
            .await
    }

    pub async fn search_timeline(
        &self,
        agent_id: &str,
        query: &str,
        cursor: Option<u64>,
    ) -> Result<TimelineSearch, PaseoError> {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Body<'a> {
            agent_id: &'a str,
            query: &'a str,
            #[serde(skip_serializing_if = "Option::is_none")]
            cursor: Option<u64>,
        }
        self.require(DaemonFeature::AgentHistorySearch)?;
        let rpc = Rpc::new(
            "agent.timeline.search.request",
            "agent.timeline.search.response",
        );
        let body = Body {
            agent_id,
            query,
            cursor,
        };
        self.call_checked(rpc, &body).await
    }

    pub async fn skills_status(&self) -> Result<SkillsStatus, PaseoError> {
        self.require(DaemonFeature::SkillManagement)?;
        let rpc = Rpc::new(
            "agent.skills.get_status.request",
            "agent.skills.get_status.response",
        );
        self.call_checked(rpc, &NoFields {}).await
    }

    pub async fn reconcile_skills(&self) -> Result<SkillsStatus, PaseoError> {
        self.require(DaemonFeature::SkillManagement)?;
        let rpc = Rpc::new(
            "agent.skills.reconcile.request",
            "agent.skills.reconcile.response",
        );
        self.call_checked(rpc, &NoFields {}).await
    }

    pub async fn uninstall_skills(&self) -> Result<SkillsStatus, PaseoError> {
        self.require(DaemonFeature::SkillManagement)?;
        let rpc = Rpc::new(
            "agent.skills.uninstall.request",
            "agent.skills.uninstall.response",
        );
        self.call_checked(rpc, &NoFields {}).await
    }

    pub async fn save_skill_selection(
        &self,
        selection: &SkillSelection,
        confirmed_removals: &[String],
    ) -> Result<SavedSkills, PaseoError> {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Body<'a> {
            selection: &'a SkillSelection,
            #[serde(skip_serializing_if = "<[_]>::is_empty")]
            confirmed_removals: &'a [String],
        }
        self.require(DaemonFeature::SkillManagement)?;
        let rpc = Rpc::new(
            "agent.skills.save_selection.request",
            "agent.skills.save_selection.response",
        );
        let body = Body {
            selection,
            confirmed_removals,
        };
        self.call_checked(rpc, &body).await
    }

    pub async fn import_legacy_skill_selection(
        &self,
        selection: &SkillSelection,
    ) -> Result<ImportedSkills, PaseoError> {
        #[derive(Serialize)]
        struct Body<'a> {
            selection: &'a SkillSelection,
        }
        self.require(DaemonFeature::SkillManagement)?;
        let rpc = Rpc::new(
            "agent.skills.import_legacy_selection.request",
            "agent.skills.import_legacy_selection.response",
        );
        self.call_checked(rpc, &Body { selection }).await
    }

    pub async fn inspect_schedule(&self, schedule_id: &str) -> Result<ScheduleDetail, PaseoError> {
        let rpc = Rpc::new("schedule/inspect", "schedule/inspect/response");
        let inspected = self
            .call::<_, ScheduleInspected>(rpc, &ScheduleIdTarget { schedule_id })
            .await?;
        rpc.required(inspected.schedule, "schedule")
    }

    pub async fn update_schedule(
        &self,
        schedule_id: &str,
        update: &ScheduleUpdate,
    ) -> Result<ScheduleDetail, PaseoError> {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Body<'a> {
            schedule_id: &'a str,
            #[serde(flatten)]
            update: &'a ScheduleUpdate,
        }
        let rpc = Rpc::new("schedule/update", "schedule/update/response");
        let updated = self
            .call::<_, ScheduleInspected>(
                rpc,
                &Body {
                    schedule_id,
                    update,
                },
            )
            .await?;
        rpc.required(updated.schedule, "schedule")
    }

    pub async fn list_hosts(&self) -> Result<HostDirectory, PaseoError> {
        self.require(DaemonFeature::ConnectedHosts)?;
        Ok(self
            .call::<_, HostsListed>(HOSTS_LIST, &NoFields {})
            .await?
            .directory)
    }

    pub async fn observe_hosts(
        &self,
    ) -> Result<Subscription<HostDirectory, HostDirectory>, PaseoError> {
        #[derive(Serialize)]
        struct Body {
            observe: bool,
        }
        self.require(DaemonFeature::ConnectedHosts)?;
        self.observe(
            HOSTS_LIST,
            &Body { observe: true },
            |listed: HostsListed| listed.directory,
        )
        .await
    }

    pub async fn add_host(&self, host: &HostAdd) -> Result<HostSummary, PaseoError> {
        #[derive(Serialize)]
        struct Body<'a> {
            input: &'a HostAdd,
        }
        self.require(DaemonFeature::ConnectedHosts)?;
        let rpc = Rpc::new("hosts.add.request", "hosts.add.response");
        let saved = self
            .call::<_, HostSaved>(rpc, &Body { input: host })
            .await?;
        rpc.required(saved.host, "host")
    }

    pub async fn update_host(
        &self,
        registration_id: &str,
        update: &HostUpdate,
    ) -> Result<HostSummary, PaseoError> {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Body<'a> {
            registration_id: &'a str,
            patch: &'a HostUpdate,
        }
        self.require(DaemonFeature::ConnectedHosts)?;
        let rpc = Rpc::new("hosts.update.request", "hosts.update.response");
        let body = Body {
            registration_id,
            patch: update,
        };
        let saved = self.call::<_, HostSaved>(rpc, &body).await?;
        rpc.required(saved.host, "host")
    }

    pub async fn remove_host(&self, registration_id: &str) -> Result<(), PaseoError> {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Body<'a> {
            registration_id: &'a str,
        }
        self.require(DaemonFeature::ConnectedHosts)?;
        let rpc = Rpc::new("hosts.remove.request", "hosts.remove.response");
        self.call::<_, Acknowledged>(rpc, &Body { registration_id })
            .await?;
        Ok(())
    }

    pub async fn connect_host(
        &self,
        server_id: &str,
        options: &ConnectOptions,
    ) -> Result<PaseoClient, PaseoError> {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Body<'a> {
            server_id: &'a str,
            scope_id: &'a str,
        }
        self.require(DaemonFeature::ConnectedHosts)?;
        let rpc = Rpc::new("hosts.route.open.request", "hosts.route.open.response");
        let (events, receiver) = flume::unbounded();
        let (route_tx, route) = oneshot::channel();
        let request = Request {
            kind: rpc.kind,
            response_type: rpc.response,
            body: &Body {
                server_id,
                scope_id: options.client_id(),
            },
        };
        let connect = async {
            let registration = Inner::host_route_registration(events, rpc.response, route_tx);
            self.inner
                .submit(&request, Some(registration), None)?
                .wait(None)
                .await?;
            let route_id = route
                .await
                .expect("an opened route reports its id before the reply");
            let socket = Socket::Routed(RoutedSocket::new(route_id, &self.inner, receiver));
            PaseoClient::handshake(Connection::plain(socket), None, options).await
        };
        timed(connect, CONNECT_TIMEOUT)
            .await
            .ok_or(PaseoError::Timeout {
                request_type: rpc.kind,
            })?
    }
}

#[derive(Serialize)]
struct ProviderQuery<'a> {
    provider: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    cwd: Option<&'a str>,
}
