use serde::Deserialize;
use serde_json::{json, Value};

use crate::protocol::timeline::TimelineItem;

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderSubagent {
    pub id: String,
    #[serde(default)]
    pub parent_agent_id: String,
    #[serde(default)]
    pub provider: String,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub created_at: String,
    #[serde(default)]
    pub updated_at: String,
    #[serde(default)]
    pub tool_call_id: Option<String>,
    #[serde(default)]
    pub cwd: Option<String>,
}

impl ProviderSubagent {
    pub fn is_running(&self) -> bool {
        self.status == "running"
    }

    pub fn label(&self) -> &str {
        self.title
            .as_deref()
            .filter(|title| !title.is_empty())
            .or(self.description.as_deref())
            .unwrap_or(&self.id)
    }
}

#[derive(Clone, Debug)]
pub enum SubagentUpdate {
    Remove {
        parent_agent_id: String,
        subagent_id: String,
    },
    Timeline {
        parent_agent_id: String,
        subagent_id: String,
        item: Box<TimelineItem>,
    },
    Upsert(Box<ProviderSubagent>),
}

pub fn list_request(request_id: &str, parent_agent_id: &str) -> Value {
    json!({
        "type": "agent.provider_subagents.list.request",
        "parentAgentId": parent_agent_id,
        "requestId": request_id
    })
}

pub fn timeline_request(
    request_id: &str,
    parent_agent_id: &str,
    subagent_id: &str,
    direction: &str,
    limit: u32,
) -> Value {
    json!({
        "type": "agent.provider_subagents.timeline.get.request",
        "parentAgentId": parent_agent_id,
        "subagentId": subagent_id,
        "requestId": request_id,
        "direction": direction,
        "limit": limit
    })
}

pub fn parse_list(payload: &Value) -> Vec<ProviderSubagent> {
    payload
        .get("subagents")
        .and_then(Value::as_array)
        .map(|entries| {
            entries
                .iter()
                .filter_map(|entry| serde_json::from_value(entry.clone()).ok())
                .collect()
        })
        .unwrap_or_default()
}

pub fn parse_timeline(payload: &Value) -> Vec<TimelineItem> {
    payload
        .get("rows")
        .and_then(Value::as_array)
        .map(|rows| {
            rows.iter()
                .filter_map(|row| row.get("item").cloned())
                .filter_map(|item| serde_json::from_value(item).ok())
                .collect()
        })
        .unwrap_or_default()
}

pub fn parse_update(payload: &Value) -> Option<SubagentUpdate> {
    let parent_agent_id = || {
        payload
            .get("parentAgentId")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string()
    };
    let subagent_id = || {
        payload
            .get("subagentId")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string()
    };
    match payload.get("kind").and_then(Value::as_str)? {
        "remove" => Some(SubagentUpdate::Remove {
            parent_agent_id: parent_agent_id(),
            subagent_id: subagent_id(),
        }),
        "timeline" => {
            let item = serde_json::from_value(payload.get("item")?.clone()).ok()?;
            Some(SubagentUpdate::Timeline {
                parent_agent_id: parent_agent_id(),
                subagent_id: subagent_id(),
                item: Box::new(item),
            })
        }
        "upsert" => {
            let subagent = serde_json::from_value(payload.get("subagent")?.clone()).ok()?;
            Some(SubagentUpdate::Upsert(Box::new(subagent)))
        }
        _ => None,
    }
}
