use serde::Deserialize;

use crate::protocol::agents::{AgentMode, ModelDefinition};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ProviderStatus {
    Error,
    Loading,
    Ready,
    Unavailable,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ProviderSource {
    Builtin,
    Custom,
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
    generated_at: String,
}

impl ProvidersSnapshot {
    pub fn cwd(&self) -> Option<&str> {
        self.cwd.as_deref()
    }

    pub fn entries(&self) -> &[ProviderSnapshotEntry] {
        &self.entries
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
