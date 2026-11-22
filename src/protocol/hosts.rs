use std::collections::HashMap;

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum HostStatus {
    Connecting,
    Disabled,
    Error,
    Offline,
    Online,
    #[serde(other)]
    Unknown,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct HostSource {
    id: String,
    key: String,
}

impl HostSource {
    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn key(&self) -> &str {
        &self.key
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct HostFailure {
    code: String,
    message: String,
}

impl HostFailure {
    pub fn code(&self) -> &str {
        &self.code
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct HostSummary {
    registration_id: String,
    server_id: Option<String>,
    label: Option<String>,
    source: HostSource,
    #[serde(default)]
    metadata: HashMap<String, String>,
    enabled: bool,
    status: HostStatus,
    target_description: String,
    last_online_at: Option<String>,
    next_retry_at: Option<String>,
    error: Option<HostFailure>,
}

impl HostSummary {
    pub fn registration_id(&self) -> &str {
        &self.registration_id
    }

    pub fn server_id(&self) -> Option<&str> {
        self.server_id.as_deref()
    }

    pub fn label(&self) -> Option<&str> {
        self.label.as_deref()
    }

    pub fn source(&self) -> &HostSource {
        &self.source
    }

    pub fn metadata(&self) -> &HashMap<String, String> {
        &self.metadata
    }

    pub fn enabled(&self) -> bool {
        self.enabled
    }

    pub fn status(&self) -> HostStatus {
        self.status
    }

    pub fn target_description(&self) -> &str {
        &self.target_description
    }

    pub fn last_online_at(&self) -> Option<&str> {
        self.last_online_at.as_deref()
    }

    pub fn next_retry_at(&self) -> Option<&str> {
        self.next_retry_at.as_deref()
    }

    pub fn error(&self) -> Option<&HostFailure> {
        self.error.as_ref()
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct HostIdentity {
    server_id: String,
    label: String,
}

impl HostIdentity {
    pub fn server_id(&self) -> &str {
        &self.server_id
    }

    pub fn label(&self) -> &str {
        &self.label
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct HostDirectory {
    #[serde(rename = "self")]
    local: HostIdentity,
    hosts: Vec<HostSummary>,
}

impl HostDirectory {
    pub fn local(&self) -> &HostIdentity {
        &self.local
    }

    pub fn hosts(&self) -> &[HostSummary] {
        &self.hosts
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HostAdd {
    connection: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    label: Option<String>,
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    metadata: HashMap<String, String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    source_key: Option<String>,
}

impl HostAdd {
    pub fn new(connection: impl Into<String>) -> HostAdd {
        HostAdd {
            connection: connection.into(),
            label: None,
            metadata: HashMap::new(),
            source_key: None,
        }
    }

    pub fn connection(&self) -> &str {
        &self.connection
    }

    pub fn label(&self) -> Option<&str> {
        self.label.as_deref()
    }

    pub fn set_label(&mut self, label: impl Into<String>) {
        self.label = Some(label.into());
    }

    pub fn with_label(mut self, label: impl Into<String>) -> HostAdd {
        self.set_label(label);
        self
    }

    pub fn metadata(&self) -> &HashMap<String, String> {
        &self.metadata
    }

    pub fn set_metadata(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.metadata.insert(key.into(), value.into());
    }

    pub fn with_metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> HostAdd {
        self.set_metadata(key, value);
        self
    }

    pub fn source_key(&self) -> Option<&str> {
        self.source_key.as_deref()
    }

    pub fn set_source_key(&mut self, source_key: impl Into<String>) {
        self.source_key = Some(source_key.into());
    }

    pub fn with_source_key(mut self, source_key: impl Into<String>) -> HostAdd {
        self.set_source_key(source_key);
        self
    }
}

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HostUpdate {
    #[serde(skip_serializing_if = "Option::is_none")]
    label: Option<Option<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    metadata: Option<HashMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    connection: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    replace_identity: Option<bool>,
}

impl HostUpdate {
    pub fn label(&self) -> Option<Option<&str>> {
        self.label.as_ref().map(Option::as_deref)
    }

    pub fn set_label(&mut self, label: Option<String>) {
        self.label = Some(label);
    }

    pub fn with_label(mut self, label: Option<String>) -> HostUpdate {
        self.set_label(label);
        self
    }

    pub fn metadata(&self) -> Option<&HashMap<String, String>> {
        self.metadata.as_ref()
    }

    pub fn set_metadata(&mut self, metadata: HashMap<String, String>) {
        self.metadata = Some(metadata);
    }

    pub fn with_metadata(mut self, metadata: HashMap<String, String>) -> HostUpdate {
        self.set_metadata(metadata);
        self
    }

    pub fn enabled(&self) -> Option<bool> {
        self.enabled
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = Some(enabled);
    }

    pub fn with_enabled(mut self, enabled: bool) -> HostUpdate {
        self.set_enabled(enabled);
        self
    }

    pub fn connection(&self) -> Option<&str> {
        self.connection.as_deref()
    }

    pub fn set_connection(&mut self, connection: impl Into<String>) {
        self.connection = Some(connection.into());
    }

    pub fn with_connection(mut self, connection: impl Into<String>) -> HostUpdate {
        self.set_connection(connection);
        self
    }

    pub fn replace_identity(&self) -> Option<bool> {
        self.replace_identity
    }

    pub fn set_replace_identity(&mut self, replace_identity: bool) {
        self.replace_identity = Some(replace_identity);
    }

    pub fn with_replace_identity(mut self, replace_identity: bool) -> HostUpdate {
        self.set_replace_identity(replace_identity);
        self
    }
}
