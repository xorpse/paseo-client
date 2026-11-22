use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use thiserror::Error;

use crate::protocol::number::WireNumber;
use crate::protocol::skills::SkillSelection;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RelayConfig {
    enabled: bool,
    #[serde(default)]
    require_protected_security: Option<bool>,
}

impl RelayConfig {
    pub fn enabled(&self) -> bool {
        self.enabled
    }

    pub fn require_protected_security(&self) -> Option<bool> {
        self.require_protected_security
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct McpConfig {
    #[serde(default)]
    enabled: Option<bool>,
    inject_into_agents: bool,
}

impl McpConfig {
    pub fn enabled(&self) -> Option<bool> {
        self.enabled
    }

    pub fn inject_into_agents(&self) -> bool {
        self.inject_into_agents
    }
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PaseoToolsPolicy {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    enabled: Option<bool>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    disabled_tools: Vec<String>,
}

impl PaseoToolsPolicy {
    pub fn enabled(&self) -> Option<bool> {
        self.enabled
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = Some(enabled);
    }

    pub fn with_enabled(mut self, enabled: bool) -> PaseoToolsPolicy {
        self.set_enabled(enabled);
        self
    }

    pub fn disabled_tools(&self) -> &[String] {
        &self.disabled_tools
    }

    pub fn set_disabled_tools(&mut self, disabled_tools: Vec<String>) {
        self.disabled_tools = disabled_tools;
    }

    pub fn with_disabled_tools(mut self, disabled_tools: Vec<String>) -> PaseoToolsPolicy {
        self.set_disabled_tools(disabled_tools);
        self
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AdditionalModel {
    id: String,
    label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    is_default: Option<bool>,
}

impl AdditionalModel {
    pub fn new(id: impl Into<String>, label: impl Into<String>) -> AdditionalModel {
        AdditionalModel {
            id: id.into(),
            label: label.into(),
            description: None,
            is_default: None,
        }
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn label(&self) -> &str {
        &self.label
    }

    pub fn description(&self) -> Option<&str> {
        self.description.as_deref()
    }

    pub fn set_description(&mut self, description: impl Into<String>) {
        self.description = Some(description.into());
    }

    pub fn with_description(mut self, description: impl Into<String>) -> AdditionalModel {
        self.set_description(description);
        self
    }

    pub fn is_default(&self) -> bool {
        self.is_default == Some(true)
    }

    pub fn set_is_default(&mut self, is_default: bool) {
        self.is_default = Some(is_default);
    }

    pub fn with_is_default(mut self, is_default: bool) -> AdditionalModel {
        self.set_is_default(is_default);
        self
    }
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    extends: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    command: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    env: Option<HashMap<String, String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    params: Option<Map<String, Value>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    paseo_tools: Option<PaseoToolsPolicy>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    enabled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    additional_models: Option<Vec<AdditionalModel>>,
}

impl ProviderConfig {
    pub fn extends(&self) -> Option<&str> {
        self.extends.as_deref()
    }

    pub fn set_extends(&mut self, extends: impl Into<String>) {
        self.extends = Some(extends.into());
    }

    pub fn with_extends(mut self, extends: impl Into<String>) -> ProviderConfig {
        self.set_extends(extends);
        self
    }

    pub fn label(&self) -> Option<&str> {
        self.label.as_deref()
    }

    pub fn set_label(&mut self, label: impl Into<String>) {
        self.label = Some(label.into());
    }

    pub fn with_label(mut self, label: impl Into<String>) -> ProviderConfig {
        self.set_label(label);
        self
    }

    pub fn description(&self) -> Option<&str> {
        self.description.as_deref()
    }

    pub fn set_description(&mut self, description: impl Into<String>) {
        self.description = Some(description.into());
    }

    pub fn with_description(mut self, description: impl Into<String>) -> ProviderConfig {
        self.set_description(description);
        self
    }

    pub fn command(&self) -> Option<&[String]> {
        self.command.as_deref()
    }

    pub fn set_command(&mut self, command: Vec<String>) {
        self.command = Some(command);
    }

    pub fn with_command(mut self, command: Vec<String>) -> ProviderConfig {
        self.set_command(command);
        self
    }

    pub fn env(&self) -> Option<&HashMap<String, String>> {
        self.env.as_ref()
    }

    pub fn set_env(&mut self, env: HashMap<String, String>) {
        self.env = Some(env);
    }

    pub fn with_env(mut self, env: HashMap<String, String>) -> ProviderConfig {
        self.set_env(env);
        self
    }

    pub fn params(&self) -> Option<&Map<String, Value>> {
        self.params.as_ref()
    }

    pub fn set_params(&mut self, params: Map<String, Value>) {
        self.params = Some(params);
    }

    pub fn with_params(mut self, params: Map<String, Value>) -> ProviderConfig {
        self.set_params(params);
        self
    }

    pub fn paseo_tools(&self) -> Option<&PaseoToolsPolicy> {
        self.paseo_tools.as_ref()
    }

    pub fn set_paseo_tools(&mut self, paseo_tools: PaseoToolsPolicy) {
        self.paseo_tools = Some(paseo_tools);
    }

    pub fn with_paseo_tools(mut self, paseo_tools: PaseoToolsPolicy) -> ProviderConfig {
        self.set_paseo_tools(paseo_tools);
        self
    }

    pub fn enabled(&self) -> Option<bool> {
        self.enabled
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = Some(enabled);
    }

    pub fn with_enabled(mut self, enabled: bool) -> ProviderConfig {
        self.set_enabled(enabled);
        self
    }

    pub fn additional_models(&self) -> Option<&[AdditionalModel]> {
        self.additional_models.as_deref()
    }

    pub fn set_additional_models(&mut self, additional_models: Vec<AdditionalModel>) {
        self.additional_models = Some(additional_models);
    }

    pub fn with_additional_models(
        mut self,
        additional_models: Vec<AdditionalModel>,
    ) -> ProviderConfig {
        self.set_additional_models(additional_models);
        self
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerationProvider {
    provider: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    thinking_option_id: Option<String>,
}

impl GenerationProvider {
    pub fn new(provider: impl Into<String>) -> GenerationProvider {
        GenerationProvider {
            provider: provider.into(),
            model: None,
            thinking_option_id: None,
        }
    }

    pub fn provider(&self) -> &str {
        &self.provider
    }

    pub fn model(&self) -> Option<&str> {
        self.model.as_deref()
    }

    pub fn set_model(&mut self, model: impl Into<String>) {
        self.model = Some(model.into());
    }

    pub fn with_model(mut self, model: impl Into<String>) -> GenerationProvider {
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
    ) -> GenerationProvider {
        self.set_thinking_option_id(thinking_option_id);
        self
    }
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq)]
pub struct MetadataGeneration {
    #[serde(default)]
    providers: Vec<GenerationProvider>,
}

impl MetadataGeneration {
    pub fn providers(&self) -> &[GenerationProvider] {
        &self.providers
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct TerminalProfile {
    id: String,
    name: String,
    command: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    args: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    icon: Option<String>,
    #[serde(flatten)]
    extra: Map<String, Value>,
}

impl TerminalProfile {
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        command: impl Into<String>,
    ) -> TerminalProfile {
        TerminalProfile {
            id: id.into(),
            name: name.into(),
            command: command.into(),
            args: Vec::new(),
            icon: None,
            extra: Map::new(),
        }
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn command(&self) -> &str {
        &self.command
    }

    pub fn args(&self) -> &[String] {
        &self.args
    }

    pub fn set_args(&mut self, args: Vec<String>) {
        self.args = args;
    }

    pub fn with_args(mut self, args: Vec<String>) -> TerminalProfile {
        self.set_args(args);
        self
    }

    pub fn icon(&self) -> Option<&str> {
        self.icon.as_deref()
    }

    pub fn set_icon(&mut self, icon: impl Into<String>) {
        self.icon = Some(icon.into());
    }

    pub fn with_icon(mut self, icon: impl Into<String>) -> TerminalProfile {
        self.set_icon(icon);
        self
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentProfile {
    id: String,
    name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    icon: Option<String>,
    #[serde(rename = "color", default, skip_serializing_if = "Option::is_none")]
    colour: Option<String>,
    provider: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    mode_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    thinking_option_id: Option<String>,
    #[serde(default, skip_serializing_if = "Map::is_empty")]
    feature_values: Map<String, Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    notes: Option<String>,
    #[serde(flatten)]
    extra: Map<String, Value>,
}

impl AgentProfile {
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        provider: impl Into<String>,
    ) -> AgentProfile {
        AgentProfile {
            id: id.into(),
            name: name.into(),
            icon: None,
            colour: None,
            provider: provider.into(),
            model: None,
            mode_id: None,
            thinking_option_id: None,
            feature_values: Map::new(),
            notes: None,
            extra: Map::new(),
        }
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn provider(&self) -> &str {
        &self.provider
    }

    pub fn icon(&self) -> Option<&str> {
        self.icon.as_deref()
    }

    pub fn set_icon(&mut self, icon: impl Into<String>) {
        self.icon = Some(icon.into());
    }

    pub fn with_icon(mut self, icon: impl Into<String>) -> AgentProfile {
        self.set_icon(icon);
        self
    }

    pub fn colour(&self) -> Option<&str> {
        self.colour.as_deref()
    }

    pub fn set_colour(&mut self, colour: impl Into<String>) {
        self.colour = Some(colour.into());
    }

    pub fn with_colour(mut self, colour: impl Into<String>) -> AgentProfile {
        self.set_colour(colour);
        self
    }

    pub fn model(&self) -> Option<&str> {
        self.model.as_deref()
    }

    pub fn set_model(&mut self, model: impl Into<String>) {
        self.model = Some(model.into());
    }

    pub fn with_model(mut self, model: impl Into<String>) -> AgentProfile {
        self.set_model(model);
        self
    }

    pub fn mode_id(&self) -> Option<&str> {
        self.mode_id.as_deref()
    }

    pub fn set_mode_id(&mut self, mode_id: impl Into<String>) {
        self.mode_id = Some(mode_id.into());
    }

    pub fn with_mode_id(mut self, mode_id: impl Into<String>) -> AgentProfile {
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
    ) -> AgentProfile {
        self.set_thinking_option_id(thinking_option_id);
        self
    }

    pub fn feature_values(&self) -> &Map<String, Value> {
        &self.feature_values
    }

    pub fn set_feature_value(&mut self, feature_id: impl Into<String>, value: Value) {
        self.feature_values.insert(feature_id.into(), value);
    }

    pub fn with_feature_value(
        mut self,
        feature_id: impl Into<String>,
        value: Value,
    ) -> AgentProfile {
        self.set_feature_value(feature_id, value);
        self
    }

    pub fn notes(&self) -> Option<&str> {
        self.notes.as_deref()
    }

    pub fn set_notes(&mut self, notes: impl Into<String>) {
        self.notes = Some(notes.into());
    }

    pub fn with_notes(mut self, notes: impl Into<String>) -> AgentProfile {
        self.set_notes(notes);
        self
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
struct SkillsConfig {
    #[serde(default)]
    selection: Option<SkillSelection>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DaemonConfig {
    #[serde(default)]
    relay: Option<RelayConfig>,
    mcp: McpConfig,
    #[serde(default)]
    providers: HashMap<String, ProviderConfig>,
    #[serde(default)]
    metadata_generation: MetadataGeneration,
    #[serde(default)]
    auto_archive_after_merge: bool,
    #[serde(default)]
    enable_terminal_agent_hooks: bool,
    #[serde(default)]
    append_system_prompt: String,
    #[serde(default)]
    terminal_profiles: Option<Vec<TerminalProfile>>,
    #[serde(default)]
    agent_profiles: Option<Vec<AgentProfile>>,
    #[serde(default)]
    skills: Option<SkillsConfig>,
}

impl DaemonConfig {
    pub fn relay(&self) -> Option<&RelayConfig> {
        self.relay.as_ref()
    }

    pub fn mcp(&self) -> &McpConfig {
        &self.mcp
    }

    pub fn providers(&self) -> &HashMap<String, ProviderConfig> {
        &self.providers
    }

    pub fn metadata_generation(&self) -> &MetadataGeneration {
        &self.metadata_generation
    }

    pub fn auto_archive_after_merge(&self) -> bool {
        self.auto_archive_after_merge
    }

    pub fn enable_terminal_agent_hooks(&self) -> bool {
        self.enable_terminal_agent_hooks
    }

    pub fn append_system_prompt(&self) -> &str {
        &self.append_system_prompt
    }

    pub fn terminal_profiles(&self) -> Option<&[TerminalProfile]> {
        self.terminal_profiles.as_deref()
    }

    pub fn agent_profiles(&self) -> Option<&[AgentProfile]> {
        self.agent_profiles.as_deref()
    }

    pub fn skill_selection(&self) -> Option<&SkillSelection> {
        self.skills.as_ref()?.selection.as_ref()
    }
}

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
struct RelayPatch {
    #[serde(skip_serializing_if = "Option::is_none")]
    enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    require_protected_security: Option<bool>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct McpPatch {
    inject_into_agents: bool,
}

#[derive(Clone, Debug, Serialize)]
struct MetadataGenerationPatch {
    providers: Vec<GenerationProvider>,
}

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DaemonConfigPatch {
    #[serde(skip_serializing_if = "Option::is_none")]
    relay: Option<RelayPatch>,
    #[serde(skip_serializing_if = "Option::is_none")]
    mcp: Option<McpPatch>,
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    providers: HashMap<String, ProviderConfig>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    remove_providers: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    metadata_generation: Option<MetadataGenerationPatch>,
    #[serde(skip_serializing_if = "Option::is_none")]
    auto_archive_after_merge: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    enable_terminal_agent_hooks: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    append_system_prompt: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    terminal_profiles: Option<Vec<TerminalProfile>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    agent_profiles: Option<Vec<AgentProfile>>,
}

impl DaemonConfigPatch {
    pub fn relay_enabled(&self) -> Option<bool> {
        self.relay.as_ref()?.enabled
    }

    pub fn set_relay_enabled(&mut self, enabled: bool) {
        self.relay.get_or_insert_with(RelayPatch::default).enabled = Some(enabled);
    }

    pub fn with_relay_enabled(mut self, enabled: bool) -> DaemonConfigPatch {
        self.set_relay_enabled(enabled);
        self
    }

    pub fn require_protected_security(&self) -> Option<bool> {
        self.relay.as_ref()?.require_protected_security
    }

    pub fn set_require_protected_security(&mut self, required: bool) {
        self.relay
            .get_or_insert_with(RelayPatch::default)
            .require_protected_security = Some(required);
    }

    pub fn with_require_protected_security(mut self, required: bool) -> DaemonConfigPatch {
        self.set_require_protected_security(required);
        self
    }

    pub fn mcp_inject_into_agents(&self) -> Option<bool> {
        self.mcp.as_ref().map(|mcp| mcp.inject_into_agents)
    }

    pub fn set_mcp_inject_into_agents(&mut self, inject_into_agents: bool) {
        self.mcp = Some(McpPatch { inject_into_agents });
    }

    pub fn with_mcp_inject_into_agents(mut self, inject_into_agents: bool) -> DaemonConfigPatch {
        self.set_mcp_inject_into_agents(inject_into_agents);
        self
    }

    pub fn providers(&self) -> &HashMap<String, ProviderConfig> {
        &self.providers
    }

    pub fn set_provider(&mut self, provider: impl Into<String>, config: ProviderConfig) {
        self.providers.insert(provider.into(), config);
    }

    pub fn with_provider(
        mut self,
        provider: impl Into<String>,
        config: ProviderConfig,
    ) -> DaemonConfigPatch {
        self.set_provider(provider, config);
        self
    }

    pub fn removed_providers(&self) -> &[String] {
        &self.remove_providers
    }

    pub fn set_removed_providers(&mut self, providers: Vec<String>) {
        self.remove_providers = providers;
    }

    pub fn with_removed_providers(mut self, providers: Vec<String>) -> DaemonConfigPatch {
        self.set_removed_providers(providers);
        self
    }

    pub fn generation_providers(&self) -> Option<&[GenerationProvider]> {
        self.metadata_generation
            .as_ref()
            .map(|generation| generation.providers.as_slice())
    }

    pub fn set_generation_providers(&mut self, providers: Vec<GenerationProvider>) {
        self.metadata_generation = Some(MetadataGenerationPatch { providers });
    }

    pub fn with_generation_providers(
        mut self,
        providers: Vec<GenerationProvider>,
    ) -> DaemonConfigPatch {
        self.set_generation_providers(providers);
        self
    }

    pub fn auto_archive_after_merge(&self) -> Option<bool> {
        self.auto_archive_after_merge
    }

    pub fn set_auto_archive_after_merge(&mut self, enabled: bool) {
        self.auto_archive_after_merge = Some(enabled);
    }

    pub fn with_auto_archive_after_merge(mut self, enabled: bool) -> DaemonConfigPatch {
        self.set_auto_archive_after_merge(enabled);
        self
    }

    pub fn enable_terminal_agent_hooks(&self) -> Option<bool> {
        self.enable_terminal_agent_hooks
    }

    pub fn set_enable_terminal_agent_hooks(&mut self, enabled: bool) {
        self.enable_terminal_agent_hooks = Some(enabled);
    }

    pub fn with_enable_terminal_agent_hooks(mut self, enabled: bool) -> DaemonConfigPatch {
        self.set_enable_terminal_agent_hooks(enabled);
        self
    }

    pub fn append_system_prompt(&self) -> Option<&str> {
        self.append_system_prompt.as_deref()
    }

    pub fn set_append_system_prompt(&mut self, prompt: impl Into<String>) {
        self.append_system_prompt = Some(prompt.into());
    }

    pub fn with_append_system_prompt(mut self, prompt: impl Into<String>) -> DaemonConfigPatch {
        self.set_append_system_prompt(prompt);
        self
    }

    pub fn terminal_profiles(&self) -> Option<&[TerminalProfile]> {
        self.terminal_profiles.as_deref()
    }

    pub fn set_terminal_profiles(&mut self, profiles: Vec<TerminalProfile>) {
        self.terminal_profiles = Some(profiles);
    }

    pub fn with_terminal_profiles(mut self, profiles: Vec<TerminalProfile>) -> DaemonConfigPatch {
        self.set_terminal_profiles(profiles);
        self
    }

    pub fn agent_profiles(&self) -> Option<&[AgentProfile]> {
        self.agent_profiles.as_deref()
    }

    pub fn set_agent_profiles(&mut self, profiles: Vec<AgentProfile>) {
        self.agent_profiles = Some(profiles);
    }

    pub fn with_agent_profiles(mut self, profiles: Vec<AgentProfile>) -> DaemonConfigPatch {
        self.set_agent_profiles(profiles);
        self
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ConfigReload {
    applied_paths: Vec<String>,
    restart_required_paths: Vec<String>,
    override_controlled_paths: Vec<String>,
}

impl ConfigReload {
    pub fn applied_paths(&self) -> &[String] {
        &self.applied_paths
    }

    pub fn restart_required_paths(&self) -> &[String] {
        &self.restart_required_paths
    }

    pub fn override_controlled_paths(&self) -> &[String] {
        &self.override_controlled_paths
    }
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectConfigRevision {
    mtime_ms: WireNumber,
    size: WireNumber,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProjectConfig {
    #[serde(default)]
    config: Option<Map<String, Value>>,
    #[serde(default)]
    revision: Option<ProjectConfigRevision>,
    #[serde(default)]
    has_uncommitted_worktree_setup_changes: bool,
}

impl ProjectConfig {
    pub fn config(&self) -> Option<&Map<String, Value>> {
        self.config.as_ref()
    }

    pub fn revision(&self) -> Option<ProjectConfigRevision> {
        self.revision
    }

    pub fn has_uncommitted_worktree_setup_changes(&self) -> bool {
        self.has_uncommitted_worktree_setup_changes
    }
}

#[derive(Clone, Debug, Deserialize, Error, PartialEq)]
#[serde(tag = "code", rename_all = "snake_case")]
pub enum ProjectConfigError {
    #[error("the project's paseo.json is invalid")]
    InvalidProjectConfig,
    #[error("the project was not found")]
    ProjectNotFound,
    #[error("paseo.json changed since it was read")]
    StaleProjectConfig {
        #[serde(default, rename = "currentRevision")]
        current_revision: Option<ProjectConfigRevision>,
    },
    #[error("paseo.json could not be written")]
    WriteFailed,
    #[serde(other)]
    #[error("the project configuration request failed")]
    Unknown,
}
