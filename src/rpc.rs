use std::collections::HashMap;
use std::num::NonZeroU64;
use std::sync::Arc;
use std::time::Duration;

use serde::de::{DeserializeOwned, Error as _};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use serde_json::value::RawValue;

use crate::binary_frames::FileTransferFrame;
use crate::channel::{Channel, ChannelLease, NetChannel, ProcessChannel, Termination};
use crate::client::{
    Inner, PaseoClient, Payload, PendingReply, REQUEST_TIMEOUT, Request, Routed, Subscription,
    TerminalHandle, TerminalSlot,
};
use crate::envelope::{NoFields, PayloadHead, session_message};
use crate::error::{ArgumentError, PaseoError, ProtocolViolation, RequestFailure};
use crate::events::{EventKind, SessionEvent};
use crate::protocol::agents::{
    AgentActionOutcome, AgentAttachment, AgentConfigPatch, AgentCreateRequest, AgentDirectoryEntry,
    AgentDirectoryPage, AgentDirectoryQuery, AgentFeature, AgentMessage, AgentMode,
    AgentPersistenceHandle, AgentSessionConfig, AgentSnapshot, AgentUpdate, CreationSnapshot,
    DraftConfig, ForkedContext, ImageAttachment, ImportAgentRequest, ModelDefinition,
    PermissionResponse, PromptPreview, ProviderNotice, RewindMode, SlashCommand, WaitOutcome,
};
use crate::protocol::channels::{NetClosed, ProcessExit, ProcessSpawn};
use crate::protocol::daemon::{DaemonFeature, DaemonStatus, UploadedFile};
use crate::protocol::diff::{CheckoutDiff, CheckoutError, CheckoutStatus, DiffCompare};
use crate::protocol::providers::{ProviderAvailability, ProvidersSnapshot};
use crate::protocol::stream::TimelineUpdate;
use crate::protocol::subagents::{ProviderSubagent, SubagentTimelinePage, SubagentTimelineQuery};
use crate::protocol::terminals::{
    CreateTerminalOptions, TerminalInfo, TerminalRestore, TerminalStreamEvent, TerminalsChanged,
};
use crate::protocol::timeline::{AgentTimelinePage, TimelineCursor, TimelineQuery};
use crate::protocol::workspaces::{
    CreatedWorkspace, DirectorySuggestion, DirectorySuggestionsQuery, PaseoWorktree,
    ProjectDescriptor, ProjectPlacement, WorkspaceCreated, WorkspaceDescriptor,
    WorkspaceDirectoryPage, WorkspaceDirectoryQuery, WorkspaceSetupSnapshot, WorkspaceSource,
    WorkspaceUpdate, WorktreeAction,
};
use crate::transport::Frame;

const GITHUB_CLONE_TIMEOUT: Duration = Duration::from_secs(5 * 60);
const PROVIDER_CATALOGUE_TIMEOUT: Duration = Duration::from_secs(90);
const MAX_WAIT_FOR_FINISH_MS: u64 = 2_147_483_647;
const UPLOAD_CHUNK_BYTES: usize = 128 * 1024;
const UPLOAD_TIMEOUT: Duration = Duration::from_secs(5 * 60);

#[derive(Clone, Copy)]
struct Rpc {
    kind: &'static str,
    response: &'static str,
}

impl Rpc {
    const fn new(kind: &'static str, response: &'static str) -> Rpc {
        Rpc { kind, response }
    }
}

trait Response: DeserializeOwned {
    fn failure(&self) -> Option<&str> {
        None
    }
}

macro_rules! response {
    ($($name:ident { $($field:ident: $ty:ty),* $(,)? })*) => {
        $(
            #[derive(Deserialize)]
            #[serde(rename_all = "camelCase")]
            struct $name {
                $(#[serde(default)] $field: $ty,)*
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
    ActionResponse { agent_id: String, accepted: bool, notice: Option<ProviderNotice> }
    AgentCreated { agent: Option<AgentSnapshot> }
    AgentFetched { agent: Option<AgentSnapshot>, project: Option<ProjectPlacement> }
    AgentArchived { archived_at: String }
    BranchSuggestions { branches: Vec<String> }
    CancelResponse {}
    CommandsListed { commands: Vec<SlashCommand> }
    DirectorySuggestions { entries: Vec<DirectorySuggestion> }
    DirectoryCreated { directory_path: Option<String> }
    EntryCreated { path: Option<String>, success: bool }
    FeaturesListed { features: Vec<AgentFeature> }
    GithubCloned { checkout_path: Option<String>, project: Option<ProjectDescriptor> }
    ModelsListed { models: Vec<ModelDefinition> }
    ModesListed { modes: Vec<AgentMode> }
    ProjectAdded { project: Option<ProjectDescriptor> }
    ProjectOpened { workspace: Option<WorkspaceDescriptor> }
    PromptsListed { prompts: Vec<PromptPreview> }
    ProvidersListed { providers: Vec<ProviderAvailability> }
    RewindResponse { ok: bool }
    SendResponse { accepted: bool }
    SetupStatus { snapshot: Option<WorkspaceSetupSnapshot> }
    SubagentsListed { subagents: Vec<ProviderSubagent> }
    TerminalCreated { terminal: Option<TerminalInfo> }
    TerminalRenamed { success: bool }
    FileUploaded { file: Option<UploadedFile> }
    TitleSet { accepted: bool }
    WorkspaceArchived { archived_at: Option<String> }
}

impl Response for AgentDirectoryPage {}
impl Response for WorkspaceDirectoryPage {}
impl Response for ProvidersSnapshot {}
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

impl Response for Box<RawValue> {}
impl Response for CheckoutDiff {}
impl Response for CheckoutStatus {}
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
#[serde(rename_all = "camelCase")]
struct FileWritten {
    result: FileWriteResult,
}

impl Response for FileWritten {}

#[derive(Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
enum FileWriteResult {
    Conflict { version: FileVersion },
    Error { error: String },
    Written {},
}

#[derive(Deserialize)]
#[serde(
    tag = "status",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
enum FileVersion {
    Error {
        error: String,
    },
    Missing {},
    Ready {
        modified_at: String,
        #[serde(default)]
        revision: Option<String>,
    },
}

#[derive(Default, Serialize)]
#[serde(rename_all = "camelCase")]
struct ExpectedVersion {
    expected_modified_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    expected_revision: Option<String>,
}

#[derive(Deserialize)]
struct DirectoryListed {
    directory: Option<ListedDirectory>,
}

impl Response for DirectoryListed {}

#[derive(Deserialize)]
struct ListedDirectory {
    entries: Vec<ListedEntry>,
}

#[derive(Deserialize)]
struct ListedEntry {
    name: String,
    kind: EntryKind,
}

#[derive(Clone, Copy, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
enum EntryKind {
    Directory,
    File,
}

fn decode_payload<T: DeserializeOwned>(
    expected: &str,
    kind: &str,
    message: &RawValue,
) -> Result<T, PaseoError> {
    if kind != expected {
        return Err(PaseoError::decode(
            kind,
            serde_json::Error::custom("message type does not belong to this subscription"),
        ));
    }
    serde_json::from_str::<Payload<T>>(message.get())
        .map(|message| message.payload)
        .map_err(|error| PaseoError::decode(kind, error))
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
        created.agent.ok_or_else(|| {
            PaseoError::request(AGENT_CREATE.kind, RequestFailure::MissingResult("agent"))
        })
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

const AGENT_CREATE: Rpc = Rpc::new("agent.create.request", "agent.create.response");
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
        #[derive(Serialize)]
        struct Body<'a> {
            host: &'a str,
            port: u16,
        }
        self.require(DaemonFeature::StreamChannels)?;
        let channel = Arc::new(Channel::<NetClosed>::new(Arc::downgrade(&self.inner), true));
        let opened = self
            .open_channel(NET_CONNECT, &Body { host, port }, channel)
            .await?;
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
            .submit(&request, Some(registration))?
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
        let response = serde_json::from_str::<R>(payload.get())
            .map_err(|error| PaseoError::decode(rpc.response, error))?;
        if let Some(error) = response.failure() {
            return Err(PaseoError::rpc(rpc.kind, error));
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
        let payload = self.inner.submit(&request, None)?.wait(timeout).await?;
        Self::parse(rpc, &payload)
    }

    async fn call_checked<B: Serialize, T: DeserializeOwned>(
        &self,
        rpc: Rpc,
        body: &B,
    ) -> Result<T, PaseoError> {
        let payload = self.call::<_, Box<RawValue>>(rpc, body).await?;
        Self::parse::<Failure>(rpc, &payload)?;
        serde_json::from_str(payload.get()).map_err(|error| PaseoError::decode(rpc.response, error))
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
        let registration = Inner::route_registration(updates, rpc.response);
        let payload = self
            .inner
            .submit(&request, Some(registration))?
            .wait(Some(REQUEST_TIMEOUT))
            .await?;
        let id = Self::subscription_id(rpc, &payload)?;
        match Self::parse::<S>(rpc, &payload) {
            Ok(response) => Ok(Subscription::new(
                id,
                snapshot(response),
                receiver,
                &self.inner,
            )),
            Err(error) => {
                self.inner.release(&id);
                Err(error)
            }
        }
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
            .submit(&request, Some(Inner::terminal_registration(events)))?
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
        if !response.accepted {
            return Err(PaseoError::request(rpc.kind, RequestFailure::Declined));
        }
        Ok(())
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
        if !response.accepted {
            return Err(PaseoError::request(rpc.kind, RequestFailure::Declined));
        }
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
        if !response.ok {
            return Err(PaseoError::request(rpc.kind, RequestFailure::Declined));
        }
        Ok(())
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
        self.agent_status(rpc, &Body { handle, overrides }, "agent_resumed")
            .await?
            .ok_or_else(|| PaseoError::request(rpc.kind, RequestFailure::MissingResult("agent")))
    }

    pub async fn import_agent(
        &self,
        request: &ImportAgentRequest,
    ) -> Result<AgentSnapshot, PaseoError> {
        let rpc = Rpc::new("import_agent_request", "status");
        self.agent_status(rpc, request, "agent_resumed")
            .await?
            .ok_or_else(|| PaseoError::request(rpc.kind, RequestFailure::MissingResult("agent")))
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
        if !self
            .call::<_, RefreshAcknowledged>(rpc, &Body { cwd, providers })
            .await?
            .acknowledged
        {
            return Err(PaseoError::request(rpc.kind, RequestFailure::Declined));
        }
        Ok(())
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
        self.call::<_, ProjectOpened>(rpc, &CwdTarget { cwd })
            .await?
            .workspace
            .ok_or_else(|| {
                PaseoError::request(rpc.kind, RequestFailure::MissingResult("workspace"))
            })
    }

    pub async fn add_project(&self, cwd: &str) -> Result<ProjectDescriptor, PaseoError> {
        let rpc = Rpc::new("project.add.request", "project.add.response");
        self.call::<_, ProjectAdded>(rpc, &CwdTarget { cwd })
            .await?
            .project
            .ok_or_else(|| PaseoError::request(rpc.kind, RequestFailure::MissingResult("project")))
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
        self.call::<_, DirectoryCreated>(rpc, &Body { parent_path, name })
            .await?
            .directory_path
            .ok_or_else(|| {
                PaseoError::request(rpc.kind, RequestFailure::MissingResult("directory"))
            })
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
        response
            .checkout_path
            .or_else(|| {
                response
                    .project
                    .map(|project| project.project_root_path().to_owned())
            })
            .ok_or_else(|| PaseoError::request(rpc.kind, RequestFailure::MissingResult("project")))
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
        if !response.accepted {
            return Err(PaseoError::request(rpc.kind, RequestFailure::Declined));
        }
        Ok(())
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
            error: Option<CheckoutError>,
        }
        impl Response for Listed {
            fn failure(&self) -> Option<&str> {
                self.error.as_ref().map(CheckoutError::message)
            }
        }
        Ok(self
            .call::<_, Listed>(rpc, &CwdTarget { cwd })
            .await?
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
        let workspace = creation.workspace.ok_or_else(|| {
            PaseoError::request(rpc.kind, RequestFailure::MissingResult("workspace"))
        })?;
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
        const MAX_LIMIT: u32 = 200;
        if !(1..=MAX_LIMIT).contains(&limit) {
            return Err(ArgumentError::LimitOutOfRange {
                name: "branch suggestion limit",
                value: limit,
                max: MAX_LIMIT,
            }
            .into());
        }
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

    pub async fn write_text_file(
        &self,
        cwd: &str,
        parent_path: &str,
        name: &str,
        content: &str,
    ) -> Result<String, PaseoError> {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Write<'a> {
            cwd: &'a str,
            path: &'a str,
            content: &'a str,
            #[serde(flatten)]
            expected: &'a ExpectedVersion,
        }
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
        let write = Rpc::new("fs.file.write.request", "fs.file.write.response");
        let mut expected = ExpectedVersion::default();
        let mut created = false;
        let mut refreshed = false;
        loop {
            let body = Write {
                cwd,
                path: &path,
                content,
                expected: &expected,
            };
            match self.call::<_, FileWritten>(write, &body).await?.result {
                FileWriteResult::Written {} => return Ok(path),
                FileWriteResult::Error { error }
                | FileWriteResult::Conflict {
                    version: FileVersion::Error { error },
                } => return Err(PaseoError::rpc(write.kind, error)),
                FileWriteResult::Conflict {
                    version: FileVersion::Missing {},
                } if !created => {
                    self.create_file(cwd, &parents, name).await?;
                    created = true;
                }
                FileWriteResult::Conflict {
                    version:
                        FileVersion::Ready {
                            modified_at,
                            revision,
                        },
                } if !refreshed => {
                    expected = ExpectedVersion {
                        expected_modified_at: modified_at,
                        expected_revision: revision,
                    };
                    refreshed = true;
                }
                FileWriteResult::Conflict { .. } => {
                    return Err(PaseoError::request(write.kind, RequestFailure::FileChanged));
                }
            }
        }
    }

    async fn create_file(&self, cwd: &str, parents: &[&str], name: &str) -> Result<(), PaseoError> {
        let mut parent = String::from(".");
        for segment in parents {
            let existing = self
                .list_directory(cwd, &parent)
                .await?
                .into_iter()
                .find(|entry| entry.name == *segment);
            match existing.map(|entry| entry.kind) {
                Some(EntryKind::Directory) => {}
                Some(EntryKind::File) => {
                    return Err(PaseoError::request(
                        "fs.entry.create.request",
                        RequestFailure::NotADirectory {
                            path: format!("{parent}/{segment}"),
                        },
                    ));
                }
                None => {
                    self.create_entry(cwd, &parent, segment, EntryKind::Directory)
                        .await?;
                }
            }
            parent = match parent.as_str() {
                "." => (*segment).to_owned(),
                _ => format!("{parent}/{segment}"),
            };
        }
        self.create_entry(cwd, &parent, name, EntryKind::File).await
    }

    async fn list_directory(&self, cwd: &str, path: &str) -> Result<Vec<ListedEntry>, PaseoError> {
        #[derive(Serialize)]
        struct Body<'a> {
            cwd: &'a str,
            path: &'a str,
            mode: &'static str,
        }
        let rpc = Rpc::new("file_explorer_request", "file_explorer_response");
        let listed = self
            .call::<_, DirectoryListed>(
                rpc,
                &Body {
                    cwd,
                    path,
                    mode: "list",
                },
            )
            .await?;
        let directory = listed.directory.ok_or(PaseoError::request(
            rpc.kind,
            RequestFailure::MissingResult("directory"),
        ))?;
        Ok(directory.entries)
    }

    async fn create_entry(
        &self,
        cwd: &str,
        parent_path: &str,
        name: &str,
        kind: EntryKind,
    ) -> Result<(), PaseoError> {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Create<'a> {
            cwd: &'a str,
            parent_path: &'a str,
            name: &'a str,
            kind: EntryKind,
        }
        let rpc = Rpc::new("fs.entry.create.request", "fs.entry.create.response");
        let body = Create {
            cwd,
            parent_path,
            name,
            kind,
        };
        let created = self.call::<_, EntryCreated>(rpc, &body).await?;
        if !created.success || created.path.is_none() {
            return Err(PaseoError::request(
                rpc.kind,
                RequestFailure::MissingResult("path"),
            ));
        }
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
        self.call::<_, TerminalCreated>(rpc, &Body { cwd, options })
            .await?
            .terminal
            .ok_or_else(|| PaseoError::request(rpc.kind, RequestFailure::MissingResult("terminal")))
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
        if !response.success {
            return Err(PaseoError::request(rpc.kind, RequestFailure::Declined));
        }
        Ok(())
    }

    pub async fn kill_terminal(&self, terminal_id: &str) -> Result<(), PaseoError> {
        #[derive(Deserialize)]
        struct Killed {
            success: bool,
        }
        impl Response for Killed {}
        let rpc = Rpc::new("kill_terminal_request", "kill_terminal_response");
        if !self
            .call::<_, Killed>(rpc, &TerminalTarget { terminal_id })
            .await?
            .success
        {
            return Err(PaseoError::request(rpc.kind, RequestFailure::Declined));
        }
        Ok(())
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
        let pending = self.inner.submit(&request, None)?;
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
        let payload = pending.wait(Some(UPLOAD_TIMEOUT)).await?;
        Self::parse::<FileUploaded>(rpc, &payload)?
            .file
            .ok_or_else(|| PaseoError::request(rpc.kind, RequestFailure::MissingResult("file")))
    }

    /// Asks the daemon's supervisor to replace its worker. The supervisor keeps running, so the
    /// new worker loads the daemon's code from disk again.
    pub async fn restart_server(&self, reason: &str) -> Result<(), PaseoError> {
        #[derive(Serialize)]
        struct Body<'a> {
            reason: &'a str,
        }
        #[derive(Deserialize)]
        struct Status {
            status: String,
        }
        impl Response for Status {}
        let rpc = Rpc::new("restart_server_request", "status");
        let response = self.call::<_, Status>(rpc, &Body { reason }).await?;
        if response.status != "restart_requested" {
            return Err(PaseoError::request(
                rpc.kind,
                RequestFailure::UnexpectedStatus(response.status),
            ));
        }
        Ok(())
    }

    pub async fn daemon_status(&self) -> Result<DaemonStatus, PaseoError> {
        self.require(DaemonFeature::DaemonStatusRpc)?;
        self.call(
            Rpc::new("daemon.get_status.request", "daemon.get_status.response"),
            &NoFields {},
        )
        .await
    }
}

#[derive(Serialize)]
struct ProviderQuery<'a> {
    provider: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    cwd: Option<&'a str>,
}
