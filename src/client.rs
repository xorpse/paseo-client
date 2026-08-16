use std::collections::HashMap;
use std::sync::Arc;

use futures::channel::oneshot;
use parking_lot::Mutex;
use serde_json::Value;

use crate::binary_frames::{self, encode_input, encode_resize, Opcode};
use crate::envelope::{
    hello_message, parse_top_level, session_envelope, session_message_type, session_request_id,
    Incoming,
};
use crate::error::{PaseoError, Result};
use crate::events::{DaemonEvent, TerminalStreamEvent};
use crate::offer::{build_relay_ws_url, decode_daemon_public_key, ConnectionOffer};
use crate::protocol::agents::{AgentListEntry, PermissionResponse};
use crate::protocol::stream::{AgentStreamEvent, AgentUpdate};
use crate::protocol::terminals::{self, CreateTerminalOpts, TerminalInfo};
use crate::protocol::{agents, diff, subagents, ServerInfo};
use crate::transport::{Frame, Transport, WsTransport};

fn new_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

struct Inner {
    transport: Arc<dyn Transport>,
    pending: Mutex<HashMap<String, oneshot::Sender<Result<Value>>>>,
    events_tx: async_broadcast::Sender<DaemonEvent>,
    _events_keep: async_broadcast::InactiveReceiver<DaemonEvent>,
    terminals: Mutex<HashMap<u8, flume::Sender<TerminalStreamEvent>>>,
    terminal_slots: Mutex<HashMap<String, u8>>,
    server_info: ServerInfo,
}

#[derive(Clone)]
pub struct PaseoClient {
    inner: Arc<Inner>,
}

#[derive(Clone, Debug)]
pub struct CreatedWorkspace {
    pub id: String,
    pub cwd: String,
}

enum FileWrite {
    Written,
    Conflict { modified_at: Option<String> },
}

pub struct TerminalHandle {
    terminal_id: String,
    slot: u8,
    transport: Arc<dyn Transport>,
    owner: PaseoClient,
    output_rx: flume::Receiver<TerminalStreamEvent>,
}

impl PaseoClient {
    pub async fn connect_relay(offer: &ConnectionOffer, client_id: &str) -> Result<PaseoClient> {
        let daemon_public_key = decode_daemon_public_key(offer)?;
        let url = build_relay_ws_url(&offer.relay.endpoint, offer.use_tls(), &offer.server_id)?;
        let transport = WsTransport::connect_relay(&url, daemon_public_key).await?;
        Self::finish_connect(transport, client_id).await
    }

    pub async fn connect_local(
        host_port: &str,
        use_tls: bool,
        password: Option<&str>,
        client_id: &str,
    ) -> Result<PaseoClient> {
        let transport = WsTransport::connect_local(host_port, use_tls, password).await?;
        Self::finish_connect(transport, client_id).await
    }

    async fn finish_connect(transport: Arc<WsTransport>, client_id: &str) -> Result<PaseoClient> {
        let transport: Arc<dyn Transport> = transport;
        transport
            .send_text(hello_message(client_id, None).to_string())
            .await?;

        let server_info = loop {
            match transport.recv().await? {
                None => return Err(PaseoError::Handshake("closed before server_info".into())),
                Some(Frame::Json(text)) => {
                    if let Some(info) = try_parse_server_info(&text) {
                        break info;
                    }
                }
                Some(Frame::Binary(_)) => continue,
            }
        };

        let (mut events_tx, events_rx) = async_broadcast::broadcast(1024);
        events_tx.set_overflow(true);

        let inner = Arc::new(Inner {
            transport,
            pending: Mutex::new(HashMap::new()),
            events_tx,
            _events_keep: events_rx.deactivate(),
            terminals: Mutex::new(HashMap::new()),
            terminal_slots: Mutex::new(HashMap::new()),
            server_info,
        });

        Ok(PaseoClient { inner })
    }

    pub fn server_info(&self) -> &ServerInfo {
        &self.inner.server_info
    }

    pub fn feature_enabled(&self, name: &str) -> bool {
        self.inner
            .server_info
            .features
            .get(name)
            .and_then(Value::as_bool)
            .unwrap_or(false)
    }

    pub fn events(&self) -> async_broadcast::Receiver<DaemonEvent> {
        self.inner.events_tx.new_receiver()
    }

    pub async fn run(&self) -> Result<()> {
        const OFFLOAD_JSON_BYTES: usize = 64 * 1024;
        let result = loop {
            match self.inner.transport.recv().await {
                Ok(Some(frame)) => match &frame {
                    Frame::Json(text) if text.len() > OFFLOAD_JSON_BYTES => {
                        let client = self.clone();
                        std::thread::spawn(move || client.dispatch(frame));
                    }
                    _ => self.dispatch(frame),
                },
                Ok(None) => break Ok(()),
                Err(err) => break Err(err),
            }
        };
        self.inner.terminals.lock().clear();
        self.inner.terminal_slots.lock().clear();
        self.emit(DaemonEvent::Disconnected);
        result
    }

    async fn request(&self, message: Value) -> Result<Value> {
        let request_id = message
            .get("requestId")
            .and_then(Value::as_str)
            .map(str::to_string)
            .ok_or_else(|| PaseoError::Protocol("request missing requestId".into()))?;

        let (tx, rx) = oneshot::channel();
        self.inner.pending.lock().insert(request_id.clone(), tx);

        let envelope = session_envelope(message).to_string();
        if let Err(err) = self.inner.transport.send_text(envelope).await {
            self.inner.pending.lock().remove(&request_id);
            return Err(err);
        }

        match rx.await {
            Ok(result) => result,
            Err(_) => Err(PaseoError::Closed),
        }
    }

    pub async fn fetch_agents(&self) -> Result<Vec<AgentListEntry>> {
        let id = new_id();
        let payload = self.request(agents::fetch_agents_request(&id)).await?;
        let entries = payload.get("entries").cloned().unwrap_or(Value::Null);
        Ok(serde_json::from_value(entries).unwrap_or_default())
    }

    pub async fn fetch_agents_raw(&self) -> Result<Vec<Value>> {
        let id = new_id();
        let payload = self.request(agents::fetch_agents_request(&id)).await?;
        let entries = payload
            .get("entries")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        Ok(entries)
    }

    pub async fn fetch_agent_timeline(
        &self,
        agent_id: &str,
        direction: &str,
        limit: u32,
    ) -> Result<Vec<crate::protocol::TimelineItem>> {
        let id = new_id();
        let payload = self
            .request(agents::fetch_agent_timeline_request(
                &id, agent_id, direction, limit,
            ))
            .await?;
        let items = payload
            .get("entries")
            .and_then(Value::as_array)
            .map(|entries| {
                entries
                    .iter()
                    .filter_map(|entry| entry.get("item").cloned())
                    .filter_map(|item| serde_json::from_value(item).ok())
                    .collect()
            })
            .unwrap_or_default();
        Ok(items)
    }

    pub async fn fetch_agent_timeline_page(
        &self,
        agent_id: &str,
        direction: &str,
        limit: u32,
        cursor: Option<Value>,
    ) -> Result<Value> {
        let id = new_id();
        self.request(agents::fetch_agent_timeline_page_request(
            &id,
            agent_id,
            direction,
            limit,
            cursor.as_ref(),
        ))
        .await
    }

    pub async fn list_provider_subagents(
        &self,
        parent_agent_id: &str,
    ) -> Result<Vec<crate::protocol::subagents::ProviderSubagent>> {
        let id = new_id();
        let payload = self
            .request(subagents::list_request(&id, parent_agent_id))
            .await?;
        Ok(subagents::parse_list(&payload))
    }

    pub async fn fetch_provider_subagent_timeline(
        &self,
        parent_agent_id: &str,
        subagent_id: &str,
        direction: &str,
        limit: u32,
    ) -> Result<Vec<crate::protocol::TimelineItem>> {
        let id = new_id();
        let payload = self
            .request(subagents::timeline_request(
                &id,
                parent_agent_id,
                subagent_id,
                direction,
                limit,
            ))
            .await?;
        Ok(subagents::parse_timeline(&payload))
    }

    pub async fn subscribe_agents(&self) -> Result<()> {
        let id = new_id();
        self.request(agents::subscribe_agents_request(&id, "wezterm-paseo"))
            .await?;
        Ok(())
    }

    pub async fn set_timeline_subscription(&self, agent_ids: &[String]) -> Result<()> {
        if !self.feature_enabled("selectiveAgentTimeline") {
            return Ok(());
        }
        let id = new_id();
        self.request(agents::set_timeline_subscription_request(&id, agent_ids))
            .await?;
        Ok(())
    }

    pub async fn subscribe_checkout_diff(
        &self,
        cwd: &str,
        mode: &str,
        base_ref: Option<&str>,
    ) -> Result<crate::protocol::diff::CheckoutDiff> {
        let id = new_id();
        let subscription_id = new_id();
        let payload = self
            .request(diff::subscribe_checkout_diff_request(
                &id,
                &subscription_id,
                cwd,
                mode,
                base_ref,
            ))
            .await?;
        Ok(diff::parse_checkout_diff(&payload))
    }

    pub async fn checkout_status(
        &self,
        cwd: &str,
    ) -> Result<crate::protocol::diff::CheckoutStatus> {
        let id = new_id();
        let payload = self
            .request(diff::checkout_status_request(&id, cwd))
            .await?;
        Ok(diff::parse_checkout_status(&payload))
    }

    pub async fn unsubscribe_checkout_diff(&self, subscription_id: &str) -> Result<()> {
        let message = diff::unsubscribe_checkout_diff_request(subscription_id);
        self.inner
            .transport
            .send_text(session_envelope(message).to_string())
            .await
    }

    pub async fn send_agent_message(&self, agent_id: &str, text: &str) -> Result<()> {
        let id = new_id();
        self.request(agents::send_agent_message_request(&id, agent_id, text))
            .await?;
        Ok(())
    }

    pub async fn fetch_agent(&self, agent_id: &str) -> Result<crate::protocol::AgentSnapshot> {
        let id = new_id();
        let payload = self
            .request(agents::fetch_agent_request(&id, agent_id))
            .await?;
        let agent = payload
            .get("agent")
            .cloned()
            .ok_or_else(|| PaseoError::Protocol("fetch_agent missing agent".into()))?;
        serde_json::from_value(agent).map_err(PaseoError::from)
    }

    pub async fn create_agent(
        &self,
        provider: &str,
        cwd: &str,
        workspace_id: Option<&str>,
        initial_prompt: Option<&str>,
        attachments: &[agents::TextAttachment],
    ) -> Result<crate::protocol::AgentSnapshot> {
        let id = new_id();
        let mut message = serde_json::json!({
            "type": "create_agent_request",
            "requestId": id,
            "config": { "provider": provider, "cwd": cwd },
            "labels": {}
        });
        if let Some(workspace_id) = workspace_id {
            message["workspaceId"] = Value::from(workspace_id);
        }
        if let Some(initial_prompt) = initial_prompt {
            message["initialPrompt"] = Value::from(initial_prompt);
        }
        if !attachments.is_empty() {
            message["attachments"] = serde_json::to_value(attachments).map_err(PaseoError::from)?;
        }
        let payload = self.request(message).await?;
        match payload.get("status").and_then(Value::as_str) {
            Some("agent_created") => {
                let agent = payload
                    .get("agent")
                    .cloned()
                    .ok_or_else(|| PaseoError::Protocol("agent_created missing agent".into()))?;
                serde_json::from_value(agent).map_err(PaseoError::from)
            }
            _ => Err(PaseoError::Rpc(
                payload
                    .get("error")
                    .and_then(Value::as_str)
                    .unwrap_or("create_agent failed")
                    .to_owned(),
            )),
        }
    }

    fn resumed_agent(payload: &Value) -> Result<crate::protocol::AgentSnapshot> {
        match payload.get("status").and_then(Value::as_str) {
            Some("agent_resumed") => {
                let agent = payload
                    .get("agent")
                    .cloned()
                    .ok_or_else(|| PaseoError::Protocol("agent_resumed missing agent".into()))?;
                serde_json::from_value(agent).map_err(PaseoError::from)
            }
            _ => Err(PaseoError::Rpc(
                payload
                    .get("error")
                    .and_then(Value::as_str)
                    .unwrap_or("resume failed")
                    .to_owned(),
            )),
        }
    }

    fn ensure_ok(payload: &Value) -> Result<()> {
        match payload.get("error").and_then(Value::as_str) {
            Some(error) if !error.is_empty() => Err(PaseoError::Rpc(error.to_owned())),
            _ => Ok(()),
        }
    }

    pub async fn resume_agent(
        &self,
        handle: &agents::AgentPersistenceHandle,
        overrides: Option<&Value>,
    ) -> Result<crate::protocol::AgentSnapshot> {
        let id = new_id();
        let payload = self
            .request(agents::resume_agent_request(&id, handle, overrides))
            .await?;
        Self::resumed_agent(&payload)
    }

    pub async fn import_agent(
        &self,
        request: &agents::ImportAgentRequest,
    ) -> Result<crate::protocol::AgentSnapshot> {
        let id = new_id();
        let payload = self
            .request(agents::import_agent_request(&id, request))
            .await?;
        Self::resumed_agent(&payload)
    }

    pub async fn refresh_agent(&self, agent_id: &str) -> Result<()> {
        let id = new_id();
        let payload = self
            .request(agents::refresh_agent_request(&id, agent_id))
            .await?;
        Self::ensure_ok(&payload)
    }

    pub async fn update_agent(
        &self,
        agent_id: &str,
        name: Option<&str>,
        labels: Option<&HashMap<String, String>>,
    ) -> Result<()> {
        let id = new_id();
        let payload = self
            .request(agents::update_agent_request(&id, agent_id, name, labels))
            .await?;
        Self::ensure_ok(&payload)
    }

    pub async fn detach_agent(&self, agent_id: &str) -> Result<()> {
        let id = new_id();
        let payload = self
            .request(agents::detach_agent_request(&id, agent_id))
            .await?;
        Self::ensure_ok(&payload)
    }

    pub async fn rewind_agent(
        &self,
        agent_id: &str,
        message_id: &str,
        mode: agents::RewindMode,
    ) -> Result<()> {
        let id = new_id();
        let payload = self
            .request(agents::rewind_agent_request(&id, agent_id, message_id, mode))
            .await?;
        Self::ensure_ok(&payload)
    }

    pub async fn wait_for_finish(
        &self,
        agent_id: &str,
        timeout_ms: Option<u32>,
    ) -> Result<agents::WaitOutcome> {
        let id = new_id();
        let payload = self
            .request(agents::wait_for_finish_request(&id, agent_id, timeout_ms))
            .await?;
        serde_json::from_value(payload).map_err(PaseoError::from)
    }

    pub async fn list_prompts(&self, agent_id: &str) -> Result<Vec<agents::PromptPreview>> {
        let id = new_id();
        let payload = self
            .request(agents::list_prompts_request(&id, agent_id))
            .await?;
        Self::ensure_ok(&payload)?;
        let prompts = payload
            .get("prompts")
            .cloned()
            .map(serde_json::from_value)
            .transpose()?
            .unwrap_or_default();
        Ok(prompts)
    }

    pub async fn fork_context(
        &self,
        agent_id: &str,
        boundary_message_id: Option<&str>,
    ) -> Result<agents::ForkedContext> {
        let id = new_id();
        let payload = self
            .request(agents::fork_context_request(&id, agent_id, boundary_message_id))
            .await?;
        Self::ensure_ok(&payload)?;
        serde_json::from_value(payload).map_err(PaseoError::from)
    }

    pub async fn list_available_providers(&self) -> Result<Vec<String>> {
        let id = new_id();
        let payload = self
            .request(serde_json::json!({
                "type": "list_available_providers_request",
                "requestId": id
            }))
            .await?;
        let providers = payload
            .get("providers")
            .and_then(Value::as_array)
            .map(|entries| {
                entries
                    .iter()
                    .filter(|entry| {
                        entry
                            .get("available")
                            .and_then(Value::as_bool)
                            .unwrap_or(false)
                    })
                    .filter_map(|entry| {
                        entry
                            .get("provider")
                            .and_then(Value::as_str)
                            .map(String::from)
                    })
                    .collect()
            })
            .unwrap_or_default();
        Ok(providers)
    }

    pub async fn cancel_agent(&self, agent_id: &str) -> Result<()> {
        let id = new_id();
        self.request(agents::cancel_agent_request(&id, agent_id))
            .await?;
        Ok(())
    }

    pub async fn archive_agent(&self, agent_id: &str) -> Result<()> {
        let id = new_id();
        self.request(agents::archive_agent_request(&id, agent_id))
            .await?;
        Ok(())
    }

    pub async fn delete_agent(&self, agent_id: &str) -> Result<()> {
        let id = new_id();
        self.request(agents::delete_agent_request(&id, agent_id))
            .await?;
        Ok(())
    }

    pub async fn set_agent_mode(&self, agent_id: &str, mode_id: &str) -> Result<()> {
        let id = new_id();
        self.request(agents::set_agent_mode_request(&id, agent_id, mode_id))
            .await?;
        Ok(())
    }

    pub async fn set_agent_model(&self, agent_id: &str, model_id: &str) -> Result<()> {
        let id = new_id();
        self.request(agents::set_agent_model_request(&id, agent_id, model_id))
            .await?;
        Ok(())
    }

    pub async fn set_agent_feature(
        &self,
        agent_id: &str,
        feature_id: &str,
        value: Value,
    ) -> Result<()> {
        let id = new_id();
        self.request(agents::set_agent_feature_request(
            &id, agent_id, feature_id, value,
        ))
        .await?;
        Ok(())
    }

    pub async fn list_commands(&self, agent_id: &str) -> Result<Vec<agents::SlashCommand>> {
        let id = new_id();
        let payload = self
            .request(agents::list_commands_request(&id, agent_id))
            .await?;
        let commands = payload
            .get("commands")
            .cloned()
            .map(serde_json::from_value)
            .transpose()?
            .unwrap_or_default();
        Ok(commands)
    }

    pub async fn set_agent_thinking(&self, agent_id: &str, thinking_option_id: &str) -> Result<()> {
        let id = new_id();
        self.request(agents::set_agent_thinking_request(
            &id,
            agent_id,
            thinking_option_id,
        ))
        .await?;
        Ok(())
    }

    pub async fn list_provider_models(
        &self,
        provider: &str,
        cwd: Option<&str>,
    ) -> Result<Vec<crate::protocol::agents::ModelDefinition>> {
        let id = new_id();
        let payload = self
            .request(agents::list_provider_models_request(&id, provider, cwd))
            .await?;
        let models = payload.get("models").cloned().unwrap_or(Value::Null);
        Ok(serde_json::from_value(models).unwrap_or_default())
    }

    pub async fn respond_permission(
        &self,
        agent_id: &str,
        request_id: &str,
        response: PermissionResponse,
    ) -> Result<()> {
        let message = agents::permission_response_message(agent_id, request_id, &response);
        self.inner
            .transport
            .send_text(session_envelope(message).to_string())
            .await
    }

    pub async fn fetch_workspaces(&self) -> Result<Vec<crate::protocol::workspaces::Workspace>> {
        let id = new_id();
        let payload = self
            .request(crate::protocol::workspaces::fetch_workspaces_request(&id))
            .await?;
        let entries = payload.get("entries").cloned().unwrap_or(Value::Null);
        Ok(serde_json::from_value(entries).unwrap_or_default())
    }

    pub async fn project_add(&self, cwd: &str) -> Result<()> {
        let id = new_id();
        self.request(crate::protocol::workspaces::project_add_request(&id, cwd))
            .await?;
        Ok(())
    }

    pub async fn project_create_directory(&self, parent_path: &str, name: &str) -> Result<String> {
        let id = new_id();
        let payload = self
            .request(
                crate::protocol::workspaces::project_create_directory_request(
                    &id,
                    parent_path,
                    name,
                ),
            )
            .await?;
        if let Some(error) = payload.get("error").and_then(Value::as_str) {
            return Err(PaseoError::Rpc(error.to_owned()));
        }
        payload
            .get("directoryPath")
            .and_then(Value::as_str)
            .map(str::to_string)
            .ok_or_else(|| PaseoError::Protocol("create_directory missing directoryPath".into()))
    }

    pub async fn write_text_file(
        &self,
        cwd: &str,
        parent_path: &str,
        name: &str,
        content: &str,
    ) -> Result<String> {
        if parent_path != "." && !parent_path.is_empty() {
            let _ = self.fs_entry_create(cwd, ".", parent_path, "directory").await;
        }
        let path = self.fs_entry_create(cwd, parent_path, name, "file").await?;
        let expected = match self.fs_file_write(cwd, &path, content, "").await? {
            FileWrite::Written => return Ok(path),
            FileWrite::Conflict { modified_at } => modified_at.ok_or_else(|| {
                PaseoError::Protocol("file write conflict missing modifiedAt".into())
            })?,
        };
        match self.fs_file_write(cwd, &path, content, &expected).await? {
            FileWrite::Written => Ok(path),
            FileWrite::Conflict { .. } => Err(PaseoError::Rpc("file write conflict".into())),
        }
    }

    async fn fs_entry_create(
        &self,
        cwd: &str,
        parent_path: &str,
        name: &str,
        kind: &str,
    ) -> Result<String> {
        let id = new_id();
        let payload = self
            .request(serde_json::json!({
                "type": "fs.entry.create.request",
                "cwd": cwd,
                "parentPath": parent_path,
                "name": name,
                "kind": kind,
                "requestId": id,
            }))
            .await?;
        if let Some(error) = payload.get("error").and_then(Value::as_str) {
            return Err(PaseoError::Rpc(error.to_owned()));
        }
        payload
            .get("path")
            .and_then(Value::as_str)
            .map(str::to_string)
            .ok_or_else(|| PaseoError::Protocol("entry.create missing path".into()))
    }

    async fn fs_file_write(
        &self,
        cwd: &str,
        path: &str,
        content: &str,
        expected_modified_at: &str,
    ) -> Result<FileWrite> {
        let id = new_id();
        let payload = self
            .request(serde_json::json!({
                "type": "fs.file.write.request",
                "cwd": cwd,
                "path": path,
                "content": content,
                "expectedModifiedAt": expected_modified_at,
                "requestId": id,
            }))
            .await?;
        let result = payload.get("result").cloned().unwrap_or(Value::Null);
        match result.get("status").and_then(Value::as_str) {
            Some("written") => Ok(FileWrite::Written),
            Some("conflict") => Ok(FileWrite::Conflict {
                modified_at: result
                    .get("version")
                    .and_then(|version| version.get("modifiedAt"))
                    .and_then(Value::as_str)
                    .map(str::to_string),
            }),
            Some("error") => Err(PaseoError::Rpc(
                result
                    .get("error")
                    .and_then(Value::as_str)
                    .unwrap_or("file write error")
                    .to_owned(),
            )),
            _ => Err(PaseoError::Protocol("file write missing status".into())),
        }
    }

    pub async fn directory_suggestions(&self, query: &str, limit: u32) -> Result<Vec<String>> {
        let id = new_id();
        let payload = self
            .request(crate::protocol::workspaces::directory_suggestions_request(
                &id, query, limit,
            ))
            .await?;
        if let Some(error) = payload.get("error").and_then(Value::as_str) {
            return Err(PaseoError::Rpc(error.to_owned()));
        }
        let directories = payload
            .get("directories")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| item.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default();
        Ok(directories)
    }

    pub async fn branch_suggestions(
        &self,
        cwd: &str,
        query: &str,
        limit: u32,
    ) -> Result<Vec<String>> {
        let id = new_id();
        let payload = self
            .request(crate::protocol::workspaces::branch_suggestions_request(
                &id, cwd, query, limit,
            ))
            .await?;
        if let Some(error) = payload.get("error").and_then(Value::as_str) {
            return Err(PaseoError::Rpc(error.to_owned()));
        }
        let branches = payload
            .get("branches")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| item.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default();
        Ok(branches)
    }

    async fn workspace_create(&self, request: Value) -> Result<CreatedWorkspace> {
        let payload = self.request(request).await?;
        if let Some(error) = payload.get("error").and_then(Value::as_str) {
            return Err(PaseoError::Rpc(error.to_owned()));
        }
        let workspace = payload.get("workspace").filter(|value| !value.is_null());
        let workspace = workspace
            .ok_or_else(|| PaseoError::Protocol("workspace.create returned no workspace".into()))?;
        let id = workspace
            .get("id")
            .and_then(Value::as_str)
            .map(str::to_string)
            .ok_or_else(|| PaseoError::Protocol("workspace.create missing id".into()))?;
        let cwd = workspace
            .get("workspaceDirectory")
            .and_then(Value::as_str)
            .or_else(|| workspace.get("projectRootPath").and_then(Value::as_str))
            .map(str::to_string)
            .ok_or_else(|| PaseoError::Protocol("workspace.create missing directory".into()))?;
        Ok(CreatedWorkspace { id, cwd })
    }

    pub async fn workspace_create_directory(&self, path: &str) -> Result<CreatedWorkspace> {
        let id = new_id();
        self.workspace_create(
            crate::protocol::workspaces::workspace_create_directory_request(&id, path),
        )
        .await
    }

    pub async fn workspace_create_worktree(
        &self,
        cwd: &str,
        action: &str,
        ref_name: Option<&str>,
        base_branch: Option<&str>,
        branch_name: Option<&str>,
    ) -> Result<CreatedWorkspace> {
        let id = new_id();
        self.workspace_create(
            crate::protocol::workspaces::workspace_create_worktree_request(
                &id,
                cwd,
                action,
                ref_name,
                base_branch,
                branch_name,
            ),
        )
        .await
    }

    pub async fn project_github_clone(
        &self,
        repo: &str,
        protocol: &str,
        target_directory: &str,
    ) -> Result<String> {
        let id = new_id();
        let payload = self
            .request(crate::protocol::workspaces::project_github_clone_request(
                &id,
                repo,
                protocol,
                target_directory,
            ))
            .await?;
        if let Some(error) = payload.get("error").and_then(Value::as_str) {
            return Err(PaseoError::Rpc(error.to_owned()));
        }
        payload
            .get("project")
            .and_then(|p| p.get("projectRootPath"))
            .and_then(Value::as_str)
            .map(str::to_string)
            .ok_or_else(|| {
                PaseoError::Protocol("github clone missing project.projectRootPath".into())
            })
    }

    pub async fn open_project(&self, cwd: &str) -> Result<String> {
        let id = new_id();
        let payload = self
            .request(serde_json::json!({
                "type": "open_project_request",
                "cwd": cwd,
                "requestId": id
            }))
            .await?;
        if let Some(err) = payload.get("error").and_then(Value::as_str) {
            return Err(PaseoError::Rpc(err.to_owned()));
        }
        payload
            .get("workspace")
            .and_then(|workspace| workspace.get("id"))
            .and_then(Value::as_str)
            .map(str::to_string)
            .ok_or_else(|| {
                PaseoError::Protocol("open_project_response missing workspace.id".into())
            })
    }

    pub async fn list_terminals(&self, cwd: Option<&str>) -> Result<Vec<TerminalInfo>> {
        let id = new_id();
        let payload = self
            .request(terminals::list_terminals_request(&id, cwd))
            .await?;
        Ok(terminals::parse_terminal_list(&payload))
    }

    pub async fn create_terminal(
        &self,
        cwd: &str,
        opts: CreateTerminalOpts,
    ) -> Result<TerminalInfo> {
        let id = new_id();
        let payload = self
            .request(terminals::create_terminal_request(&id, cwd, &opts))
            .await?;
        if let Some(err) = payload.get("error").and_then(Value::as_str) {
            return Err(PaseoError::Rpc(err.to_owned()));
        }
        let terminal = payload.get("terminal").cloned().ok_or_else(|| {
            PaseoError::Protocol("create_terminal_response missing terminal".into())
        })?;
        serde_json::from_value(terminal).map_err(PaseoError::from)
    }

    pub async fn subscribe_terminal(
        &self,
        terminal_id: &str,
        restore_mode: &str,
    ) -> Result<TerminalHandle> {
        let id = new_id();
        let payload = self
            .request(terminals::subscribe_terminal_request(
                &id,
                terminal_id,
                restore_mode,
            ))
            .await?;
        let slot = terminals::parse_subscribe_slot(&payload)?;

        let (tx, rx) = flume::unbounded();
        self.inner.terminals.lock().insert(slot, tx);
        self.inner
            .terminal_slots
            .lock()
            .insert(terminal_id.to_owned(), slot);

        Ok(TerminalHandle {
            terminal_id: terminal_id.to_owned(),
            slot,
            transport: self.inner.transport.clone(),
            owner: self.clone(),
            output_rx: rx,
        })
    }

    pub async fn kill_terminal(&self, terminal_id: &str) -> Result<()> {
        let id = new_id();
        self.request(terminals::kill_terminal_request(&id, terminal_id))
            .await?;
        Ok(())
    }

    pub async fn close(&self) {
        self.inner.transport.close().await;
    }

    fn dispatch(&self, frame: Frame) {
        match frame {
            Frame::Json(text) => match parse_top_level(&text) {
                Ok(Incoming::Session(message)) => self.dispatch_session(message),
                Ok(Incoming::Pong) | Ok(Incoming::Unknown(_)) => {}
                Err(_) => {}
            },
            Frame::Binary(bytes) => self.dispatch_binary(&bytes),
        }
    }

    fn dispatch_session(&self, message: Value) {
        let ty = session_message_type(&message).to_owned();
        let payload = message.get("payload").cloned().unwrap_or(Value::Null);

        if let Some(request_id) = session_request_id(&message) {
            let waiter = self.inner.pending.lock().remove(&request_id);
            if let Some(tx) = waiter {
                if ty == "rpc_error" {
                    let err = payload
                        .get("error")
                        .and_then(Value::as_str)
                        .unwrap_or("rpc error")
                        .to_owned();
                    let _ = tx.send(Err(PaseoError::Rpc(err)));
                } else {
                    let _ = tx.send(Ok(payload));
                }
                return;
            }
        }

        match ty.as_str() {
            "agent_update" => {
                if let Ok(update) = serde_json::from_value::<AgentUpdate>(payload) {
                    match update.kind.as_str() {
                        "remove" => {
                            if let Some(id) = update.agent_id {
                                self.emit(DaemonEvent::AgentRemove(id));
                            }
                        }
                        _ => {
                            if let Some(agent) = update.agent {
                                self.emit(DaemonEvent::AgentUpsert(Box::new(agent)));
                            }
                        }
                    }
                }
            }
            "agent_stream" => {
                let agent_id = payload
                    .get("agentId")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned();
                if let Some(event) = payload.get("event").cloned() {
                    if let Ok(event) = serde_json::from_value::<AgentStreamEvent>(event) {
                        self.emit(DaemonEvent::AgentStream {
                            agent_id,
                            event: Box::new(event),
                        });
                    }
                }
            }
            "agent_permission_request" => {
                let agent_id = payload
                    .get("agentId")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned();
                if let Some(request) = payload.get("request").cloned() {
                    if let Ok(request) = serde_json::from_value(request) {
                        self.emit(DaemonEvent::PermissionRequest {
                            agent_id,
                            request: Box::new(request),
                        });
                    }
                }
            }
            "terminals_changed" => {
                let terminals = terminals::parse_terminal_list(&payload);
                self.emit(DaemonEvent::TerminalsChanged { terminals });
            }
            "terminal_stream_exit" => {
                if let Some(terminal_id) = payload.get("terminalId").and_then(Value::as_str) {
                    if let Some(slot) = self.inner.terminal_slots.lock().remove(terminal_id) {
                        self.inner.terminals.lock().remove(&slot);
                    }
                    self.emit(DaemonEvent::TerminalExit(terminal_id.to_owned()));
                }
            }
            "agent.provider_subagents.update" => {
                if let Some(update) = subagents::parse_update(&payload) {
                    self.emit(DaemonEvent::Subagent(Box::new(update)));
                }
            }
            "checkout_diff_update" => {
                let diff = diff::parse_checkout_diff(&payload);
                self.emit(DaemonEvent::CheckoutDiff(Box::new(diff)));
            }
            _ => {}
        }
    }

    fn dispatch_binary(&self, bytes: &[u8]) {
        let Some(frame) = binary_frames::decode(bytes) else {
            return;
        };
        let senders = self.inner.terminals.lock();
        let Some(tx) = senders.get(&frame.slot) else {
            return;
        };
        let event = match frame.opcode {
            Opcode::Output => TerminalStreamEvent::Output(frame.payload),
            Opcode::Restore => TerminalStreamEvent::Restore(frame.payload),
            Opcode::Snapshot => TerminalStreamEvent::Snapshot(
                serde_json::from_slice(&frame.payload).unwrap_or(Value::Null),
            ),
            Opcode::Input | Opcode::Resize => return,
        };
        let _ = tx.send(event);
    }

    fn emit(&self, event: DaemonEvent) {
        let _ = self.inner.events_tx.try_broadcast(event);
    }
}

impl TerminalHandle {
    pub fn terminal_id(&self) -> &str {
        &self.terminal_id
    }

    pub fn output(&self) -> flume::Receiver<TerminalStreamEvent> {
        self.output_rx.clone()
    }

    pub fn writer(&self) -> TerminalWriter {
        TerminalWriter {
            transport: self.transport.clone(),
            slot: self.slot,
        }
    }

    pub async fn send_input(&self, data: &[u8]) -> Result<()> {
        self.writer().send_input(data).await
    }

    pub async fn resize(&self, rows: u32, cols: u32) -> Result<()> {
        self.writer().resize(rows, cols).await
    }

    pub async fn unsubscribe(self) -> Result<()> {
        self.owner.inner.terminals.lock().remove(&self.slot);
        let message = terminals::unsubscribe_terminal_request(&self.terminal_id);
        self.transport
            .send_text(session_envelope(message).to_string())
            .await
    }
}

#[derive(Clone)]
pub struct TerminalWriter {
    transport: Arc<dyn Transport>,
    slot: u8,
}

impl TerminalWriter {
    pub async fn send_input(&self, data: &[u8]) -> Result<()> {
        self.transport
            .send_binary(encode_input(self.slot, data))
            .await
    }

    pub async fn resize(&self, rows: u32, cols: u32) -> Result<()> {
        self.transport
            .send_binary(encode_resize(self.slot, rows, cols)?)
            .await
    }
}

fn try_parse_server_info(text: &str) -> Option<ServerInfo> {
    let Incoming::Session(message) = parse_top_level(text).ok()? else {
        return None;
    };
    if session_message_type(&message) != "status" {
        return None;
    }
    let payload = message.get("payload")?;
    if payload.get("status").and_then(Value::as_str) != Some("server_info") {
        return None;
    }
    serde_json::from_value(payload.clone()).ok()
}
