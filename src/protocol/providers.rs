use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::error::ArgumentError;
use crate::protocol::agents::{AgentMode, ModelDefinition};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ProviderStatus {
    Error,
    Loading,
    Ready,
    Unavailable,
    #[serde(other)]
    Unknown,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ProviderSource {
    Builtin,
    Custom,
    #[serde(other)]
    Unknown,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProviderInstallation {
    command: Vec<String>,
    version: Option<String>,
    permission_modes: Option<Vec<String>>,
    selected: bool,
    error: Option<String>,
}

impl ProviderInstallation {
    pub fn command(&self) -> &[String] {
        &self.command
    }
    pub fn version(&self) -> Option<&str> {
        self.version.as_deref()
    }
    pub fn permission_modes(&self) -> Option<&[String]> {
        self.permission_modes.as_deref()
    }
    pub fn selected(&self) -> bool {
        self.selected
    }
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct ProviderInstallations {
    installations: Vec<ProviderInstallation>,
    explicit: bool,
}

impl ProviderInstallations {
    pub fn installations(&self) -> &[ProviderInstallation] {
        &self.installations
    }
    pub fn explicit(&self) -> bool {
        self.explicit
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderSetupAction {
    Install,
    Login,
    Status,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProviderSetup {
    terminal_id: String,
}

impl ProviderSetup {
    pub fn terminal_id(&self) -> &str {
        &self.terminal_id
    }
}

fn enabled_by_default() -> bool {
    true
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProviderSnapshotEntry {
    provider: String,
    status: ProviderStatus,
    #[serde(default = "enabled_by_default")]
    enabled: bool,
    #[serde(default)]
    source: Option<ProviderSource>,
    #[serde(default)]
    error: Option<String>,
    #[serde(default)]
    models: Vec<ModelDefinition>,
    #[serde(default)]
    modes: Vec<AgentMode>,
    #[serde(default)]
    fetched_at: Option<String>,
    #[serde(default)]
    label: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    icon_svg: Option<String>,
    #[serde(default)]
    default_mode_id: Option<String>,
}

impl ProviderSnapshotEntry {
    pub fn provider(&self) -> &str {
        &self.provider
    }

    pub fn status(&self) -> ProviderStatus {
        self.status
    }

    pub fn enabled(&self) -> bool {
        self.enabled
    }

    pub fn source(&self) -> Option<ProviderSource> {
        self.source
    }

    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    pub fn models(&self) -> &[ModelDefinition] {
        &self.models
    }

    pub fn modes(&self) -> &[AgentMode] {
        &self.modes
    }

    pub fn fetched_at(&self) -> Option<&str> {
        self.fetched_at.as_deref()
    }

    pub fn label(&self) -> Option<&str> {
        self.label.as_deref()
    }

    pub fn description(&self) -> Option<&str> {
        self.description.as_deref()
    }

    pub fn icon_svg(&self) -> Option<&str> {
        self.icon_svg.as_deref()
    }

    pub fn default_mode_id(&self) -> Option<&str> {
        self.default_mode_id.as_deref()
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProvidersSnapshot {
    #[serde(default)]
    cwd: Option<String>,
    entries: Vec<ProviderSnapshotEntry>,
    #[serde(default)]
    snapshot_hash: Option<String>,
    #[serde(default)]
    fetched_at: HashMap<String, String>,
    generated_at: String,
}

impl ProvidersSnapshot {
    pub fn cwd(&self) -> Option<&str> {
        self.cwd.as_deref()
    }

    pub fn entries(&self) -> &[ProviderSnapshotEntry] {
        &self.entries
    }

    pub fn snapshot_hash(&self) -> Option<&str> {
        self.snapshot_hash.as_deref()
    }

    pub fn fetched_at(&self) -> &HashMap<String, String> {
        &self.fetched_at
    }

    pub fn generated_at(&self) -> &str {
        &self.generated_at
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProviderAvailability {
    provider: String,
    available: bool,
    #[serde(default)]
    error: Option<String>,
}

impl ProviderAvailability {
    pub fn provider(&self) -> &str {
        &self.provider
    }

    pub fn available(&self) -> bool {
        self.available
    }

    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RecentProviderSession {
    provider_id: String,
    provider_label: String,
    provider_handle_id: String,
    cwd: String,
    title: Option<String>,
    first_prompt_preview: Option<String>,
    last_prompt_preview: Option<String>,
    last_activity_at: String,
}

impl RecentProviderSession {
    pub fn provider_id(&self) -> &str {
        &self.provider_id
    }

    pub fn provider_label(&self) -> &str {
        &self.provider_label
    }

    pub fn provider_handle_id(&self) -> &str {
        &self.provider_handle_id
    }

    pub fn cwd(&self) -> &str {
        &self.cwd
    }

    pub fn title(&self) -> Option<&str> {
        self.title.as_deref()
    }

    pub fn first_prompt_preview(&self) -> Option<&str> {
        self.first_prompt_preview.as_deref()
    }

    pub fn last_prompt_preview(&self) -> Option<&str> {
        self.last_prompt_preview.as_deref()
    }

    pub fn last_activity_at(&self) -> &str {
        &self.last_activity_at
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct ProviderSessionError {
    provider: String,
    message: String,
}

impl ProviderSessionError {
    pub fn provider(&self) -> &str {
        &self.provider
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RecentProviderSessions {
    entries: Vec<RecentProviderSession>,
    #[serde(default)]
    filtered_already_imported_count: u64,
    #[serde(default)]
    provider_errors: Vec<ProviderSessionError>,
}

impl RecentProviderSessions {
    pub fn entries(&self) -> &[RecentProviderSession] {
        &self.entries
    }

    pub fn filtered_already_imported_count(&self) -> u64 {
        self.filtered_already_imported_count
    }

    pub fn provider_errors(&self) -> &[ProviderSessionError] {
        &self.provider_errors
    }
}

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecentSessionsQuery {
    #[serde(skip_serializing_if = "Option::is_none")]
    cwd: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    providers: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    since: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    limit: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    query: Option<String>,
}

impl RecentSessionsQuery {
    pub fn cwd(&self) -> Option<&str> {
        self.cwd.as_deref()
    }

    pub fn set_cwd(&mut self, cwd: impl Into<String>) {
        self.cwd = Some(cwd.into());
    }

    pub fn with_cwd(mut self, cwd: impl Into<String>) -> RecentSessionsQuery {
        self.set_cwd(cwd);
        self
    }

    pub fn providers(&self) -> &[String] {
        &self.providers
    }

    pub fn set_providers(&mut self, providers: Vec<String>) {
        self.providers = providers;
    }

    pub fn with_providers(mut self, providers: Vec<String>) -> RecentSessionsQuery {
        self.set_providers(providers);
        self
    }

    pub fn since(&self) -> Option<&str> {
        self.since.as_deref()
    }

    pub fn set_since(&mut self, since: impl Into<String>) {
        self.since = Some(since.into());
    }

    pub fn with_since(mut self, since: impl Into<String>) -> RecentSessionsQuery {
        self.set_since(since);
        self
    }

    pub fn limit(&self) -> Option<u32> {
        self.limit
    }

    pub fn set_limit(&mut self, limit: u32) -> Result<(), ArgumentError> {
        const MAX_LIMIT: u32 = 200;
        if !(1..=MAX_LIMIT).contains(&limit) {
            return Err(ArgumentError::LimitOutOfRange {
                name: "recent session limit",
                value: limit,
                max: MAX_LIMIT,
            });
        }
        self.limit = Some(limit);
        Ok(())
    }

    pub fn with_limit(mut self, limit: u32) -> Result<RecentSessionsQuery, ArgumentError> {
        self.set_limit(limit)?;
        Ok(self)
    }

    pub fn query(&self) -> Option<&str> {
        self.query.as_deref()
    }

    pub fn set_query(&mut self, query: impl Into<String>) {
        self.query = Some(query.into());
    }

    pub fn with_query(mut self, query: impl Into<String>) -> RecentSessionsQuery {
        self.set_query(query);
        self
    }
}
