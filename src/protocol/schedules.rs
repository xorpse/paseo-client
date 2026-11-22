use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum ScheduleCadence {
    #[serde(rename = "cron")]
    Cron {
        expression: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        timezone: Option<String>,
    },
    #[serde(rename = "every", rename_all = "camelCase")]
    Every { every_ms: u64 },
    #[serde(other)]
    Unknown,
}

#[derive(Clone, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(from = "WireTarget", into = "WireTarget")]
pub enum ScheduleTarget {
    Agent { agent_id: String },
    NewAgent(ScheduleAgentConfig),
    Unknown,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(tag = "type")]
enum WireTarget {
    #[serde(rename = "agent", alias = "self", rename_all = "camelCase")]
    Agent { agent_id: String },
    #[serde(rename = "new-agent")]
    NewAgent { config: ScheduleAgentConfig },
    #[serde(other)]
    Unknown,
}

impl From<WireTarget> for ScheduleTarget {
    fn from(target: WireTarget) -> ScheduleTarget {
        match target {
            WireTarget::Agent { agent_id } => ScheduleTarget::Agent { agent_id },
            WireTarget::NewAgent { config } => ScheduleTarget::NewAgent(config),
            WireTarget::Unknown => ScheduleTarget::Unknown,
        }
    }
}

impl From<ScheduleTarget> for WireTarget {
    fn from(target: ScheduleTarget) -> WireTarget {
        match target {
            ScheduleTarget::Agent { agent_id } => WireTarget::Agent { agent_id },
            ScheduleTarget::NewAgent(config) => WireTarget::NewAgent { config },
            ScheduleTarget::Unknown => WireTarget::Unknown,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScheduleAgentConfig {
    provider: String,
    cwd: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    mode_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    thinking_option_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    archive_on_finish: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    isolation: Option<AgentIsolation>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    feature_values: Option<Map<String, Value>>,
}

impl ScheduleAgentConfig {
    pub fn new(provider: impl Into<String>, cwd: impl Into<String>) -> ScheduleAgentConfig {
        ScheduleAgentConfig {
            provider: provider.into(),
            cwd: cwd.into(),
            model: None,
            mode_id: None,
            thinking_option_id: None,
            archive_on_finish: None,
            isolation: None,
            title: None,
            feature_values: None,
        }
    }

    pub fn provider(&self) -> &str {
        &self.provider
    }

    pub fn cwd(&self) -> &str {
        &self.cwd
    }

    pub fn model(&self) -> Option<&str> {
        self.model.as_deref()
    }

    pub fn set_model(&mut self, model: impl Into<String>) {
        self.model = Some(model.into());
    }

    pub fn with_model(mut self, model: impl Into<String>) -> ScheduleAgentConfig {
        self.set_model(model);
        self
    }

    pub fn mode_id(&self) -> Option<&str> {
        self.mode_id.as_deref()
    }

    pub fn set_mode_id(&mut self, mode_id: impl Into<String>) {
        self.mode_id = Some(mode_id.into());
    }

    pub fn with_mode_id(mut self, mode_id: impl Into<String>) -> ScheduleAgentConfig {
        self.set_mode_id(mode_id);
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
    ) -> ScheduleAgentConfig {
        self.set_thinking_option_id(thinking_option_id);
        self
    }

    pub fn archive_on_finish(&self) -> Option<bool> {
        self.archive_on_finish
    }

    pub fn set_archive_on_finish(&mut self, archive_on_finish: bool) {
        self.archive_on_finish = Some(archive_on_finish);
    }

    pub fn with_archive_on_finish(mut self, archive_on_finish: bool) -> ScheduleAgentConfig {
        self.set_archive_on_finish(archive_on_finish);
        self
    }

    pub fn isolation(&self) -> Option<AgentIsolation> {
        self.isolation
    }

    pub fn set_isolation(&mut self, isolation: AgentIsolation) {
        self.isolation = Some(isolation);
    }

    pub fn with_isolation(mut self, isolation: AgentIsolation) -> ScheduleAgentConfig {
        self.set_isolation(isolation);
        self
    }

    pub fn title(&self) -> Option<&str> {
        self.title.as_deref()
    }

    pub fn set_title(&mut self, title: impl Into<String>) {
        self.title = Some(title.into());
    }

    pub fn with_title(mut self, title: impl Into<String>) -> ScheduleAgentConfig {
        self.set_title(title);
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
    ) -> ScheduleAgentConfig {
        self.set_feature_value(feature_id, value);
        self
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ScheduleStatus {
    Active,
    Paused,
    Completed,
    #[serde(other)]
    Unknown,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Schedule {
    id: String,
    #[serde(default)]
    name: Option<String>,
    prompt: String,
    cadence: ScheduleCadence,
    target: ScheduleTarget,
    status: ScheduleStatus,
    created_at: String,
    #[serde(default)]
    updated_at: Option<String>,
    #[serde(default)]
    next_run_at: Option<String>,
    #[serde(default)]
    last_run_at: Option<String>,
    #[serde(default)]
    paused_at: Option<String>,
    #[serde(default)]
    expires_at: Option<String>,
    #[serde(default)]
    max_runs: Option<u32>,
}

impl Schedule {
    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    pub fn prompt(&self) -> &str {
        &self.prompt
    }

    pub fn cadence(&self) -> &ScheduleCadence {
        &self.cadence
    }

    pub fn target(&self) -> &ScheduleTarget {
        &self.target
    }

    pub fn status(&self) -> ScheduleStatus {
        self.status
    }

    pub fn next_run_at(&self) -> Option<&str> {
        self.next_run_at.as_deref()
    }

    pub fn last_run_at(&self) -> Option<&str> {
        self.last_run_at.as_deref()
    }

    pub fn created_at(&self) -> &str {
        &self.created_at
    }

    pub fn updated_at(&self) -> Option<&str> {
        self.updated_at.as_deref()
    }

    pub fn paused_at(&self) -> Option<&str> {
        self.paused_at.as_deref()
    }

    pub fn expires_at(&self) -> Option<&str> {
        self.expires_at.as_deref()
    }

    pub fn max_runs(&self) -> Option<u32> {
        self.max_runs
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ScheduleRunStatus {
    Failed,
    Running,
    Succeeded,
    #[serde(other)]
    Unknown,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ScheduleRun {
    id: String,
    status: ScheduleRunStatus,
    #[serde(default)]
    scheduled_for: Option<String>,
    #[serde(default)]
    started_at: Option<String>,
    #[serde(default)]
    ended_at: Option<String>,
    #[serde(default)]
    agent_id: Option<String>,
    #[serde(default)]
    workspace_id: Option<String>,
    #[serde(default)]
    output: Option<String>,
    #[serde(default)]
    error: Option<String>,
}

impl ScheduleRun {
    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn status(&self) -> ScheduleRunStatus {
        self.status
    }

    pub fn scheduled_for(&self) -> Option<&str> {
        self.scheduled_for.as_deref()
    }

    pub fn started_at(&self) -> Option<&str> {
        self.started_at.as_deref()
    }

    pub fn ended_at(&self) -> Option<&str> {
        self.ended_at.as_deref()
    }

    pub fn agent_id(&self) -> Option<&str> {
        self.agent_id.as_deref()
    }

    pub fn workspace_id(&self) -> Option<&str> {
        self.workspace_id.as_deref()
    }

    pub fn output(&self) -> Option<&str> {
        self.output.as_deref()
    }

    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScheduleCreate {
    prompt: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    cadence: ScheduleCadence,
    target: ScheduleTarget,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_runs: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    run_on_create: Option<bool>,
}

impl ScheduleCreate {
    pub fn new(
        prompt: impl Into<String>,
        cadence: ScheduleCadence,
        target: ScheduleTarget,
    ) -> ScheduleCreate {
        ScheduleCreate {
            prompt: prompt.into(),
            name: None,
            cadence,
            target,
            max_runs: None,
            run_on_create: None,
        }
    }

    pub fn prompt(&self) -> &str {
        &self.prompt
    }

    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    pub fn set_name(&mut self, name: impl Into<String>) {
        self.name = Some(name.into());
    }

    pub fn with_name(mut self, name: impl Into<String>) -> ScheduleCreate {
        self.set_name(name);
        self
    }

    pub fn cadence(&self) -> &ScheduleCadence {
        &self.cadence
    }

    pub fn target(&self) -> &ScheduleTarget {
        &self.target
    }

    pub fn run_on_create(&self) -> Option<bool> {
        self.run_on_create
    }

    pub fn set_run_on_create(&mut self, run_on_create: bool) {
        self.run_on_create = Some(run_on_create);
    }

    pub fn with_max_runs(mut self, max_runs: u32) -> ScheduleCreate {
        self.max_runs = Some(max_runs);
        self
    }

    pub fn with_run_on_create(mut self, run_on_create: bool) -> ScheduleCreate {
        self.set_run_on_create(run_on_create);
        self
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct ScheduleDetail {
    #[serde(flatten)]
    schedule: Schedule,
    runs: Vec<ScheduleRun>,
}

impl ScheduleDetail {
    pub fn schedule(&self) -> &Schedule {
        &self.schedule
    }

    pub fn runs(&self) -> &[ScheduleRun] {
        &self.runs
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentIsolation {
    Local,
    Worktree,
    #[serde(other)]
    Unknown,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScheduleAgentPatch {
    #[serde(skip_serializing_if = "Option::is_none")]
    provider: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    model: Option<Option<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    mode_id: Option<Option<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    thinking_option_id: Option<Option<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    archive_on_finish: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    isolation: Option<AgentIsolation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    cwd: Option<String>,
}

impl ScheduleAgentPatch {
    pub fn provider(&self) -> Option<&str> {
        self.provider.as_deref()
    }

    pub fn set_provider(&mut self, provider: impl Into<String>) {
        self.provider = Some(provider.into());
    }

    pub fn with_provider(mut self, provider: impl Into<String>) -> ScheduleAgentPatch {
        self.set_provider(provider);
        self
    }

    /// The new model; `Some(None)` returns to the provider's default.
    pub fn model(&self) -> Option<Option<&str>> {
        self.model.as_ref().map(Option::as_deref)
    }

    pub fn set_model(&mut self, model: Option<String>) {
        self.model = Some(model);
    }

    pub fn with_model(mut self, model: Option<String>) -> ScheduleAgentPatch {
        self.set_model(model);
        self
    }

    pub fn mode_id(&self) -> Option<Option<&str>> {
        self.mode_id.as_ref().map(Option::as_deref)
    }

    pub fn set_mode_id(&mut self, mode_id: Option<String>) {
        self.mode_id = Some(mode_id);
    }

    pub fn with_mode_id(mut self, mode_id: Option<String>) -> ScheduleAgentPatch {
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
    ) -> ScheduleAgentPatch {
        self.set_thinking_option_id(thinking_option_id);
        self
    }

    pub fn archive_on_finish(&self) -> Option<bool> {
        self.archive_on_finish
    }

    pub fn set_archive_on_finish(&mut self, archive_on_finish: bool) {
        self.archive_on_finish = Some(archive_on_finish);
    }

    pub fn with_archive_on_finish(mut self, archive_on_finish: bool) -> ScheduleAgentPatch {
        self.set_archive_on_finish(archive_on_finish);
        self
    }

    pub fn isolation(&self) -> Option<AgentIsolation> {
        self.isolation
    }

    pub fn set_isolation(&mut self, isolation: AgentIsolation) {
        self.isolation = Some(isolation);
    }

    pub fn with_isolation(mut self, isolation: AgentIsolation) -> ScheduleAgentPatch {
        self.set_isolation(isolation);
        self
    }

    pub fn cwd(&self) -> Option<&str> {
        self.cwd.as_deref()
    }

    pub fn set_cwd(&mut self, cwd: impl Into<String>) {
        self.cwd = Some(cwd.into());
    }

    pub fn with_cwd(mut self, cwd: impl Into<String>) -> ScheduleAgentPatch {
        self.set_cwd(cwd);
        self
    }
}

/// Changes to a schedule. Unset fields keep their value; `Some(None)` clears an optional one.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScheduleUpdate {
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<Option<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    prompt: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    cadence: Option<ScheduleCadence>,
    #[serde(skip_serializing_if = "Option::is_none")]
    new_agent_config: Option<ScheduleAgentPatch>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_runs: Option<Option<u32>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    expires_at: Option<Option<String>>,
}

impl ScheduleUpdate {
    pub fn name(&self) -> Option<Option<&str>> {
        self.name.as_ref().map(Option::as_deref)
    }

    pub fn set_name(&mut self, name: Option<String>) {
        self.name = Some(name);
    }

    pub fn with_name(mut self, name: Option<String>) -> ScheduleUpdate {
        self.set_name(name);
        self
    }

    pub fn prompt(&self) -> Option<&str> {
        self.prompt.as_deref()
    }

    pub fn set_prompt(&mut self, prompt: impl Into<String>) {
        self.prompt = Some(prompt.into());
    }

    pub fn with_prompt(mut self, prompt: impl Into<String>) -> ScheduleUpdate {
        self.set_prompt(prompt);
        self
    }

    pub fn cadence(&self) -> Option<&ScheduleCadence> {
        self.cadence.as_ref()
    }

    pub fn set_cadence(&mut self, cadence: ScheduleCadence) {
        self.cadence = Some(cadence);
    }

    pub fn with_cadence(mut self, cadence: ScheduleCadence) -> ScheduleUpdate {
        self.set_cadence(cadence);
        self
    }

    /// Applies only to a schedule that starts a new agent for each run.
    pub fn new_agent_config(&self) -> Option<&ScheduleAgentPatch> {
        self.new_agent_config.as_ref()
    }

    pub fn set_new_agent_config(&mut self, config: ScheduleAgentPatch) {
        self.new_agent_config = Some(config);
    }

    pub fn with_new_agent_config(mut self, config: ScheduleAgentPatch) -> ScheduleUpdate {
        self.set_new_agent_config(config);
        self
    }

    pub fn max_runs(&self) -> Option<Option<u32>> {
        self.max_runs
    }

    pub fn set_max_runs(&mut self, max_runs: Option<u32>) {
        self.max_runs = Some(max_runs);
    }

    pub fn with_max_runs(mut self, max_runs: Option<u32>) -> ScheduleUpdate {
        self.set_max_runs(max_runs);
        self
    }

    pub fn expires_at(&self) -> Option<Option<&str>> {
        self.expires_at.as_ref().map(Option::as_deref)
    }

    pub fn set_expires_at(&mut self, expires_at: Option<String>) {
        self.expires_at = Some(expires_at);
    }

    pub fn with_expires_at(mut self, expires_at: Option<String>) -> ScheduleUpdate {
        self.set_expires_at(expires_at);
        self
    }
}
