use std::num::NonZeroU16;

use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum TerminalActivityState {
    Attention,
    Working,
    #[default]
    #[serde(other)]
    Idle,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum TerminalAttentionReason {
    Finished,
    NeedsInput,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TerminalActivity {
    state: TerminalActivityState,
    #[serde(default, deserialize_with = "lenient_attention_reason")]
    attention_reason: Option<TerminalAttentionReason>,
    changed_at: f64,
}

impl TerminalActivity {
    pub fn state(&self) -> TerminalActivityState {
        self.state
    }

    pub fn attention_reason(&self) -> Option<TerminalAttentionReason> {
        self.attention_reason
    }

    pub fn changed_at(&self) -> f64 {
        self.changed_at
    }
}

fn lenient_attention_reason<'de, D>(
    deserializer: D,
) -> Result<Option<TerminalAttentionReason>, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Option::<Value>::deserialize(deserializer)?;
    Ok(value.and_then(|value| serde_json::from_value(value).ok()))
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TerminalInfo {
    id: String,
    name: String,
    #[serde(default)]
    cwd: Option<String>,
    #[serde(default)]
    workspace_id: Option<String>,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    activity: Option<TerminalActivity>,
}

impl TerminalInfo {
    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn cwd(&self) -> Option<&str> {
        self.cwd.as_deref()
    }

    pub fn workspace_id(&self) -> Option<&str> {
        self.workspace_id.as_deref()
    }

    pub fn title(&self) -> Option<&str> {
        self.title.as_deref()
    }

    pub fn activity(&self) -> Option<&TerminalActivity> {
        self.activity.as_ref()
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TerminalsChanged {
    #[serde(default)]
    workspace_id: Option<String>,
    cwd: String,
    terminals: Vec<TerminalInfo>,
}

impl TerminalsChanged {
    pub fn workspace_id(&self) -> Option<&str> {
        self.workspace_id.as_deref()
    }

    pub fn cwd(&self) -> &str {
        &self.cwd
    }

    pub fn terminals(&self) -> &[TerminalInfo] {
        &self.terminals
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TerminalAttentionRequired {
    #[serde(default)]
    server_id: Option<String>,
    terminal_id: String,
    cwd: String,
    #[serde(default)]
    workspace_id: Option<String>,
    reason: TerminalAttentionReason,
    title: String,
    body: String,
    should_notify: bool,
}

impl TerminalAttentionRequired {
    pub fn server_id(&self) -> Option<&str> {
        self.server_id.as_deref()
    }

    pub fn terminal_id(&self) -> &str {
        &self.terminal_id
    }

    pub fn cwd(&self) -> &str {
        &self.cwd
    }

    pub fn workspace_id(&self) -> Option<&str> {
        self.workspace_id.as_deref()
    }

    pub fn reason(&self) -> TerminalAttentionReason {
        self.reason
    }

    pub fn title(&self) -> &str {
        &self.title
    }

    pub fn body(&self) -> &str {
        &self.body
    }

    pub fn should_notify(&self) -> bool {
        self.should_notify
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct TerminalSize {
    rows: NonZeroU16,
    cols: NonZeroU16,
}

impl TerminalSize {
    pub fn rows(&self) -> NonZeroU16 {
        self.rows
    }

    pub fn cols(&self) -> NonZeroU16 {
        self.cols
    }

    pub const fn new(rows: NonZeroU16, cols: NonZeroU16) -> TerminalSize {
        TerminalSize { rows, cols }
    }
}

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateTerminalOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    workspace_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    command: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    args: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    size: Option<TerminalSize>,
}

impl CreateTerminalOptions {
    pub fn workspace_id(&self) -> Option<&str> {
        self.workspace_id.as_deref()
    }

    pub fn set_workspace_id(&mut self, workspace_id: impl Into<String>) {
        self.workspace_id = Some(workspace_id.into());
    }

    pub fn with_workspace_id(mut self, workspace_id: impl Into<String>) -> CreateTerminalOptions {
        self.set_workspace_id(workspace_id);
        self
    }

    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    pub fn set_name(&mut self, name: impl Into<String>) {
        self.name = Some(name.into());
    }

    pub fn with_name(mut self, name: impl Into<String>) -> CreateTerminalOptions {
        self.set_name(name);
        self
    }

    pub fn command(&self) -> Option<&str> {
        self.command.as_deref()
    }

    pub fn args(&self) -> &[String] {
        &self.args
    }

    pub fn set_command(&mut self, command: impl Into<String>, args: Vec<String>) {
        self.command = Some(command.into());
        self.args = args;
    }

    pub fn with_command(
        mut self,
        command: impl Into<String>,
        args: Vec<String>,
    ) -> CreateTerminalOptions {
        self.set_command(command, args);
        self
    }

    pub fn size(&self) -> Option<TerminalSize> {
        self.size
    }

    pub fn set_size(&mut self, size: TerminalSize) {
        self.size = Some(size);
    }

    pub fn with_size(mut self, size: TerminalSize) -> CreateTerminalOptions {
        self.set_size(size);
        self
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum RestoreMode {
    FullSnapshot,
    Live,
    VisibleSnapshot,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalRestore {
    mode: RestoreMode,
    #[serde(skip_serializing_if = "Option::is_none")]
    scrollback_lines: Option<u32>,
}

impl TerminalRestore {
    pub fn new(mode: RestoreMode) -> TerminalRestore {
        TerminalRestore {
            mode,
            scrollback_lines: None,
        }
    }

    pub fn mode(&self) -> RestoreMode {
        self.mode
    }

    pub fn scrollback_lines(&self) -> Option<u32> {
        self.scrollback_lines
    }

    pub fn set_scrollback_lines(&mut self, scrollback_lines: u32) {
        self.scrollback_lines = Some(scrollback_lines);
    }

    pub fn with_scrollback_lines(mut self, scrollback_lines: u32) -> TerminalRestore {
        self.set_scrollback_lines(scrollback_lines);
        self
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum TerminalStreamEvent {
    Exit(Option<String>),
    Output(Vec<u8>),
    Restore(Vec<u8>),
    Snapshot(Value),
}
