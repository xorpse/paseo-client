use serde::{Deserialize, Serialize};

use crate::protocol::timeline::{TimelineCursor, TimelineDirection, TimelineItem, TimelineWindow};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum SubagentStatus {
    Canceled,
    Completed,
    Failed,
    Running,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProviderSubagent {
    id: String,
    parent_agent_id: String,
    #[serde(default)]
    parent_subagent_id: Option<String>,
    provider: String,
    title: Option<String>,
    description: Option<String>,
    status: SubagentStatus,
    created_at: String,
    updated_at: String,
    tool_call_id: Option<String>,
    #[serde(default)]
    cwd: Option<String>,
    #[serde(default)]
    subtitle: Option<String>,
}

impl ProviderSubagent {
    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn parent_agent_id(&self) -> &str {
        &self.parent_agent_id
    }

    pub fn parent_subagent_id(&self) -> Option<&str> {
        self.parent_subagent_id.as_deref()
    }

    pub fn provider(&self) -> &str {
        &self.provider
    }

    pub fn title(&self) -> Option<&str> {
        self.title.as_deref()
    }

    pub fn description(&self) -> Option<&str> {
        self.description.as_deref()
    }

    pub fn status(&self) -> SubagentStatus {
        self.status
    }

    pub fn created_at(&self) -> &str {
        &self.created_at
    }

    pub fn updated_at(&self) -> &str {
        &self.updated_at
    }

    pub fn tool_call_id(&self) -> Option<&str> {
        self.tool_call_id.as_deref()
    }

    pub fn cwd(&self) -> Option<&str> {
        self.cwd.as_deref()
    }

    pub fn subtitle(&self) -> Option<&str> {
        self.subtitle.as_deref()
    }

    pub fn label(&self) -> &str {
        self.title
            .as_deref()
            .filter(|title| !title.is_empty())
            .or(self.description.as_deref())
            .unwrap_or(&self.id)
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SubagentUpdate {
    Remove(SubagentRemoval),
    Timeline(SubagentTimelineUpdate),
    Upsert(SubagentUpsert),
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SubagentRemoval {
    parent_agent_id: String,
    subagent_id: String,
}

impl SubagentRemoval {
    pub fn parent_agent_id(&self) -> &str {
        &self.parent_agent_id
    }

    pub fn subagent_id(&self) -> &str {
        &self.subagent_id
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SubagentTimelineUpdate {
    parent_agent_id: String,
    subagent_id: String,
    provider: String,
    item: Box<TimelineItem>,
    timestamp: String,
    seq: u64,
    epoch: String,
}

impl SubagentTimelineUpdate {
    pub fn parent_agent_id(&self) -> &str {
        &self.parent_agent_id
    }

    pub fn subagent_id(&self) -> &str {
        &self.subagent_id
    }

    pub fn provider(&self) -> &str {
        &self.provider
    }

    pub fn item(&self) -> &TimelineItem {
        &self.item
    }

    pub fn timestamp(&self) -> &str {
        &self.timestamp
    }

    pub fn seq(&self) -> u64 {
        self.seq
    }

    pub fn epoch(&self) -> &str {
        &self.epoch
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct SubagentUpsert {
    subagent: Box<ProviderSubagent>,
}

impl SubagentUpsert {
    pub fn subagent(&self) -> &ProviderSubagent {
        &self.subagent
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SubagentTimelineRow {
    item: TimelineItem,
    timestamp: String,
    seq: u64,
    #[serde(default)]
    seq_start: Option<u64>,
    #[serde(default)]
    seq_end: Option<u64>,
}

impl SubagentTimelineRow {
    pub fn item(&self) -> &TimelineItem {
        &self.item
    }

    pub fn timestamp(&self) -> &str {
        &self.timestamp
    }

    pub fn seq(&self) -> u64 {
        self.seq
    }

    pub fn seq_start(&self) -> Option<u64> {
        self.seq_start
    }

    pub fn seq_end(&self) -> Option<u64> {
        self.seq_end
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SubagentTimelinePage {
    parent_agent_id: String,
    subagent_id: String,
    provider: Option<String>,
    direction: TimelineDirection,
    epoch: String,
    #[serde(default)]
    start_cursor: Option<TimelineCursor>,
    #[serde(default)]
    end_cursor: Option<TimelineCursor>,
    reset: bool,
    stale_cursor: bool,
    gap: bool,
    window: TimelineWindow,
    has_older: bool,
    has_newer: bool,
    rows: Vec<SubagentTimelineRow>,
}

impl SubagentTimelinePage {
    pub fn parent_agent_id(&self) -> &str {
        &self.parent_agent_id
    }

    pub fn subagent_id(&self) -> &str {
        &self.subagent_id
    }

    pub fn provider(&self) -> Option<&str> {
        self.provider.as_deref()
    }

    pub fn direction(&self) -> TimelineDirection {
        self.direction
    }

    pub fn epoch(&self) -> &str {
        &self.epoch
    }

    pub fn start_cursor(&self) -> Option<&TimelineCursor> {
        self.start_cursor.as_ref()
    }

    pub fn end_cursor(&self) -> Option<&TimelineCursor> {
        self.end_cursor.as_ref()
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

    pub fn has_older(&self) -> bool {
        self.has_older
    }

    pub fn has_newer(&self) -> bool {
        self.has_newer
    }

    pub fn rows(&self) -> &[SubagentTimelineRow] {
        &self.rows
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SubagentTimelineQuery {
    parent_agent_id: String,
    subagent_id: String,
    direction: TimelineDirection,
    #[serde(skip_serializing_if = "Option::is_none")]
    cursor: Option<TimelineCursor>,
    #[serde(skip_serializing_if = "Option::is_none")]
    limit: Option<u32>,
}

impl SubagentTimelineQuery {
    pub fn tail(
        parent_agent_id: impl Into<String>,
        subagent_id: impl Into<String>,
        limit: u32,
    ) -> SubagentTimelineQuery {
        SubagentTimelineQuery {
            parent_agent_id: parent_agent_id.into(),
            subagent_id: subagent_id.into(),
            direction: TimelineDirection::Tail,
            cursor: None,
            limit: Some(limit),
        }
    }

    pub fn before(
        parent_agent_id: impl Into<String>,
        subagent_id: impl Into<String>,
        cursor: TimelineCursor,
        limit: u32,
    ) -> SubagentTimelineQuery {
        SubagentTimelineQuery {
            parent_agent_id: parent_agent_id.into(),
            subagent_id: subagent_id.into(),
            direction: TimelineDirection::Before,
            cursor: Some(cursor),
            limit: Some(limit),
        }
    }
}
