use std::collections::HashMap;

use serde::Deserialize;
use serde_json::{json, Value};

use crate::protocol::timeline::ToolCallDetail;

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentMode {
    pub id: String,
    #[serde(default)]
    pub label: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SelectOption {
    pub id: String,
    #[serde(default)]
    pub label: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentFeature {
    #[serde(rename = "type", default)]
    pub kind: String,
    pub id: String,
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub value: Value,
    #[serde(default)]
    pub options: Vec<SelectOption>,
}

impl AgentFeature {
    pub fn is_toggle(&self) -> bool {
        self.kind == "toggle"
    }

    pub fn toggle_value(&self) -> bool {
        self.value.as_bool().unwrap_or(false)
    }

    pub fn select_value(&self) -> Option<&str> {
        self.value.as_str()
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SlashCommand {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub argument_hint: String,
    #[serde(default)]
    pub kind: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelDefinition {
    pub id: String,
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub thinking_options: Vec<SelectOption>,
    #[serde(default)]
    pub default_thinking_option_id: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentSnapshot {
    pub id: String,
    #[serde(default)]
    pub provider: String,
    #[serde(default)]
    pub cwd: String,
    #[serde(default)]
    pub workspace_id: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub current_mode_id: Option<String>,
    #[serde(default)]
    pub available_modes: Vec<AgentMode>,
    #[serde(default)]
    pub thinking_option_id: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub labels: HashMap<String, Value>,
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub updated_at: Option<String>,
    #[serde(default)]
    pub requires_attention: bool,
    #[serde(default)]
    pub attention_reason: Option<String>,
    #[serde(default)]
    pub archived_at: Option<String>,
    #[serde(default)]
    pub pending_permissions: Vec<PermissionRequest>,
    #[serde(default)]
    pub features: Vec<AgentFeature>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentListEntry {
    pub agent: AgentSnapshot,
    #[serde(default)]
    pub project: Value,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionAction {
    pub id: String,
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub behavior: String,
    #[serde(default)]
    pub variant: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionRequest {
    pub id: String,
    #[serde(default)]
    pub provider: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub detail: Option<ToolCallDetail>,
    #[serde(default)]
    pub input: Option<Value>,
    #[serde(default)]
    pub actions: Vec<PermissionAction>,
}

#[derive(Clone, Debug)]
pub enum PermissionResponse {
    Allow {
        selected_action_id: Option<String>,
        updated_input: Option<Value>,
    },
    Deny {
        message: Option<String>,
        interrupt: bool,
    },
}

impl PermissionResponse {
    pub fn to_value(&self) -> Value {
        match self {
            PermissionResponse::Allow {
                selected_action_id,
                updated_input,
            } => {
                let mut v = json!({ "behavior": "allow" });
                if let Some(id) = selected_action_id {
                    v["selectedActionId"] = json!(id);
                }
                if let Some(input) = updated_input {
                    v["updatedInput"] = input.clone();
                }
                v
            }
            PermissionResponse::Deny { message, interrupt } => {
                let mut v = json!({ "behavior": "deny", "interrupt": interrupt });
                if let Some(msg) = message {
                    v["message"] = json!(msg);
                }
                v
            }
        }
    }
}

pub fn fetch_agents_request(request_id: &str) -> Value {
    json!({
        "type": "fetch_agents_request",
        "requestId": request_id,
        "page": { "limit": 200 }
    })
}

pub fn subscribe_agents_request(request_id: &str, subscription_id: &str) -> Value {
    json!({
        "type": "fetch_agents_request",
        "requestId": request_id,
        "page": { "limit": 200 },
        "subscribe": { "subscriptionId": subscription_id }
    })
}

pub fn fetch_agent_request(request_id: &str, agent_id: &str) -> Value {
    json!({
        "type": "fetch_agent_request",
        "requestId": request_id,
        "agentId": agent_id
    })
}

pub fn fetch_agent_timeline_request(
    request_id: &str,
    agent_id: &str,
    direction: &str,
    limit: u32,
) -> Value {
    json!({
        "type": "fetch_agent_timeline_request",
        "requestId": request_id,
        "agentId": agent_id,
        "direction": direction,
        "limit": limit
    })
}

pub fn fetch_agent_timeline_page_request(
    request_id: &str,
    agent_id: &str,
    direction: &str,
    limit: u32,
    cursor: Option<&Value>,
) -> Value {
    let mut msg = json!({
        "type": "fetch_agent_timeline_request",
        "requestId": request_id,
        "agentId": agent_id,
        "direction": direction,
        "limit": limit
    });
    if let Some(cursor) = cursor {
        msg["cursor"] = cursor.clone();
    }
    msg
}

pub fn set_timeline_subscription_request(request_id: &str, agent_ids: &[String]) -> Value {
    json!({
        "type": "agent.timeline.set_subscription.request",
        "requestId": request_id,
        "agentIds": agent_ids
    })
}

pub fn send_agent_message_request(request_id: &str, agent_id: &str, text: &str) -> Value {
    json!({
        "type": "send_agent_message_request",
        "requestId": request_id,
        "agentId": agent_id,
        "text": text
    })
}

pub fn cancel_agent_request(request_id: &str, agent_id: &str) -> Value {
    json!({ "type": "cancel_agent_request", "agentId": agent_id, "requestId": request_id })
}

pub fn archive_agent_request(request_id: &str, agent_id: &str) -> Value {
    json!({ "type": "archive_agent_request", "agentId": agent_id, "requestId": request_id })
}

pub fn delete_agent_request(request_id: &str, agent_id: &str) -> Value {
    json!({ "type": "delete_agent_request", "agentId": agent_id, "requestId": request_id })
}

pub fn set_agent_mode_request(request_id: &str, agent_id: &str, mode_id: &str) -> Value {
    json!({
        "type": "set_agent_mode_request",
        "agentId": agent_id,
        "modeId": mode_id,
        "requestId": request_id
    })
}

pub fn set_agent_feature_request(
    request_id: &str,
    agent_id: &str,
    feature_id: &str,
    value: Value,
) -> Value {
    json!({
        "type": "set_agent_feature_request",
        "agentId": agent_id,
        "featureId": feature_id,
        "value": value,
        "requestId": request_id
    })
}

pub fn list_commands_request(request_id: &str, agent_id: &str) -> Value {
    json!({
        "type": "list_commands_request",
        "agentId": agent_id,
        "requestId": request_id
    })
}

pub fn set_agent_model_request(request_id: &str, agent_id: &str, model_id: &str) -> Value {
    json!({
        "type": "set_agent_model_request",
        "agentId": agent_id,
        "modelId": model_id,
        "requestId": request_id
    })
}

pub fn set_agent_thinking_request(
    request_id: &str,
    agent_id: &str,
    thinking_option_id: &str,
) -> Value {
    json!({
        "type": "set_agent_thinking_request",
        "agentId": agent_id,
        "thinkingOptionId": thinking_option_id,
        "requestId": request_id
    })
}

pub fn list_provider_models_request(request_id: &str, provider: &str, cwd: Option<&str>) -> Value {
    let mut msg = json!({
        "type": "list_provider_models_request",
        "provider": provider,
        "requestId": request_id
    });
    if let Some(cwd) = cwd {
        msg["cwd"] = json!(cwd);
    }
    msg
}

pub fn permission_response_message(
    agent_id: &str,
    request_id: &str,
    response: &PermissionResponse,
) -> Value {
    json!({
        "type": "agent_permission_response",
        "agentId": agent_id,
        "requestId": request_id,
        "response": response.to_value()
    })
}
