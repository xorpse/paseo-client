use serde_json::{json, Value};

pub fn capabilities() -> Value {
    json!({
        "custom_mode_icons": true,
        "reasoning_merge_enum": true,
        "terminal_reflowable_snapshot": true,
        "provider_subagents": true,
        "project_updates": true,
        "selective_agent_timeline": true
    })
}

pub fn hello_message(client_id: &str, app_version: Option<&str>) -> Value {
    let mut msg = json!({
        "type": "hello",
        "clientId": client_id,
        "clientType": "cli",
        "protocolVersion": 1,
        "capabilities": capabilities()
    });
    if let Some(version) = app_version {
        msg["appVersion"] = json!(version);
    }
    msg
}

pub fn ping_message() -> Value {
    json!({ "type": "ping" })
}

pub fn session_envelope(message: Value) -> Value {
    json!({ "type": "session", "message": message })
}

pub enum Incoming {
    Pong,
    Session(Value),
    Unknown(Value),
}

pub fn parse_top_level(text: &str) -> Result<Incoming, serde_json::Error> {
    let value: Value = serde_json::from_str(text)?;
    let ty = value.get("type").and_then(Value::as_str).unwrap_or("");
    Ok(match ty {
        "pong" => Incoming::Pong,
        "session" => Incoming::Session(value.get("message").cloned().unwrap_or(Value::Null)),
        _ => Incoming::Unknown(value),
    })
}

pub fn session_message_type(message: &Value) -> &str {
    message.get("type").and_then(Value::as_str).unwrap_or("")
}

pub fn session_request_id(message: &Value) -> Option<String> {
    message
        .get("payload")
        .and_then(|p| p.get("requestId"))
        .or_else(|| message.get("requestId"))
        .and_then(Value::as_str)
        .map(str::to_string)
}
