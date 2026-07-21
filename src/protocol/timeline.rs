use serde::Deserialize;

#[derive(Clone, Debug, Deserialize)]
pub struct ToolCallDetail {
    #[serde(rename = "type", default)]
    pub kind: String,
    #[serde(default)]
    pub command: Option<String>,
    #[serde(default)]
    pub cwd: Option<String>,
    #[serde(default)]
    pub output: Option<String>,
    #[serde(rename = "exitCode", default)]
    pub exit_code: Option<i64>,
    #[serde(rename = "unifiedDiff", default)]
    pub unified_diff: Option<String>,
    #[serde(rename = "filePath", default)]
    pub file_path: Option<String>,
    #[serde(default)]
    pub content: Option<String>,
    #[serde(default)]
    pub query: Option<String>,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub text: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct TimelineItem {
    #[serde(rename = "type", default)]
    pub kind: String,
    #[serde(default)]
    pub text: Option<String>,
    #[serde(rename = "messageId", default)]
    pub message_id: Option<String>,
    #[serde(rename = "callId", default)]
    pub call_id: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub detail: Option<ToolCallDetail>,
    #[serde(default)]
    pub status: Option<String>,
}
