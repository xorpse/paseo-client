use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use std::num::NonZeroU64;

use crate::protocol::agents::AgentSnapshot;
use crate::protocol::number::WireNumber;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum WorktreeSetupCommandStatus {
    Completed,
    Failed,
    Running,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WorktreeSetupCommand {
    index: NonZeroU64,
    command: String,
    cwd: String,
    #[serde(default)]
    log: String,
    status: WorktreeSetupCommandStatus,
    exit_code: Option<WireNumber>,
    #[serde(default)]
    duration_ms: Option<f64>,
}

impl WorktreeSetupCommand {
    pub fn index(&self) -> NonZeroU64 {
        self.index
    }

    pub fn command(&self) -> &str {
        &self.command
    }

    pub fn cwd(&self) -> &str {
        &self.cwd
    }

    pub fn log(&self) -> &str {
        &self.log
    }

    pub fn status(&self) -> WorktreeSetupCommandStatus {
        self.status
    }

    pub fn exit_code(&self) -> Option<i32> {
        self.exit_code.and_then(WireNumber::whole_i32)
    }

    pub fn duration_ms(&self) -> Option<f64> {
        self.duration_ms
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WorktreeSetupDetail {
    worktree_path: String,
    branch_name: String,
    log: String,
    commands: Vec<WorktreeSetupCommand>,
    #[serde(default)]
    truncated: bool,
}

impl WorktreeSetupDetail {
    pub fn worktree_path(&self) -> &str {
        &self.worktree_path
    }

    pub fn branch_name(&self) -> &str {
        &self.branch_name
    }

    pub fn log(&self) -> &str {
        &self.log
    }

    pub fn commands(&self) -> &[WorktreeSetupCommand] {
        &self.commands
    }

    pub fn truncated(&self) -> bool {
        self.truncated
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum SearchTool {
    Glob,
    Grep,
    Search,
    WebSearch,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum SearchMode {
    Content,
    Count,
    FilesWithMatches,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct WebResult {
    title: String,
    url: String,
}

impl WebResult {
    pub fn title(&self) -> &str {
        &self.title
    }

    pub fn url(&self) -> &str {
        &self.url
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ToolCallDetail {
    Edit(EditDetail),
    Fetch(FetchDetail),
    Plan(PlanDetail),
    PlainText(PlainTextDetail),
    Read(ReadDetail),
    Search(SearchDetail),
    Shell(ShellDetail),
    SubAgent(SubAgentDetail),
    Unknown(UnknownDetail),
    WorktreeSetup(WorktreeSetupDetail),
    Write(WriteDetail),
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct EditDetail {
    file_path: String,
    #[serde(default)]
    old_string: Option<String>,
    #[serde(default)]
    new_string: Option<String>,
    #[serde(default)]
    unified_diff: Option<String>,
}

impl EditDetail {
    pub fn file_path(&self) -> &str {
        &self.file_path
    }

    pub fn old_string(&self) -> Option<&str> {
        self.old_string.as_deref()
    }

    pub fn new_string(&self) -> Option<&str> {
        self.new_string.as_deref()
    }

    pub fn unified_diff(&self) -> Option<&str> {
        self.unified_diff.as_deref()
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FetchDetail {
    url: String,
    #[serde(default)]
    prompt: Option<String>,
    #[serde(default)]
    result: Option<String>,
    #[serde(default)]
    code: Option<WireNumber>,
    #[serde(default)]
    code_text: Option<String>,
    #[serde(default)]
    bytes: Option<WireNumber>,
    #[serde(default)]
    duration_ms: Option<f64>,
}

impl FetchDetail {
    pub fn url(&self) -> &str {
        &self.url
    }

    pub fn prompt(&self) -> Option<&str> {
        self.prompt.as_deref()
    }

    pub fn result(&self) -> Option<&str> {
        self.result.as_deref()
    }

    pub fn code(&self) -> Option<u32> {
        self.code.and_then(WireNumber::whole_u32)
    }

    pub fn code_text(&self) -> Option<&str> {
        self.code_text.as_deref()
    }

    pub fn bytes(&self) -> Option<u64> {
        self.bytes.map(WireNumber::count)
    }

    pub fn duration_ms(&self) -> Option<f64> {
        self.duration_ms
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct PlanDetail {
    text: String,
}

impl PlanDetail {
    pub fn text(&self) -> &str {
        &self.text
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct PlainTextDetail {
    #[serde(default)]
    label: Option<String>,
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    icon: Option<String>,
}

impl PlainTextDetail {
    pub fn label(&self) -> Option<&str> {
        self.label.as_deref()
    }

    pub fn text(&self) -> Option<&str> {
        self.text.as_deref()
    }

    pub fn icon(&self) -> Option<&str> {
        self.icon.as_deref()
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ReadDetail {
    file_path: String,
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    offset: Option<WireNumber>,
    #[serde(default)]
    limit: Option<WireNumber>,
}

impl ReadDetail {
    pub fn file_path(&self) -> &str {
        &self.file_path
    }

    pub fn content(&self) -> Option<&str> {
        self.content.as_deref()
    }

    pub fn offset(&self) -> Option<u32> {
        self.offset.and_then(WireNumber::whole_u32)
    }

    pub fn limit(&self) -> Option<u32> {
        self.limit.and_then(WireNumber::whole_u32)
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SearchDetail {
    query: String,
    #[serde(default)]
    tool_name: Option<SearchTool>,
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    file_paths: Vec<String>,
    #[serde(default)]
    web_results: Vec<WebResult>,
    #[serde(default)]
    annotations: Vec<String>,
    #[serde(default)]
    num_files: Option<WireNumber>,
    #[serde(default)]
    num_matches: Option<WireNumber>,
    #[serde(default)]
    duration_ms: Option<f64>,
    #[serde(default)]
    duration_seconds: Option<f64>,
    #[serde(default)]
    truncated: bool,
    #[serde(default)]
    mode: Option<SearchMode>,
}

impl SearchDetail {
    pub fn query(&self) -> &str {
        &self.query
    }

    pub fn tool_name(&self) -> Option<SearchTool> {
        self.tool_name
    }

    pub fn content(&self) -> Option<&str> {
        self.content.as_deref()
    }

    pub fn file_paths(&self) -> &[String] {
        &self.file_paths
    }

    pub fn web_results(&self) -> &[WebResult] {
        &self.web_results
    }

    pub fn annotations(&self) -> &[String] {
        &self.annotations
    }

    pub fn num_files(&self) -> Option<u64> {
        self.num_files.map(WireNumber::count)
    }

    pub fn num_matches(&self) -> Option<u64> {
        self.num_matches.map(WireNumber::count)
    }

    pub fn duration_ms(&self) -> Option<f64> {
        self.duration_ms
    }

    pub fn duration_seconds(&self) -> Option<f64> {
        self.duration_seconds
    }

    pub fn truncated(&self) -> bool {
        self.truncated
    }

    pub fn mode(&self) -> Option<SearchMode> {
        self.mode
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ShellDetail {
    command: String,
    #[serde(default)]
    cwd: Option<String>,
    #[serde(default)]
    output: Option<String>,
    #[serde(default)]
    exit_code: Option<WireNumber>,
}

impl ShellDetail {
    pub fn command(&self) -> &str {
        &self.command
    }

    pub fn cwd(&self) -> Option<&str> {
        self.cwd.as_deref()
    }

    pub fn output(&self) -> Option<&str> {
        self.output.as_deref()
    }

    pub fn exit_code(&self) -> Option<i32> {
        self.exit_code.and_then(WireNumber::whole_i32)
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SubAgentDetail {
    #[serde(default)]
    sub_agent_type: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    child_session_id: Option<String>,
    log: String,
}

impl SubAgentDetail {
    pub fn sub_agent_type(&self) -> Option<&str> {
        self.sub_agent_type.as_deref()
    }

    pub fn description(&self) -> Option<&str> {
        self.description.as_deref()
    }

    pub fn child_session_id(&self) -> Option<&str> {
        self.child_session_id.as_deref()
    }

    pub fn log(&self) -> &str {
        &self.log
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct UnknownDetail {
    input: Value,
    output: Value,
}

impl UnknownDetail {
    pub fn input(&self) -> &Value {
        &self.input
    }

    pub fn output(&self) -> &Value {
        &self.output
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WriteDetail {
    file_path: String,
    #[serde(default)]
    content: Option<String>,
}

impl WriteDetail {
    pub fn file_path(&self) -> &str {
        &self.file_path
    }

    pub fn content(&self) -> Option<&str> {
        self.content.as_deref()
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum AgentMessageEvent {
    Closed,
    Errored,
    Finished,
    Message,
    PermissionRequired,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct AgentMessageSender {
    id: String,
    #[serde(default)]
    title: Option<String>,
}

impl AgentMessageSender {
    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn title(&self) -> Option<&str> {
        self.title.as_deref()
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct AgentMessageProvenance {
    event: AgentMessageEvent,
    sender: AgentMessageSender,
    text: String,
}

impl AgentMessageProvenance {
    pub fn event(&self) -> AgentMessageEvent {
        self.event
    }

    pub fn sender(&self) -> &AgentMessageSender {
        &self.sender
    }

    pub fn text(&self) -> &str {
        &self.text
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum ToolCallStatus {
    Canceled,
    Completed,
    Failed(Value),
    Running,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(from = "ToolCallWire")]
pub struct ToolCall {
    call_id: String,
    name: String,
    detail: ToolCallDetail,
    status: ToolCallStatus,
    metadata: Option<Map<String, Value>>,
    agent_message: Option<AgentMessageProvenance>,
}

impl ToolCall {
    pub fn call_id(&self) -> &str {
        &self.call_id
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn detail(&self) -> &ToolCallDetail {
        &self.detail
    }

    pub fn status(&self) -> &ToolCallStatus {
        &self.status
    }

    pub fn metadata(&self) -> Option<&Map<String, Value>> {
        self.metadata.as_ref()
    }

    pub fn agent_message(&self) -> Option<&AgentMessageProvenance> {
        self.agent_message.as_ref()
    }
}

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
enum ToolCallStatusWire {
    Canceled,
    Completed,
    Failed,
    Running,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ToolCallWire {
    call_id: String,
    name: String,
    detail: ToolCallDetail,
    status: ToolCallStatusWire,
    #[serde(default)]
    error: Value,
    #[serde(default)]
    metadata: Option<Map<String, Value>>,
    #[serde(default)]
    agent_message: Option<AgentMessageProvenance>,
}

impl From<ToolCallWire> for ToolCall {
    fn from(wire: ToolCallWire) -> ToolCall {
        let status = match wire.status {
            ToolCallStatusWire::Canceled => ToolCallStatus::Canceled,
            ToolCallStatusWire::Completed => ToolCallStatus::Completed,
            ToolCallStatusWire::Failed => ToolCallStatus::Failed(wire.error),
            ToolCallStatusWire::Running => ToolCallStatus::Running,
        };
        ToolCall {
            call_id: wire.call_id,
            name: wire.name,
            detail: wire.detail,
            status,
            metadata: wire.metadata,
            agent_message: wire.agent_message,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum TodoStatus {
    Completed,
    InProgress,
    Pending,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TodoEntry {
    text: String,
    completed: bool,
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    status: Option<TodoStatus>,
    #[serde(default)]
    active_form: Option<String>,
}

impl TodoEntry {
    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn completed(&self) -> bool {
        self.completed
    }

    pub fn id(&self) -> Option<&str> {
        self.id.as_deref()
    }

    pub fn status(&self) -> Option<TodoStatus> {
        self.status
    }

    pub fn active_form(&self) -> Option<&str> {
        self.active_form.as_deref()
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum NotificationLevel {
    Error,
    Info,
    Warning,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum CompactionStatus {
    Completed,
    Loading,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum CompactionTrigger {
    Auto,
    Manual,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TimelineItem {
    AssistantMessage(AssistantMessageItem),
    Compaction(CompactionItem),
    Error(ErrorItem),
    Notification(NotificationItem),
    Reasoning(ReasoningItem),
    Todo(TodoItem),
    ToolCall(Box<ToolCall>),
    UserMessage(UserMessageItem),
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AssistantMessageItem {
    text: String,
    #[serde(default)]
    message_id: Option<String>,
}

impl AssistantMessageItem {
    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn message_id(&self) -> Option<&str> {
        self.message_id.as_deref()
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CompactionItem {
    status: CompactionStatus,
    #[serde(default)]
    trigger: Option<CompactionTrigger>,
    #[serde(default)]
    pre_tokens: Option<WireNumber>,
}

impl CompactionItem {
    pub fn status(&self) -> CompactionStatus {
        self.status
    }

    pub fn trigger(&self) -> Option<CompactionTrigger> {
        self.trigger
    }

    pub fn pre_tokens(&self) -> Option<u64> {
        self.pre_tokens.map(WireNumber::count)
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct ErrorItem {
    message: String,
}

impl ErrorItem {
    pub fn message(&self) -> &str {
        &self.message
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct NotificationItem {
    level: NotificationLevel,
    message: String,
}

impl NotificationItem {
    pub fn level(&self) -> NotificationLevel {
        self.level
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct ReasoningItem {
    text: String,
}

impl ReasoningItem {
    pub fn text(&self) -> &str {
        &self.text
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct TodoItem {
    items: Vec<TodoEntry>,
}

impl TodoItem {
    pub fn items(&self) -> &[TodoEntry] {
        &self.items
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct UserMessageItem {
    text: String,
    #[serde(default)]
    message_id: Option<String>,
    #[serde(default)]
    client_message_id: Option<String>,
}

impl UserMessageItem {
    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn message_id(&self) -> Option<&str> {
        self.message_id.as_deref()
    }

    pub fn client_message_id(&self) -> Option<&str> {
        self.client_message_id.as_deref()
    }
}

#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct TimelineCursor {
    epoch: String,
    seq: u64,
}

impl TimelineCursor {
    pub fn new(epoch: impl Into<String>, seq: u64) -> TimelineCursor {
        TimelineCursor {
            epoch: epoch.into(),
            seq,
        }
    }

    pub fn epoch(&self) -> &str {
        &self.epoch
    }

    pub fn seq(&self) -> u64 {
        self.seq
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SeqRange {
    start_seq: u64,
    end_seq: u64,
}

impl SeqRange {
    pub fn start_seq(&self) -> u64 {
        self.start_seq
    }

    pub fn end_seq(&self) -> u64 {
        self.end_seq
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum TimelineCollapse {
    AssistantMerge,
    Identity,
    ReasoningMerge,
    ToolLifecycle,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TimelineEntry {
    provider: String,
    item: TimelineItem,
    #[serde(default)]
    turn_id: Option<String>,
    timestamp: String,
    seq_start: u64,
    seq_end: u64,
    source_seq_ranges: Vec<SeqRange>,
    collapsed: Vec<TimelineCollapse>,
}

impl TimelineEntry {
    pub fn provider(&self) -> &str {
        &self.provider
    }

    pub fn item(&self) -> &TimelineItem {
        &self.item
    }

    pub fn turn_id(&self) -> Option<&str> {
        self.turn_id.as_deref()
    }

    pub fn timestamp(&self) -> &str {
        &self.timestamp
    }

    pub fn seq_start(&self) -> u64 {
        self.seq_start
    }

    pub fn seq_end(&self) -> u64 {
        self.seq_end
    }

    pub fn source_seq_ranges(&self) -> &[SeqRange] {
        &self.source_seq_ranges
    }

    pub fn collapsed(&self) -> &[TimelineCollapse] {
        &self.collapsed
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TimelineDirection {
    After,
    Before,
    Tail,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TimelineProjection {
    Canonical,
    Projected,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TimelineWindow {
    min_seq: u64,
    max_seq: u64,
    next_seq: u64,
}

impl TimelineWindow {
    pub fn min_seq(&self) -> u64 {
        self.min_seq
    }

    pub fn max_seq(&self) -> u64 {
        self.max_seq
    }

    pub fn next_seq(&self) -> u64 {
        self.next_seq
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AgentTimelinePage {
    agent_id: String,
    agent: Option<Box<AgentSnapshot>>,
    direction: TimelineDirection,
    projection: TimelineProjection,
    epoch: String,
    reset: bool,
    stale_cursor: bool,
    gap: bool,
    window: TimelineWindow,
    start_cursor: Option<TimelineCursor>,
    end_cursor: Option<TimelineCursor>,
    has_older: bool,
    has_newer: bool,
    entries: Vec<TimelineEntry>,
}

impl AgentTimelinePage {
    pub fn agent_id(&self) -> &str {
        &self.agent_id
    }

    pub fn agent(&self) -> Option<&AgentSnapshot> {
        self.agent.as_deref()
    }

    pub fn direction(&self) -> TimelineDirection {
        self.direction
    }

    pub fn projection(&self) -> TimelineProjection {
        self.projection
    }

    pub fn epoch(&self) -> &str {
        &self.epoch
    }

    pub fn reset(&self) -> bool {
        self.reset
    }

    pub fn stale_cursor(&self) -> bool {
        self.stale_cursor
    }

    pub fn gap(&self) -> bool {
        self.gap
    }

    pub fn window(&self) -> TimelineWindow {
        self.window
    }

    pub fn start_cursor(&self) -> Option<&TimelineCursor> {
        self.start_cursor.as_ref()
    }

    pub fn end_cursor(&self) -> Option<&TimelineCursor> {
        self.end_cursor.as_ref()
    }

    pub fn has_older(&self) -> bool {
        self.has_older
    }

    pub fn has_newer(&self) -> bool {
        self.has_newer
    }

    pub fn entries(&self) -> &[TimelineEntry] {
        &self.entries
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TimelineQuery {
    agent_id: String,
    direction: TimelineDirection,
    #[serde(skip_serializing_if = "Option::is_none")]
    cursor: Option<TimelineCursor>,
    #[serde(skip_serializing_if = "Option::is_none")]
    limit: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    projection: Option<TimelineProjection>,
}

impl TimelineQuery {
    pub fn tail(agent_id: impl Into<String>, limit: u32) -> TimelineQuery {
        TimelineQuery {
            agent_id: agent_id.into(),
            direction: TimelineDirection::Tail,
            cursor: None,
            limit: Some(limit),
            projection: None,
        }
    }

    pub fn before(
        agent_id: impl Into<String>,
        cursor: TimelineCursor,
        limit: u32,
    ) -> TimelineQuery {
        TimelineQuery {
            agent_id: agent_id.into(),
            direction: TimelineDirection::Before,
            cursor: Some(cursor),
            limit: Some(limit),
            projection: None,
        }
    }

    pub fn after(agent_id: impl Into<String>, cursor: TimelineCursor) -> TimelineQuery {
        TimelineQuery {
            agent_id: agent_id.into(),
            direction: TimelineDirection::After,
            cursor: Some(cursor),
            limit: None,
            projection: None,
        }
    }

    pub fn projection(&self) -> Option<TimelineProjection> {
        self.projection
    }

    pub fn set_projection(&mut self, projection: TimelineProjection) {
        self.projection = Some(projection);
    }

    pub fn with_projection(mut self, projection: TimelineProjection) -> TimelineQuery {
        self.set_projection(projection);
        self
    }
}
