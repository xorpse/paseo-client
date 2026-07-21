use serde::Deserialize;
use serde_json::{json, Value};

use crate::error::{PaseoError, Result};

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalInfo {
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub cwd: String,
    #[serde(default)]
    pub workspace_id: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub activity: Option<String>,
}

pub fn list_terminals_request(request_id: &str, cwd: Option<&str>) -> Value {
    let mut msg = json!({
        "type": "list_terminals_request",
        "requestId": request_id
    });
    if let Some(cwd) = cwd {
        msg["cwd"] = json!(cwd);
    }
    msg
}

pub struct CreateTerminalOpts {
    pub name: Option<String>,
    pub agent_id: Option<String>,
    pub command: Option<String>,
    pub args: Option<Vec<String>>,
    pub workspace_id: Option<String>,
    pub rows: u32,
    pub cols: u32,
}

impl Default for CreateTerminalOpts {
    fn default() -> Self {
        CreateTerminalOpts {
            name: None,
            agent_id: None,
            command: None,
            args: None,
            workspace_id: None,
            rows: 24,
            cols: 80,
        }
    }
}

pub fn create_terminal_request(request_id: &str, cwd: &str, opts: &CreateTerminalOpts) -> Value {
    let mut msg = json!({
        "type": "create_terminal_request",
        "requestId": request_id,
        "cwd": cwd,
        "size": { "rows": opts.rows, "cols": opts.cols }
    });
    if let Some(name) = &opts.name {
        msg["name"] = json!(name);
    }
    if let Some(agent_id) = &opts.agent_id {
        msg["agentId"] = json!(agent_id);
    }
    if let Some(command) = &opts.command {
        msg["command"] = json!(command);
    }
    if let Some(args) = &opts.args {
        msg["args"] = json!(args);
    }
    if let Some(workspace_id) = &opts.workspace_id {
        msg["workspaceId"] = json!(workspace_id);
    }
    msg
}

pub fn subscribe_terminal_request(
    request_id: &str,
    terminal_id: &str,
    restore_mode: &str,
) -> Value {
    json!({
        "type": "subscribe_terminal_request",
        "requestId": request_id,
        "terminalId": terminal_id,
        "restore": { "mode": restore_mode }
    })
}

pub fn unsubscribe_terminal_request(terminal_id: &str) -> Value {
    json!({
        "type": "unsubscribe_terminal_request",
        "terminalId": terminal_id
    })
}

pub fn kill_terminal_request(request_id: &str, terminal_id: &str) -> Value {
    json!({
        "type": "kill_terminal_request",
        "requestId": request_id,
        "terminalId": terminal_id
    })
}

pub fn parse_subscribe_slot(payload: &Value) -> Result<u8> {
    if let Some(error) = payload.get("error").and_then(Value::as_str) {
        return Err(PaseoError::Rpc(error.to_owned()));
    }
    let slot = payload
        .get("slot")
        .and_then(Value::as_u64)
        .ok_or_else(|| PaseoError::Protocol("subscribe_terminal_response missing slot".into()))?;
    if slot > 255 {
        return Err(PaseoError::Protocol("slot out of range".into()));
    }
    Ok(slot as u8)
}

pub fn parse_terminal_list(payload: &Value) -> Vec<TerminalInfo> {
    payload
        .get("terminals")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(|t| serde_json::from_value(t.clone()).ok())
                .collect()
        })
        .unwrap_or_default()
}
