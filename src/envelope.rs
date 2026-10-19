use std::borrow::Cow;

use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;

use crate::error::HelloRejection;

#[derive(Serialize)]
struct Session<'a, B> {
    #[serde(rename = "type")]
    kind: &'static str,
    message: Tagged<'a, B>,
}

#[derive(Serialize)]
struct Tagged<'a, B> {
    #[serde(rename = "type")]
    kind: &'a str,
    #[serde(rename = "requestId", skip_serializing_if = "Option::is_none")]
    request_id: Option<&'a str>,
    #[serde(flatten)]
    body: &'a B,
}

pub(crate) fn session_message<B: Serialize>(
    kind: &str,
    request_id: Option<&str>,
    body: &B,
) -> Result<String, serde_json::Error> {
    serde_json::to_string(&Session {
        kind: "session",
        message: Tagged {
            kind,
            request_id,
            body,
        },
    })
}

#[derive(Serialize)]
pub(crate) struct NoFields {}

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub(crate) enum HelloAuth<'a> {
    Password { password: &'a str },
}

#[derive(Serialize)]
pub(crate) struct Capabilities {
    all_providers: bool,
    custom_mode_icons: bool,
    explicit_event_subscriptions: bool,
    hello_rejection: bool,
    owned_subscriptions: bool,
    project_updates: bool,
    projected_subagent_timeline: bool,
    provider_subagents: bool,
    reasoning_merge_enum: bool,
    selective_agent_timeline: bool,
    terminal_reflowable_snapshot: bool,
    timeline_notifications: bool,
    timeline_replacement_invalidation: bool,
    workspace_setup_blocked: bool,
}

impl Capabilities {
    const SUPPORTED: Capabilities = Capabilities {
        all_providers: true,
        custom_mode_icons: true,
        explicit_event_subscriptions: true,
        hello_rejection: true,
        owned_subscriptions: true,
        project_updates: true,
        projected_subagent_timeline: true,
        provider_subagents: true,
        reasoning_merge_enum: true,
        selective_agent_timeline: true,
        terminal_reflowable_snapshot: true,
        timeline_notifications: true,
        timeline_replacement_invalidation: true,
        workspace_setup_blocked: true,
    };
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Hello<'a> {
    #[serde(rename = "type")]
    kind: &'static str,
    client_id: &'a str,
    client_type: &'static str,
    protocol_version: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    auth: Option<HelloAuth<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    app_version: Option<&'a str>,
    capabilities: &'static Capabilities,
}

impl<'a> Hello<'a> {
    pub(crate) fn new(
        client_id: &'a str,
        password: Option<&'a str>,
        app_version: Option<&'a str>,
    ) -> Hello<'a> {
        Hello {
            kind: "hello",
            client_id,
            client_type: "cli",
            protocol_version: 1,
            auth: password.map(|password| HelloAuth::Password { password }),
            app_version,
            capabilities: &Capabilities::SUPPORTED,
        }
    }
}

pub(crate) const PING: &str = r#"{"type":"ping"}"#;

#[derive(Deserialize)]
pub(crate) struct Envelope<'a> {
    #[serde(rename = "type", borrow)]
    pub(crate) kind: Cow<'a, str>,
    #[serde(borrow, default)]
    pub(crate) message: Option<&'a RawValue>,
    #[serde(default)]
    pub(crate) reason: Option<HelloRejection>,
}

#[derive(Deserialize)]
pub(crate) struct MessageHead<'a> {
    #[serde(rename = "type", borrow)]
    pub(crate) kind: Cow<'a, str>,
    #[serde(borrow, default)]
    pub(crate) payload: Option<&'a RawValue>,
}

#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PayloadHead<'a> {
    #[serde(borrow, default)]
    pub(crate) request_id: Option<Cow<'a, str>>,
    #[serde(borrow, default)]
    pub(crate) subscription_id: Option<Cow<'a, str>>,
    #[serde(borrow, default)]
    pub(crate) status: Option<Cow<'a, str>>,
}
