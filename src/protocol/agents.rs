use std::collections::HashMap;
use std::num::NonZeroU32;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::client::new_id;
use crate::error::ArgumentError;
use crate::protocol::diff::DiffMode;
use crate::protocol::number::WireNumber;
use crate::protocol::timeline::{TimelineCursor, ToolCallDetail};
use crate::protocol::workspaces::{ProjectPlacement, WorkspaceDescriptor};

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentStatus {
    Closed,
    Error,
    Idle,
    Initializing,
    Running,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AttentionReason {
    Error,
    Finished,
    Permission,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AgentMode {
    id: String,
    label: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    icon: Option<String>,
    #[serde(default)]
    color_tier: Option<String>,
}

impl AgentMode {
    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn label(&self) -> &str {
        &self.label
    }

    pub fn description(&self) -> Option<&str> {
        self.description.as_deref()
    }

    pub fn icon(&self) -> Option<&str> {
        self.icon.as_deref()
    }

    pub fn color_tier(&self) -> Option<&str> {
        self.color_tier.as_deref()
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SelectOption {
    id: String,
    label: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    is_default: bool,
}

impl SelectOption {
    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn label(&self) -> &str {
        &self.label
    }

    pub fn description(&self) -> Option<&str> {
        self.description.as_deref()
    }

    pub fn is_default(&self) -> bool {
        self.is_default
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AgentFeature {
    Select(SelectFeature),
    Toggle(ToggleFeature),
}

impl AgentFeature {
    pub fn id(&self) -> &str {
        match self {
            AgentFeature::Select(feature) => &feature.id,
            AgentFeature::Toggle(feature) => &feature.id,
        }
    }

    pub fn label(&self) -> &str {
        match self {
            AgentFeature::Select(feature) => &feature.label,
            AgentFeature::Toggle(feature) => &feature.label,
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ToggleFeature {
    id: String,
    label: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    tooltip: Option<String>,
    #[serde(default)]
    icon: Option<String>,
    value: bool,
}

impl ToggleFeature {
    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn label(&self) -> &str {
        &self.label
    }

    pub fn description(&self) -> Option<&str> {
        self.description.as_deref()
    }

    pub fn tooltip(&self) -> Option<&str> {
        self.tooltip.as_deref()
    }

    pub fn icon(&self) -> Option<&str> {
        self.icon.as_deref()
    }

    pub fn value(&self) -> bool {
        self.value
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SelectFeature {
    id: String,
    label: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    tooltip: Option<String>,
    #[serde(default)]
    icon: Option<String>,
    value: Option<String>,
    options: Vec<SelectOption>,
}

impl SelectFeature {
    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn label(&self) -> &str {
        &self.label
    }

    pub fn description(&self) -> Option<&str> {
        self.description.as_deref()
    }

    pub fn tooltip(&self) -> Option<&str> {
        self.tooltip.as_deref()
    }

    pub fn icon(&self) -> Option<&str> {
        self.icon.as_deref()
    }

    pub fn value(&self) -> Option<&str> {
        self.value.as_deref()
    }

    pub fn options(&self) -> &[SelectOption] {
        &self.options
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ModelDefinition {
    provider: String,
    id: String,
    label: String,
    #[serde(default)]
    aliases: Vec<String>,
    #[serde(default)]
    is_selectable: Option<bool>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    is_default: bool,
    #[serde(default)]
    context_window_max_tokens: Option<WireNumber>,
    #[serde(default)]
    thinking_options: Vec<SelectOption>,
    #[serde(default)]
    default_thinking_option_id: Option<String>,
}

impl ModelDefinition {
    pub fn provider(&self) -> &str {
        &self.provider
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn label(&self) -> &str {
        &self.label
    }

    pub fn aliases(&self) -> &[String] {
        &self.aliases
    }

    pub fn is_selectable(&self) -> Option<bool> {
        self.is_selectable
    }

    pub fn description(&self) -> Option<&str> {
        self.description.as_deref()
    }

    pub fn is_default(&self) -> bool {
        self.is_default
    }

    pub fn context_window_max_tokens(&self) -> Option<u64> {
        self.context_window_max_tokens.map(WireNumber::count)
    }

    pub fn thinking_options(&self) -> &[SelectOption] {
        &self.thinking_options
    }

    pub fn default_thinking_option_id(&self) -> Option<&str> {
        self.default_thinking_option_id.as_deref()
    }
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AgentCapabilities {
    supports_streaming: bool,
    supports_session_persistence: bool,
    #[serde(default)]
    supports_session_listing: bool,
    supports_dynamic_modes: bool,
    supports_mcp_servers: bool,
    supports_reasoning_stream: bool,
    supports_tool_invocations: bool,
    #[serde(default)]
    supports_rewind_conversation: bool,
    #[serde(default)]
    supports_rewind_files: bool,
    #[serde(default)]
    supports_rewind_both: bool,
}

impl AgentCapabilities {
    pub fn supports_streaming(&self) -> bool {
        self.supports_streaming
    }

    pub fn supports_session_persistence(&self) -> bool {
        self.supports_session_persistence
    }

    pub fn supports_session_listing(&self) -> bool {
        self.supports_session_listing
    }

    pub fn supports_dynamic_modes(&self) -> bool {
        self.supports_dynamic_modes
    }

    pub fn supports_mcp_servers(&self) -> bool {
        self.supports_mcp_servers
    }

    pub fn supports_reasoning_stream(&self) -> bool {
        self.supports_reasoning_stream
    }

    pub fn supports_tool_invocations(&self) -> bool {
        self.supports_tool_invocations
    }

    pub fn supports_rewind_conversation(&self) -> bool {
        self.supports_rewind_conversation
    }

    pub fn supports_rewind_files(&self) -> bool {
        self.supports_rewind_files
    }

    pub fn supports_rewind_both(&self) -> bool {
        self.supports_rewind_both
    }
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AgentUsage {
    #[serde(default)]
    input_tokens: Option<WireNumber>,
    #[serde(default)]
    cached_input_tokens: Option<WireNumber>,
    #[serde(default)]
    output_tokens: Option<WireNumber>,
    #[serde(default)]
    total_cost_usd: Option<f64>,
    #[serde(default)]
    context_window_max_tokens: Option<WireNumber>,
    #[serde(default)]
    context_window_used_tokens: Option<WireNumber>,
}

impl AgentUsage {
    pub fn input_tokens(&self) -> Option<u64> {
        self.input_tokens.map(WireNumber::count)
    }

    pub fn cached_input_tokens(&self) -> Option<u64> {
        self.cached_input_tokens.map(WireNumber::count)
    }

    pub fn output_tokens(&self) -> Option<u64> {
        self.output_tokens.map(WireNumber::count)
    }

    pub fn total_cost_usd(&self) -> Option<f64> {
        self.total_cost_usd
    }

    pub fn context_window_max_tokens(&self) -> Option<u64> {
        self.context_window_max_tokens.map(WireNumber::count)
    }

    pub fn context_window_used_tokens(&self) -> Option<u64> {
        self.context_window_used_tokens.map(WireNumber::count)
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum PermissionKind {
    Mode,
    Other,
    Plan,
    Question,
    Tool,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionBehavior {
    Allow,
    Deny,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum PermissionActionVariant {
    Danger,
    Primary,
    Secondary,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum PermissionActionIntent {
    Dismiss,
    Implement,
    ImplementResume,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PermissionAction {
    id: String,
    label: String,
    behavior: PermissionBehavior,
    #[serde(default)]
    variant: Option<PermissionActionVariant>,
    #[serde(default)]
    intent: Option<PermissionActionIntent>,
}

impl PermissionAction {
    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn label(&self) -> &str {
        &self.label
    }

    pub fn behavior(&self) -> PermissionBehavior {
        self.behavior
    }

    pub fn variant(&self) -> Option<PermissionActionVariant> {
        self.variant
    }

    pub fn intent(&self) -> Option<PermissionActionIntent> {
        self.intent
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PermissionRequest {
    id: String,
    provider: String,
    name: String,
    kind: PermissionKind,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    input: Option<Map<String, Value>>,
    #[serde(default)]
    detail: Option<ToolCallDetail>,
    #[serde(default)]
    suggestions: Vec<Map<String, Value>>,
    #[serde(default)]
    actions: Vec<PermissionAction>,
    #[serde(default)]
    metadata: Option<Map<String, Value>>,
}

impl PermissionRequest {
    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn provider(&self) -> &str {
        &self.provider
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn kind(&self) -> PermissionKind {
        self.kind
    }

    pub fn title(&self) -> Option<&str> {
        self.title.as_deref()
    }

    pub fn description(&self) -> Option<&str> {
        self.description.as_deref()
    }

    pub fn input(&self) -> Option<&Map<String, Value>> {
        self.input.as_ref()
    }

    pub fn detail(&self) -> Option<&ToolCallDetail> {
        self.detail.as_ref()
    }

    pub fn suggestions(&self) -> &[Map<String, Value>] {
        &self.suggestions
    }

    pub fn actions(&self) -> &[PermissionAction] {
        &self.actions
    }

    pub fn metadata(&self) -> Option<&Map<String, Value>> {
        self.metadata.as_ref()
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "behavior", rename_all = "snake_case")]
pub enum PermissionResponse {
    Allow(PermissionAllow),
    Deny(PermissionDeny),
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionAllow {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    selected_action_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    updated_input: Option<Map<String, Value>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    updated_permissions: Option<Vec<Map<String, Value>>>,
}

impl PermissionAllow {
    pub fn selected_action_id(&self) -> Option<&str> {
        self.selected_action_id.as_deref()
    }

    pub fn updated_input(&self) -> Option<&Map<String, Value>> {
        self.updated_input.as_ref()
    }

    pub fn updated_permissions(&self) -> Option<&[Map<String, Value>]> {
        self.updated_permissions.as_deref()
    }

    pub fn set_selected_action_id(&mut self, selected_action_id: impl Into<String>) {
        self.selected_action_id = Some(selected_action_id.into());
    }

    pub fn with_selected_action_id(
        mut self,
        selected_action_id: impl Into<String>,
    ) -> PermissionAllow {
        self.set_selected_action_id(selected_action_id);
        self
    }

    pub fn set_updated_input(&mut self, updated_input: Map<String, Value>) {
        self.updated_input = Some(updated_input);
    }

    pub fn with_updated_input(mut self, updated_input: Map<String, Value>) -> PermissionAllow {
        self.set_updated_input(updated_input);
        self
    }

    pub fn set_updated_permissions(&mut self, updated_permissions: Vec<Map<String, Value>>) {
        self.updated_permissions = Some(updated_permissions);
    }

    pub fn with_updated_permissions(
        mut self,
        updated_permissions: Vec<Map<String, Value>>,
    ) -> PermissionAllow {
        self.set_updated_permissions(updated_permissions);
        self
    }
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionDeny {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    selected_action_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    message: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    interrupt: Option<bool>,
}

impl PermissionDeny {
    pub fn selected_action_id(&self) -> Option<&str> {
        self.selected_action_id.as_deref()
    }

    pub fn message(&self) -> Option<&str> {
        self.message.as_deref()
    }

    pub fn interrupt(&self) -> Option<bool> {
        self.interrupt
    }

    pub fn set_selected_action_id(&mut self, selected_action_id: impl Into<String>) {
        self.selected_action_id = Some(selected_action_id.into());
    }

    pub fn with_selected_action_id(
        mut self,
        selected_action_id: impl Into<String>,
    ) -> PermissionDeny {
        self.set_selected_action_id(selected_action_id);
        self
    }

    pub fn set_message(&mut self, message: impl Into<String>) {
        self.message = Some(message.into());
    }

    pub fn with_message(mut self, message: impl Into<String>) -> PermissionDeny {
        self.set_message(message);
        self
    }

    pub fn set_interrupt(&mut self, interrupt: bool) {
        self.interrupt = Some(interrupt);
    }

    pub fn with_interrupt(mut self, interrupt: bool) -> PermissionDeny {
        self.set_interrupt(interrupt);
        self
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentPersistenceHandle {
    provider: String,
    session_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    native_handle: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    metadata: Option<Map<String, Value>>,
}

impl AgentPersistenceHandle {
    pub fn provider(&self) -> &str {
        &self.provider
    }

    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    pub fn native_handle(&self) -> Option<&str> {
        self.native_handle.as_deref()
    }

    pub fn metadata(&self) -> Option<&Map<String, Value>> {
        self.metadata.as_ref()
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AgentRuntimeInfo {
    provider: String,
    session_id: Option<String>,
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    thinking_option_id: Option<String>,
    #[serde(default)]
    mode_id: Option<String>,
}

impl AgentRuntimeInfo {
    pub fn provider(&self) -> &str {
        &self.provider
    }

    pub fn session_id(&self) -> Option<&str> {
        self.session_id.as_deref()
    }

    pub fn model(&self) -> Option<&str> {
        self.model.as_deref()
    }

    pub fn thinking_option_id(&self) -> Option<&str> {
        self.thinking_option_id.as_deref()
    }

    pub fn mode_id(&self) -> Option<&str> {
        self.mode_id.as_deref()
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ActiveTurn {
    turn_id: String,
    started_at: Option<String>,
}

impl ActiveTurn {
    pub fn turn_id(&self) -> &str {
        &self.turn_id
    }

    pub fn started_at(&self) -> Option<&str> {
        self.started_at.as_deref()
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AgentSnapshot {
    id: String,
    provider: String,
    cwd: String,
    #[serde(default)]
    workspace_id: Option<String>,
    model: Option<String>,
    #[serde(default)]
    features: Vec<AgentFeature>,
    #[serde(default)]
    thinking_option_id: Option<String>,
    #[serde(default)]
    effective_thinking_option_id: Option<String>,
    created_at: String,
    updated_at: String,
    last_user_message_at: Option<String>,
    status: AgentStatus,
    #[serde(default)]
    active_turn: Option<ActiveTurn>,
    capabilities: AgentCapabilities,
    current_mode_id: Option<String>,
    available_modes: Vec<AgentMode>,
    pending_permissions: Vec<PermissionRequest>,
    persistence: Option<AgentPersistenceHandle>,
    #[serde(default)]
    runtime_info: Option<AgentRuntimeInfo>,
    #[serde(default)]
    last_usage: Option<AgentUsage>,
    #[serde(default)]
    last_error: Option<String>,
    title: Option<String>,
    #[serde(default)]
    labels: HashMap<String, String>,
    #[serde(default)]
    requires_attention: bool,
    #[serde(default)]
    attention_reason: Option<AttentionReason>,
    #[serde(default)]
    attention_timestamp: Option<String>,
    #[serde(default)]
    archived_at: Option<String>,
    #[serde(default)]
    provider_unavailable: bool,
}

impl AgentSnapshot {
    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn provider(&self) -> &str {
        &self.provider
    }

    pub fn cwd(&self) -> &str {
        &self.cwd
    }

    pub fn workspace_id(&self) -> Option<&str> {
        self.workspace_id.as_deref()
    }

    pub fn model(&self) -> Option<&str> {
        self.model.as_deref()
    }

    pub fn features(&self) -> &[AgentFeature] {
        &self.features
    }

    pub fn thinking_option_id(&self) -> Option<&str> {
        self.thinking_option_id.as_deref()
    }

    pub fn effective_thinking_option_id(&self) -> Option<&str> {
        self.effective_thinking_option_id.as_deref()
    }

    pub fn created_at(&self) -> &str {
        &self.created_at
    }

    pub fn updated_at(&self) -> &str {
        &self.updated_at
    }

    pub fn last_user_message_at(&self) -> Option<&str> {
        self.last_user_message_at.as_deref()
    }

    pub fn status(&self) -> AgentStatus {
        self.status
    }

    pub fn active_turn(&self) -> Option<&ActiveTurn> {
        self.active_turn.as_ref()
    }

    pub fn capabilities(&self) -> &AgentCapabilities {
        &self.capabilities
    }

    pub fn current_mode_id(&self) -> Option<&str> {
        self.current_mode_id.as_deref()
    }

    pub fn available_modes(&self) -> &[AgentMode] {
        &self.available_modes
    }

    pub fn pending_permissions(&self) -> &[PermissionRequest] {
        &self.pending_permissions
    }

    pub fn persistence(&self) -> Option<&AgentPersistenceHandle> {
        self.persistence.as_ref()
    }

    pub fn runtime_info(&self) -> Option<&AgentRuntimeInfo> {
        self.runtime_info.as_ref()
    }

    pub fn last_usage(&self) -> Option<&AgentUsage> {
        self.last_usage.as_ref()
    }

    pub fn last_error(&self) -> Option<&str> {
        self.last_error.as_deref()
    }

    pub fn title(&self) -> Option<&str> {
        self.title.as_deref()
    }

    pub fn labels(&self) -> &HashMap<String, String> {
        &self.labels
    }

    /// The agent that started this one, from its `paseo.parent-agent-id` label.
    pub fn parent_agent_id(&self) -> Option<&str> {
        self.labels
            .get("paseo.parent-agent-id")
            .map(|id| id.trim())
            .filter(|id| !id.is_empty())
    }

    pub fn requires_attention(&self) -> bool {
        self.requires_attention
    }

    pub fn attention_reason(&self) -> Option<AttentionReason> {
        self.attention_reason
    }

    pub fn attention_timestamp(&self) -> Option<&str> {
        self.attention_timestamp.as_deref()
    }

    pub fn archived_at(&self) -> Option<&str> {
        self.archived_at.as_deref()
    }

    pub fn provider_unavailable(&self) -> bool {
        self.provider_unavailable
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AgentDirectoryEntry {
    agent: AgentSnapshot,
    project: ProjectPlacement,
}

impl AgentDirectoryEntry {
    pub(crate) fn new(agent: AgentSnapshot, project: ProjectPlacement) -> AgentDirectoryEntry {
        AgentDirectoryEntry { agent, project }
    }

    pub fn agent(&self) -> &AgentSnapshot {
        &self.agent
    }

    pub fn project(&self) -> &ProjectPlacement {
        &self.project
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PageInfo {
    next_cursor: Option<String>,
    prev_cursor: Option<String>,
    has_more: bool,
}

impl PageInfo {
    pub fn next_cursor(&self) -> Option<&str> {
        self.next_cursor.as_deref()
    }

    pub fn prev_cursor(&self) -> Option<&str> {
        self.prev_cursor.as_deref()
    }

    pub fn has_more(&self) -> bool {
        self.has_more
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AgentDirectoryPage {
    entries: Vec<AgentDirectoryEntry>,
    page_info: PageInfo,
}

impl AgentDirectoryPage {
    pub fn entries(&self) -> &[AgentDirectoryEntry] {
        &self.entries
    }

    pub fn page_info(&self) -> &PageInfo {
        &self.page_info
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AgentUpdate {
    Remove(AgentRemoval),
    Upsert(AgentUpsert),
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AgentRemoval {
    agent_id: String,
}

impl AgentRemoval {
    pub fn agent_id(&self) -> &str {
        &self.agent_id
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct AgentUpsert {
    agent: Box<AgentSnapshot>,
    #[serde(default)]
    project: Option<ProjectPlacement>,
}

impl AgentUpsert {
    pub fn agent(&self) -> &AgentSnapshot {
        &self.agent
    }

    pub fn project(&self) -> Option<&ProjectPlacement> {
        self.project.as_ref()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentSortKey {
    CreatedAt,
    StatusPriority,
    Title,
    UpdatedAt,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SortDirection {
    Asc,
    Desc,
}

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentDirectoryFilter {
    #[serde(skip_serializing_if = "Option::is_none")]
    labels: Option<HashMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    project_keys: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    statuses: Option<Vec<AgentStatus>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    include_archived: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    requires_attention: Option<bool>,
}

impl AgentDirectoryFilter {
    pub fn labels(&self) -> Option<&HashMap<String, String>> {
        self.labels.as_ref()
    }

    pub fn set_labels(&mut self, labels: HashMap<String, String>) {
        self.labels = Some(labels);
    }

    pub fn with_labels(mut self, labels: HashMap<String, String>) -> AgentDirectoryFilter {
        self.set_labels(labels);
        self
    }

    pub fn project_keys(&self) -> Option<&[String]> {
        self.project_keys.as_deref()
    }

    pub fn set_project_keys(&mut self, project_keys: Vec<String>) {
        self.project_keys = Some(project_keys);
    }

    pub fn with_project_keys(mut self, project_keys: Vec<String>) -> AgentDirectoryFilter {
        self.set_project_keys(project_keys);
        self
    }

    pub fn statuses(&self) -> Option<&[AgentStatus]> {
        self.statuses.as_deref()
    }

    pub fn set_statuses(&mut self, statuses: Vec<AgentStatus>) {
        self.statuses = Some(statuses);
    }

    pub fn with_statuses(mut self, statuses: Vec<AgentStatus>) -> AgentDirectoryFilter {
        self.set_statuses(statuses);
        self
    }

    pub fn include_archived(&self) -> Option<bool> {
        self.include_archived
    }

    pub fn set_include_archived(&mut self, include_archived: bool) {
        self.include_archived = Some(include_archived);
    }

    pub fn with_include_archived(mut self, include_archived: bool) -> AgentDirectoryFilter {
        self.set_include_archived(include_archived);
        self
    }

    pub fn requires_attention(&self) -> Option<bool> {
        self.requires_attention
    }

    pub fn set_requires_attention(&mut self, requires_attention: bool) {
        self.requires_attention = Some(requires_attention);
    }

    pub fn with_requires_attention(mut self, requires_attention: bool) -> AgentDirectoryFilter {
        self.set_requires_attention(requires_attention);
        self
    }
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct Sort<K> {
    pub(crate) key: K,
    pub(crate) direction: SortDirection,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Page {
    limit: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    cursor: Option<String>,
}

impl Page {
    const MAX_LIMIT: u32 = 200;

    pub(crate) fn new(limit: u32, cursor: Option<String>) -> Result<Page, ArgumentError> {
        if !(1..=Page::MAX_LIMIT).contains(&limit) {
            return Err(ArgumentError::LimitOutOfRange {
                name: "page limit",
                value: limit,
                max: Page::MAX_LIMIT,
            });
        }
        Ok(Page { limit, cursor })
    }
}

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentDirectoryQuery {
    #[serde(skip_serializing_if = "Option::is_none")]
    filter: Option<AgentDirectoryFilter>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    sort: Vec<Sort<AgentSortKey>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    page: Option<Page>,
}

impl AgentDirectoryQuery {
    pub fn filter(&self) -> Option<&AgentDirectoryFilter> {
        self.filter.as_ref()
    }

    pub fn set_filter(&mut self, filter: AgentDirectoryFilter) {
        self.filter = Some(filter);
    }

    pub fn with_filter(mut self, filter: AgentDirectoryFilter) -> AgentDirectoryQuery {
        self.set_filter(filter);
        self
    }

    pub fn add_sort(&mut self, key: AgentSortKey, direction: SortDirection) {
        self.sort.push(Sort { key, direction });
    }

    pub fn with_sort(mut self, key: AgentSortKey, direction: SortDirection) -> AgentDirectoryQuery {
        self.add_sort(key, direction);
        self
    }

    pub fn set_page(&mut self, limit: u32, cursor: Option<String>) -> Result<(), ArgumentError> {
        self.page = Some(Page::new(limit, cursor)?);
        Ok(())
    }

    pub fn with_page(
        mut self,
        limit: u32,
        cursor: Option<String>,
    ) -> Result<AgentDirectoryQuery, ArgumentError> {
        self.set_page(limit, cursor)?;
        Ok(self)
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum NoticeLevel {
    Error,
    Info,
    Warning,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct ProviderNotice {
    #[serde(rename = "type")]
    level: NoticeLevel,
    message: String,
}

impl ProviderNotice {
    pub fn level(&self) -> NoticeLevel {
        self.level
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AgentActionOutcome {
    agent_id: String,
    #[serde(default)]
    notice: Option<ProviderNotice>,
}

impl AgentActionOutcome {
    pub(crate) fn new(agent_id: String, notice: Option<ProviderNotice>) -> AgentActionOutcome {
        AgentActionOutcome { agent_id, notice }
    }

    pub fn agent_id(&self) -> &str {
        &self.agent_id
    }

    pub fn notice(&self) -> Option<&ProviderNotice> {
        self.notice.as_ref()
    }
}

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentConfigPatch {
    #[serde(skip_serializing_if = "Option::is_none")]
    model_id: Option<Option<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    mode_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    thinking_option_id: Option<Option<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    feature_values: Option<Map<String, Value>>,
}

impl AgentConfigPatch {
    pub fn model_id(&self) -> Option<Option<&str>> {
        self.model_id.as_ref().map(Option::as_deref)
    }

    pub fn set_model_id(&mut self, model_id: Option<String>) {
        self.model_id = Some(model_id);
    }

    pub fn with_model_id(mut self, model_id: Option<String>) -> AgentConfigPatch {
        self.set_model_id(model_id);
        self
    }

    pub fn mode_id(&self) -> Option<&str> {
        self.mode_id.as_deref()
    }

    pub fn set_mode_id(&mut self, mode_id: impl Into<String>) {
        self.mode_id = Some(mode_id.into());
    }

    pub fn with_mode_id(mut self, mode_id: impl Into<String>) -> AgentConfigPatch {
        self.set_mode_id(mode_id);
        self
    }

    pub fn thinking_option_id(&self) -> Option<Option<&str>> {
        self.thinking_option_id.as_ref().map(Option::as_deref)
    }

    pub fn set_thinking_option_id(&mut self, thinking_option_id: Option<String>) {
        self.thinking_option_id = Some(thinking_option_id);
    }

    pub fn with_thinking_option_id(
        mut self,
        thinking_option_id: Option<String>,
    ) -> AgentConfigPatch {
        self.set_thinking_option_id(thinking_option_id);
        self
    }

    pub fn feature_values(&self) -> Option<&Map<String, Value>> {
        self.feature_values.as_ref()
    }

    pub fn set_feature_value(&mut self, feature_id: impl Into<String>, value: Value) {
        self.feature_values
            .get_or_insert_with(Map::new)
            .insert(feature_id.into(), value);
    }

    pub fn with_feature_value(
        mut self,
        feature_id: impl Into<String>,
        value: Value,
    ) -> AgentConfigPatch {
        self.set_feature_value(feature_id, value);
        self
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentSessionConfig {
    provider: String,
    cwd: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    mode_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    thinking_option_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    feature_values: Option<Map<String, Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    title: Option<String>,
}

impl AgentSessionConfig {
    pub fn new(provider: impl Into<String>, cwd: impl Into<String>) -> AgentSessionConfig {
        AgentSessionConfig {
            provider: provider.into(),
            cwd: cwd.into(),
            mode_id: None,
            model: None,
            thinking_option_id: None,
            feature_values: None,
            title: None,
        }
    }

    pub fn provider(&self) -> &str {
        &self.provider
    }

    pub fn cwd(&self) -> &str {
        &self.cwd
    }

    pub fn mode_id(&self) -> Option<&str> {
        self.mode_id.as_deref()
    }

    pub fn set_mode_id(&mut self, mode_id: impl Into<String>) {
        self.mode_id = Some(mode_id.into());
    }

    pub fn with_mode_id(mut self, mode_id: impl Into<String>) -> AgentSessionConfig {
        self.set_mode_id(mode_id);
        self
    }

    pub fn model(&self) -> Option<&str> {
        self.model.as_deref()
    }

    pub fn set_model(&mut self, model: impl Into<String>) {
        self.model = Some(model.into());
    }

    pub fn with_model(mut self, model: impl Into<String>) -> AgentSessionConfig {
        self.set_model(model);
        self
    }

    pub fn thinking_option_id(&self) -> Option<&str> {
        self.thinking_option_id.as_deref()
    }

    pub fn set_thinking_option_id(&mut self, thinking_option_id: impl Into<String>) {
        self.thinking_option_id = Some(thinking_option_id.into());
    }

    pub fn with_thinking_option_id(
        mut self,
        thinking_option_id: impl Into<String>,
    ) -> AgentSessionConfig {
        self.set_thinking_option_id(thinking_option_id);
        self
    }

    pub fn feature_values(&self) -> Option<&Map<String, Value>> {
        self.feature_values.as_ref()
    }

    pub fn set_feature_value(&mut self, feature_id: impl Into<String>, value: Value) {
        self.feature_values
            .get_or_insert_with(Map::new)
            .insert(feature_id.into(), value);
    }

    pub fn with_feature_value(
        mut self,
        feature_id: impl Into<String>,
        value: Value,
    ) -> AgentSessionConfig {
        self.set_feature_value(feature_id, value);
        self
    }

    pub fn title(&self) -> Option<&str> {
        self.title.as_deref()
    }

    pub fn set_title(&mut self, title: impl Into<String>) -> Result<(), ArgumentError> {
        const MAX_TITLE_UTF16: usize = 200;
        let title = title.into();
        let length = title.trim().encode_utf16().count();
        if length == 0 {
            return Err(ArgumentError::TitleBlank);
        }
        if length > MAX_TITLE_UTF16 {
            return Err(ArgumentError::TitleTooLong {
                length,
                max: MAX_TITLE_UTF16,
            });
        }
        self.title = Some(title);
        Ok(())
    }

    pub fn with_title(
        mut self,
        title: impl Into<String>,
    ) -> Result<AgentSessionConfig, ArgumentError> {
        self.set_title(title)?;
        Ok(self)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(transparent)]
pub struct WorktreeTarget(WorktreeTargetMode);

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(
    tag = "mode",
    rename_all = "kebab-case",
    rename_all_fields = "camelCase"
)]
enum WorktreeTargetMode {
    BranchOff {
        new_branch: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        base: Option<String>,
    },
    CheckoutBranch {
        branch: String,
    },
    CheckoutPr {
        pr_number: NonZeroU32,
    },
}

impl WorktreeTarget {
    pub fn branch_off(new_branch: impl Into<String>, base: Option<String>) -> WorktreeTarget {
        WorktreeTarget(WorktreeTargetMode::BranchOff {
            new_branch: new_branch.into(),
            base,
        })
    }

    pub fn checkout_branch(branch: impl Into<String>) -> WorktreeTarget {
        WorktreeTarget(WorktreeTargetMode::CheckoutBranch {
            branch: branch.into(),
        })
    }

    pub fn checkout_pr(pr_number: NonZeroU32) -> WorktreeTarget {
        WorktreeTarget(WorktreeTargetMode::CheckoutPr { pr_number })
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageAttachment {
    data: String,
    mime_type: String,
}

impl ImageAttachment {
    pub fn data(&self) -> &str {
        &self.data
    }

    pub fn mime_type(&self) -> &str {
        &self.mime_type
    }

    pub fn new(data: impl Into<String>, mime_type: impl Into<String>) -> ImageAttachment {
        ImageAttachment {
            data: data.into(),
            mime_type: mime_type.into(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExternalResource {
    provider: String,
    provider_label: String,
    resource_type: String,
    id: String,
    identifier: String,
    title: String,
    url: String,
}

impl ExternalResource {
    pub fn provider(&self) -> &str {
        &self.provider
    }

    pub fn provider_label(&self) -> &str {
        &self.provider_label
    }

    pub fn resource_type(&self) -> &str {
        &self.resource_type
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn identifier(&self) -> &str {
        &self.identifier
    }

    pub fn title(&self) -> &str {
        &self.title
    }

    pub fn url(&self) -> &str {
        &self.url
    }

    pub fn new(
        provider: impl Into<String>,
        provider_label: impl Into<String>,
        resource_type: impl Into<String>,
        id: impl Into<String>,
        identifier: impl Into<String>,
        title: impl Into<String>,
        url: impl Into<String>,
    ) -> ExternalResource {
        ExternalResource {
            provider: provider.into(),
            provider_label: provider_label.into(),
            resource_type: resource_type.into(),
            id: id.into(),
            identifier: identifier.into(),
            title: title.into(),
            url: url.into(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TextAttachment {
    #[serde(skip_deserializing, default = "TextAttachment::mime_type")]
    mime_type: &'static str,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    context_kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    title: Option<String>,
    text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    external_resource: Option<ExternalResource>,
}

impl TextAttachment {
    pub fn new(text: impl Into<String>) -> TextAttachment {
        TextAttachment {
            mime_type: TextAttachment::mime_type(),
            context_kind: None,
            title: None,
            text: text.into(),
            external_resource: None,
        }
    }

    fn mime_type() -> &'static str {
        "text/plain"
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn title(&self) -> Option<&str> {
        self.title.as_deref()
    }

    pub fn set_title(&mut self, title: impl Into<String>) {
        self.title = Some(title.into());
    }

    pub fn with_title(mut self, title: impl Into<String>) -> TextAttachment {
        self.set_title(title);
        self
    }

    pub fn context_kind(&self) -> Option<&str> {
        self.context_kind.as_deref()
    }

    pub fn set_context_kind(&mut self, context_kind: impl Into<String>) {
        self.context_kind = Some(context_kind.into());
    }

    pub fn with_context_kind(mut self, context_kind: impl Into<String>) -> TextAttachment {
        self.set_context_kind(context_kind);
        self
    }

    pub fn external_resource(&self) -> Option<&ExternalResource> {
        self.external_resource.as_ref()
    }

    pub fn set_external_resource(&mut self, external_resource: ExternalResource) {
        self.external_resource = Some(external_resource);
    }

    pub fn with_external_resource(mut self, external_resource: ExternalResource) -> TextAttachment {
        self.set_external_resource(external_resource);
        self
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewSide {
    New,
    Old,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewLineKind {
    Add,
    Context,
    Remove,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewContextLine {
    old_line_number: Option<NonZeroU32>,
    new_line_number: Option<NonZeroU32>,
    #[serde(rename = "type")]
    kind: ReviewLineKind,
    content: String,
}

impl ReviewContextLine {
    pub fn old_line_number(&self) -> Option<NonZeroU32> {
        self.old_line_number
    }

    pub fn new_line_number(&self) -> Option<NonZeroU32> {
        self.new_line_number
    }

    pub fn kind(&self) -> ReviewLineKind {
        self.kind
    }

    pub fn content(&self) -> &str {
        &self.content
    }

    pub fn new(
        old_line_number: Option<NonZeroU32>,
        new_line_number: Option<NonZeroU32>,
        kind: ReviewLineKind,
        content: impl Into<String>,
    ) -> ReviewContextLine {
        ReviewContextLine {
            old_line_number,
            new_line_number,
            kind,
            content: content.into(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewCommentContext {
    hunk_header: String,
    target_line: ReviewContextLine,
    lines: Vec<ReviewContextLine>,
}

impl ReviewCommentContext {
    pub fn hunk_header(&self) -> &str {
        &self.hunk_header
    }

    pub fn target_line(&self) -> &ReviewContextLine {
        &self.target_line
    }

    pub fn lines(&self) -> &[ReviewContextLine] {
        &self.lines
    }

    pub fn new(
        hunk_header: impl Into<String>,
        target_line: ReviewContextLine,
        lines: Vec<ReviewContextLine>,
    ) -> ReviewCommentContext {
        ReviewCommentContext {
            hunk_header: hunk_header.into(),
            target_line,
            lines,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewComment {
    file_path: String,
    side: ReviewSide,
    line_number: NonZeroU32,
    body: String,
    context: ReviewCommentContext,
}

impl ReviewComment {
    pub fn file_path(&self) -> &str {
        &self.file_path
    }

    pub fn side(&self) -> ReviewSide {
        self.side
    }

    pub fn line_number(&self) -> NonZeroU32 {
        self.line_number
    }

    pub fn body(&self) -> &str {
        &self.body
    }

    pub fn context(&self) -> &ReviewCommentContext {
        &self.context
    }

    pub fn new(
        file_path: impl Into<String>,
        side: ReviewSide,
        line_number: NonZeroU32,
        body: impl Into<String>,
        context: ReviewCommentContext,
    ) -> ReviewComment {
        ReviewComment {
            file_path: file_path.into(),
            side,
            line_number,
            body: body.into(),
            context,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewAttachment {
    mime_type: &'static str,
    cwd: String,
    mode: DiffMode,
    #[serde(skip_serializing_if = "Option::is_none")]
    base_ref: Option<String>,
    comments: Vec<ReviewComment>,
}

impl ReviewAttachment {
    pub fn new(
        cwd: impl Into<String>,
        mode: DiffMode,
        base_ref: Option<String>,
        comments: Vec<ReviewComment>,
    ) -> ReviewAttachment {
        ReviewAttachment {
            mime_type: "application/paseo-review",
            cwd: cwd.into(),
            mode,
            base_ref,
            comments,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UploadedFileAttachment {
    id: String,
    file_name: String,
    mime_type: String,
    size: u64,
    path: String,
}

impl UploadedFileAttachment {
    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn file_name(&self) -> &str {
        &self.file_name
    }

    pub fn mime_type(&self) -> &str {
        &self.mime_type
    }

    pub fn size(&self) -> u64 {
        self.size
    }

    pub fn path(&self) -> &str {
        &self.path
    }

    pub fn new(
        id: impl Into<String>,
        file_name: impl Into<String>,
        mime_type: impl Into<String>,
        size: u64,
        path: impl Into<String>,
    ) -> UploadedFileAttachment {
        UploadedFileAttachment {
            id: id.into(),
            file_name: file_name.into(),
            mime_type: mime_type.into(),
            size,
            path: path.into(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AgentAttachment {
    Review(ReviewAttachment),
    Text(TextAttachment),
    UploadedFile(UploadedFileAttachment),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ActiveTurnBehavior {
    Interrupt,
    Steer,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentMessage {
    text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    message_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    active_turn_behavior: Option<ActiveTurnBehavior>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    images: Vec<ImageAttachment>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    attachments: Vec<AgentAttachment>,
}

impl AgentMessage {
    pub fn new(text: impl Into<String>) -> AgentMessage {
        AgentMessage {
            text: text.into(),
            message_id: None,
            active_turn_behavior: None,
            images: Vec::new(),
            attachments: Vec::new(),
        }
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn message_id(&self) -> Option<&str> {
        self.message_id.as_deref()
    }

    pub fn set_message_id(&mut self, message_id: impl Into<String>) {
        self.message_id = Some(message_id.into());
    }

    pub fn with_message_id(mut self, message_id: impl Into<String>) -> AgentMessage {
        self.set_message_id(message_id);
        self
    }

    pub fn active_turn_behavior(&self) -> Option<ActiveTurnBehavior> {
        self.active_turn_behavior
    }

    pub fn set_active_turn_behavior(&mut self, active_turn_behavior: ActiveTurnBehavior) {
        self.active_turn_behavior = Some(active_turn_behavior);
    }

    pub fn with_active_turn_behavior(
        mut self,
        active_turn_behavior: ActiveTurnBehavior,
    ) -> AgentMessage {
        self.set_active_turn_behavior(active_turn_behavior);
        self
    }

    pub fn images(&self) -> &[ImageAttachment] {
        &self.images
    }

    pub fn add_image(&mut self, image: ImageAttachment) {
        self.images.push(image);
    }

    pub fn with_image(mut self, image: ImageAttachment) -> AgentMessage {
        self.add_image(image);
        self
    }

    pub fn attachments(&self) -> &[AgentAttachment] {
        &self.attachments
    }

    pub fn add_attachment(&mut self, attachment: AgentAttachment) {
        self.attachments.push(attachment);
    }

    pub fn with_attachment(mut self, attachment: AgentAttachment) -> AgentMessage {
        self.add_attachment(attachment);
        self
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentCreateRequest {
    idempotency_key: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    agent_id: Option<String>,
    config: AgentSessionConfig,
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    env: HashMap<String, String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    workspace_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    worktree_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    worktree: Option<WorktreeTarget>,
    #[serde(skip_serializing_if = "Option::is_none")]
    initial_prompt: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    client_message_id: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    images: Vec<ImageAttachment>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    attachments: Vec<AgentAttachment>,
    labels: HashMap<String, String>,
}

impl AgentCreateRequest {
    pub fn new(config: AgentSessionConfig) -> AgentCreateRequest {
        AgentCreateRequest {
            idempotency_key: new_id(),
            agent_id: None,
            config,
            env: HashMap::new(),
            workspace_id: None,
            worktree_name: None,
            worktree: None,
            initial_prompt: None,
            client_message_id: None,
            images: Vec::new(),
            attachments: Vec::new(),
            labels: HashMap::new(),
        }
    }

    pub fn idempotency_key(&self) -> &str {
        &self.idempotency_key
    }

    pub fn agent_id(&self) -> Option<&str> {
        self.agent_id.as_deref()
    }

    pub fn set_agent_id(&mut self, agent_id: impl Into<String>) {
        self.agent_id = Some(agent_id.into());
    }

    pub fn with_agent_id(mut self, agent_id: impl Into<String>) -> AgentCreateRequest {
        self.set_agent_id(agent_id);
        self
    }

    pub fn config(&self) -> &AgentSessionConfig {
        &self.config
    }

    pub fn env(&self) -> &HashMap<String, String> {
        &self.env
    }

    pub fn set_env(&mut self, name: impl Into<String>, value: impl Into<String>) {
        self.env.insert(name.into(), value.into());
    }

    pub fn with_env(
        mut self,
        name: impl Into<String>,
        value: impl Into<String>,
    ) -> AgentCreateRequest {
        self.set_env(name, value);
        self
    }

    pub fn workspace_id(&self) -> Option<&str> {
        self.workspace_id.as_deref()
    }

    pub fn set_workspace_id(&mut self, workspace_id: impl Into<String>) {
        self.workspace_id = Some(workspace_id.into());
    }

    pub fn with_workspace_id(mut self, workspace_id: impl Into<String>) -> AgentCreateRequest {
        self.set_workspace_id(workspace_id);
        self
    }

    pub fn worktree(&self) -> Option<&WorktreeTarget> {
        self.worktree.as_ref()
    }

    pub fn set_worktree(&mut self, worktree: WorktreeTarget) {
        self.worktree = Some(worktree);
    }

    pub fn with_worktree(mut self, worktree: WorktreeTarget) -> AgentCreateRequest {
        self.set_worktree(worktree);
        self
    }

    pub fn worktree_name(&self) -> Option<&str> {
        self.worktree_name.as_deref()
    }

    pub fn set_worktree_name(&mut self, worktree_name: impl Into<String>) {
        self.worktree_name = Some(worktree_name.into());
    }

    pub fn with_worktree_name(mut self, worktree_name: impl Into<String>) -> AgentCreateRequest {
        self.set_worktree_name(worktree_name);
        self
    }

    pub fn initial_prompt(&self) -> Option<&str> {
        self.initial_prompt.as_deref()
    }

    pub fn set_initial_prompt(&mut self, initial_prompt: impl Into<String>) {
        self.initial_prompt = Some(initial_prompt.into());
    }

    pub fn with_initial_prompt(mut self, initial_prompt: impl Into<String>) -> AgentCreateRequest {
        self.set_initial_prompt(initial_prompt);
        self
    }

    pub fn client_message_id(&self) -> Option<&str> {
        self.client_message_id.as_deref()
    }

    pub fn set_client_message_id(&mut self, client_message_id: impl Into<String>) {
        self.client_message_id = Some(client_message_id.into());
    }

    pub fn with_client_message_id(
        mut self,
        client_message_id: impl Into<String>,
    ) -> AgentCreateRequest {
        self.set_client_message_id(client_message_id);
        self
    }

    pub fn images(&self) -> &[ImageAttachment] {
        &self.images
    }

    pub fn add_image(&mut self, image: ImageAttachment) {
        self.images.push(image);
    }

    pub fn with_image(mut self, image: ImageAttachment) -> AgentCreateRequest {
        self.add_image(image);
        self
    }

    pub fn attachments(&self) -> &[AgentAttachment] {
        &self.attachments
    }

    pub fn add_attachment(&mut self, attachment: AgentAttachment) {
        self.attachments.push(attachment);
    }

    pub fn with_attachment(mut self, attachment: AgentAttachment) -> AgentCreateRequest {
        self.add_attachment(attachment);
        self
    }

    pub fn labels(&self) -> &HashMap<String, String> {
        &self.labels
    }

    pub fn set_label(&mut self, name: impl Into<String>, value: impl Into<String>) {
        self.labels.insert(name.into(), value.into());
    }

    pub fn with_label(
        mut self,
        name: impl Into<String>,
        value: impl Into<String>,
    ) -> AgentCreateRequest {
        self.set_label(name, value);
        self
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum CreationKind {
    Agent,
    Workspace,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum CreationPhase {
    Accepted,
    AgentReady,
    Completed,
    Failed,
    PromptStarted,
    WorkspaceReady,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum CreationStage {
    Agent,
    Prompt,
    Workspace,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CreationSnapshot {
    kind: CreationKind,
    idempotency_key: String,
    revision: u64,
    phase: CreationPhase,
    workspace_id: Option<String>,
    #[serde(default)]
    workspace: Option<Box<WorkspaceDescriptor>>,
    agent_id: Option<String>,
    #[serde(default)]
    agent: Option<Box<AgentSnapshot>>,
    error: Option<String>,
    #[serde(default)]
    setup_skipped_reason: Option<String>,
    #[serde(default)]
    error_code: Option<String>,
    #[serde(default)]
    failed_stage: Option<CreationStage>,
    #[serde(default)]
    outcome_unknown: bool,
}

impl CreationSnapshot {
    pub fn kind(&self) -> CreationKind {
        self.kind
    }

    pub fn idempotency_key(&self) -> &str {
        &self.idempotency_key
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn phase(&self) -> CreationPhase {
        self.phase
    }

    pub fn workspace_id(&self) -> Option<&str> {
        self.workspace_id.as_deref()
    }

    pub fn workspace(&self) -> Option<&WorkspaceDescriptor> {
        self.workspace.as_deref()
    }

    pub fn agent_id(&self) -> Option<&str> {
        self.agent_id.as_deref()
    }

    pub fn agent(&self) -> Option<&AgentSnapshot> {
        self.agent.as_deref()
    }

    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    pub fn setup_skipped_reason(&self) -> Option<&str> {
        self.setup_skipped_reason.as_deref()
    }

    pub fn error_code(&self) -> Option<&str> {
        self.error_code.as_deref()
    }

    pub fn failed_stage(&self) -> Option<CreationStage> {
        self.failed_stage
    }

    pub fn outcome_unknown(&self) -> bool {
        self.outcome_unknown
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AttentionNotificationData {
    server_id: String,
    #[serde(default)]
    workspace_id: Option<String>,
    agent_id: String,
    reason: AttentionReason,
}

impl AttentionNotificationData {
    pub fn server_id(&self) -> &str {
        &self.server_id
    }

    pub fn workspace_id(&self) -> Option<&str> {
        self.workspace_id.as_deref()
    }

    pub fn agent_id(&self) -> &str {
        &self.agent_id
    }

    pub fn reason(&self) -> AttentionReason {
        self.reason
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AttentionNotification {
    title: String,
    body: String,
    data: AttentionNotificationData,
}

impl AttentionNotification {
    pub fn title(&self) -> &str {
        &self.title
    }

    pub fn body(&self) -> &str {
        &self.body
    }

    pub fn data(&self) -> &AttentionNotificationData {
        &self.data
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AgentAttention {
    agent_id: String,
    reason: AttentionReason,
    timestamp: String,
    should_notify: bool,
    #[serde(default)]
    notification: Option<AttentionNotification>,
}

impl AgentAttention {
    pub fn agent_id(&self) -> &str {
        &self.agent_id
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
#[serde(rename_all = "camelCase")]
pub struct PermissionRequested {
    agent_id: String,
    request: PermissionRequest,
}

impl PermissionRequested {
    pub fn agent_id(&self) -> &str {
        &self.agent_id
    }

    pub fn request(&self) -> &PermissionRequest {
        &self.request
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PermissionResolved {
    agent_id: String,
    request_id: String,
    resolution: PermissionResponse,
}

impl PermissionResolved {
    pub fn agent_id(&self) -> &str {
        &self.agent_id
    }

    pub fn request_id(&self) -> &str {
        &self.request_id
    }

    pub fn resolution(&self) -> &PermissionResponse {
        &self.resolution
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RewindMode {
    Both,
    Conversation,
    Files,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum WaitStatus {
    Error,
    Idle,
    Permission,
    Timeout,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WaitOutcome {
    status: WaitStatus,
    #[serde(rename = "final")]
    final_snapshot: Option<Box<AgentSnapshot>>,
    error: Option<String>,
    last_message: Option<String>,
}

impl WaitOutcome {
    pub fn status(&self) -> WaitStatus {
        self.status
    }

    pub fn final_snapshot(&self) -> Option<&AgentSnapshot> {
        self.final_snapshot.as_deref()
    }

    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    pub fn last_message(&self) -> Option<&str> {
        self.last_message.as_deref()
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PromptPreview {
    seq: u64,
    timestamp: String,
    preview: String,
}

impl PromptPreview {
    pub fn seq(&self) -> u64 {
        self.seq
    }

    pub fn timestamp(&self) -> &str {
        &self.timestamp
    }

    pub fn preview(&self) -> &str {
        &self.preview
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ForkedContext {
    attachment: Option<TextAttachment>,
    item_count: u64,
    boundary_message_id: Option<String>,
    #[serde(default)]
    boundary_cursor: Option<TimelineCursor>,
}

impl ForkedContext {
    pub fn attachment(&self) -> Option<&TextAttachment> {
        self.attachment.as_ref()
    }

    pub fn item_count(&self) -> u64 {
        self.item_count
    }

    pub fn boundary_message_id(&self) -> Option<&str> {
        self.boundary_message_id.as_deref()
    }

    pub fn boundary_cursor(&self) -> Option<&TimelineCursor> {
        self.boundary_cursor.as_ref()
    }
}

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportAgentRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    provider: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    provider_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    session_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    provider_handle_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    cwd: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    workspace_id: Option<String>,
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    labels: HashMap<String, String>,
}

impl ImportAgentRequest {
    pub fn provider(&self) -> Option<&str> {
        self.provider.as_deref()
    }

    pub fn set_provider(&mut self, provider: impl Into<String>) {
        self.provider = Some(provider.into());
    }

    pub fn with_provider(mut self, provider: impl Into<String>) -> ImportAgentRequest {
        self.set_provider(provider);
        self
    }

    pub fn provider_id(&self) -> Option<&str> {
        self.provider_id.as_deref()
    }

    pub fn set_provider_id(&mut self, provider_id: impl Into<String>) {
        self.provider_id = Some(provider_id.into());
    }

    pub fn with_provider_id(mut self, provider_id: impl Into<String>) -> ImportAgentRequest {
        self.set_provider_id(provider_id);
        self
    }

    pub fn session_id(&self) -> Option<&str> {
        self.session_id.as_deref()
    }

    pub fn set_session_id(&mut self, session_id: impl Into<String>) {
        self.session_id = Some(session_id.into());
    }

    pub fn with_session_id(mut self, session_id: impl Into<String>) -> ImportAgentRequest {
        self.set_session_id(session_id);
        self
    }

    pub fn provider_handle_id(&self) -> Option<&str> {
        self.provider_handle_id.as_deref()
    }

    pub fn set_provider_handle_id(&mut self, provider_handle_id: impl Into<String>) {
        self.provider_handle_id = Some(provider_handle_id.into());
    }

    pub fn with_provider_handle_id(
        mut self,
        provider_handle_id: impl Into<String>,
    ) -> ImportAgentRequest {
        self.set_provider_handle_id(provider_handle_id);
        self
    }

    pub fn cwd(&self) -> Option<&str> {
        self.cwd.as_deref()
    }

    pub fn set_cwd(&mut self, cwd: impl Into<String>) {
        self.cwd = Some(cwd.into());
    }

    pub fn with_cwd(mut self, cwd: impl Into<String>) -> ImportAgentRequest {
        self.set_cwd(cwd);
        self
    }

    pub fn workspace_id(&self) -> Option<&str> {
        self.workspace_id.as_deref()
    }

    pub fn set_workspace_id(&mut self, workspace_id: impl Into<String>) {
        self.workspace_id = Some(workspace_id.into());
    }

    pub fn with_workspace_id(mut self, workspace_id: impl Into<String>) -> ImportAgentRequest {
        self.set_workspace_id(workspace_id);
        self
    }

    pub fn labels(&self) -> &HashMap<String, String> {
        &self.labels
    }

    pub fn set_label(&mut self, name: impl Into<String>, value: impl Into<String>) {
        self.labels.insert(name.into(), value.into());
    }

    pub fn with_label(
        mut self,
        name: impl Into<String>,
        value: impl Into<String>,
    ) -> ImportAgentRequest {
        self.set_label(name, value);
        self
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum SlashCommandKind {
    Skill,
    #[default]
    #[serde(other)]
    Command,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SlashCommand {
    name: String,
    description: String,
    argument_hint: String,
    #[serde(default)]
    kind: SlashCommandKind,
}

impl SlashCommand {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn description(&self) -> &str {
        &self.description
    }

    pub fn argument_hint(&self) -> &str {
        &self.argument_hint
    }

    pub fn kind(&self) -> SlashCommandKind {
        self.kind
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DraftConfig {
    provider: String,
    cwd: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    mode_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    thinking_option_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    feature_values: Option<Map<String, Value>>,
}

impl From<&AgentSessionConfig> for DraftConfig {
    fn from(config: &AgentSessionConfig) -> DraftConfig {
        DraftConfig {
            provider: config.provider.clone(),
            cwd: config.cwd.clone(),
            mode_id: config.mode_id.clone(),
            model: config.model.clone(),
            thinking_option_id: config.thinking_option_id.clone(),
            feature_values: config.feature_values.clone(),
        }
    }
}
