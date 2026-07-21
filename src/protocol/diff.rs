use serde::Deserialize;
use serde_json::{json, Value};

#[derive(Clone, Debug, Deserialize)]
pub struct DiffLine {
    #[serde(default)]
    pub r#type: String,
    #[serde(default)]
    pub content: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffHunk {
    #[serde(default)]
    pub old_start: u32,
    #[serde(default)]
    pub old_count: u32,
    #[serde(default)]
    pub new_start: u32,
    #[serde(default)]
    pub new_count: u32,
    #[serde(default)]
    pub lines: Vec<DiffLine>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffFile {
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub is_new: bool,
    #[serde(default)]
    pub is_deleted: bool,
    #[serde(default)]
    pub additions: u32,
    #[serde(default)]
    pub deletions: u32,
    #[serde(default)]
    pub hunks: Vec<DiffHunk>,
    #[serde(default)]
    pub status: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckoutDiff {
    #[serde(default)]
    pub subscription_id: String,
    #[serde(default)]
    pub cwd: String,
    #[serde(default)]
    pub files: Vec<DiffFile>,
    #[serde(default)]
    pub error: Option<CheckoutError>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct CheckoutError {
    #[serde(default)]
    pub code: String,
    #[serde(default)]
    pub message: String,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckoutStatus {
    #[serde(default)]
    pub repo_root: Option<String>,
    #[serde(default)]
    pub current_branch: Option<String>,
    #[serde(default)]
    pub base_ref: Option<String>,
    #[serde(default)]
    pub is_dirty: Option<bool>,
}

pub fn subscribe_checkout_diff_request(
    request_id: &str,
    subscription_id: &str,
    cwd: &str,
    mode: &str,
    base_ref: Option<&str>,
) -> Value {
    let mut compare = json!({ "mode": mode });
    if let Some(base_ref) = base_ref {
        compare["baseRef"] = Value::from(base_ref);
    }
    json!({
        "type": "subscribe_checkout_diff_request",
        "subscriptionId": subscription_id,
        "cwd": cwd,
        "compare": compare,
        "requestId": request_id
    })
}

pub fn checkout_status_request(request_id: &str, cwd: &str) -> Value {
    json!({ "type": "checkout_status_request", "cwd": cwd, "requestId": request_id })
}

pub fn parse_checkout_status(payload: &Value) -> CheckoutStatus {
    serde_json::from_value(payload.clone()).unwrap_or_default()
}

pub fn unsubscribe_checkout_diff_request(subscription_id: &str) -> Value {
    json!({
        "type": "unsubscribe_checkout_diff_request",
        "subscriptionId": subscription_id
    })
}

pub fn parse_checkout_diff(payload: &Value) -> CheckoutDiff {
    serde_json::from_value(payload.clone()).unwrap_or_default()
}
