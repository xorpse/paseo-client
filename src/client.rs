use std::borrow::Cow;
use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::future::Future;
use std::pin::pin;
use std::sync::{Arc, Weak};
use std::time::Duration;

use async_io::Timer;
use async_tungstenite::tungstenite::Message;
use futures::channel::oneshot;
use futures::future::{self, Either};
use futures::{SinkExt, StreamExt};
use futures_rustls::rustls::ClientConfig;
use parking_lot::Mutex;
use serde::de::Error as _;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use serde_json::value::RawValue;

use crate::binary_frames::{
    ChannelFrame, FileTransferFrame, HostRouteFrame, Opcode, ResizeIntent, ResizePayload,
    TerminalFrame,
};
use crate::channel::ChannelEndpoint;
use crate::e2ee::SecurityMode;
use crate::endpoint::ConnectTarget;
use crate::envelope::{
    Envelope, Hello, HelloAuth, MessageHead, PING, PayloadHead, session_message,
};
use crate::error::{HostRouteClosed, PaseoError, ProtocolViolation, RpcError};
use crate::protocol::agents::CreationSnapshot;
use crate::protocol::daemon::{DaemonFeature, ServerInfo};
use crate::protocol::files::{FileContent, TransferMetadata};
use crate::protocol::terminals::{TerminalSize, TerminalStreamEvent};
use crate::transport::{Connection, Frame, RouteEvent, Socket, Wire};

pub(crate) const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
pub(crate) const REQUEST_TIMEOUT: Duration = Duration::from_secs(60);
const LIVENESS_INTERVAL: Duration = Duration::from_secs(10);
const LIVENESS_TIMEOUT: Duration = Duration::from_secs(15);
const LIVENESS_FAILURE_THRESHOLD: u32 = 2;

pub(crate) async fn timed<F: Future>(future: F, timeout: Duration) -> Option<F::Output> {
    match future::select(pin!(future), Timer::after(timeout)).await {
        Either::Left((output, _)) => Some(output),
        Either::Right(_) => None,
    }
}

pub(crate) fn new_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

#[derive(Clone)]
pub struct ConnectOptions {
    client_id: String,
    tls: Arc<ClientConfig>,
    app_version: Option<String>,
    outdated_daemons: bool,
}

impl ConnectOptions {
    pub fn new(client_id: impl Into<String>, tls: Arc<ClientConfig>) -> ConnectOptions {
        ConnectOptions {
            client_id: client_id.into(),
            tls,
            app_version: None,
            outdated_daemons: false,
        }
    }

    pub fn client_id(&self) -> &str {
        &self.client_id
    }

    pub fn app_version(&self) -> Option<&str> {
        self.app_version.as_deref()
    }

    pub fn set_app_version(&mut self, app_version: Option<String>) {
        self.app_version = app_version;
    }

    pub fn with_app_version(mut self, app_version: impl Into<String>) -> ConnectOptions {
        self.set_app_version(Some(app_version.into()));
        self
    }

    pub fn outdated_daemons(&self) -> bool {
        self.outdated_daemons
    }

    /// Connects to daemons that lack the subscription features the client is built on, so
    /// they can be upgraded. Only plain requests are reliable on such a connection.
    pub fn with_outdated_daemons(mut self) -> ConnectOptions {
        self.outdated_daemons = true;
        self
    }
}

pub(crate) type Updates<U> = flume::Sender<Result<U, PaseoError>>;

pub(crate) trait Routed: Sized + Send + 'static {
    fn decode(kind: &str, message: &RawValue) -> Result<Self, PaseoError>;
}

pub(crate) type Deliver = Arc<dyn Fn(&str, &RawValue) + Send + Sync>;

struct Route {
    deliver: Deliver,
    terminal_slot: Option<u8>,
    channel: Option<u32>,
    host_route: Option<flume::Sender<RouteEvent>>,
}

struct SlotOwner {
    subscription_id: String,
    events: Updates<TerminalStreamEvent>,
}

type Reply = Result<Box<RawValue>, PaseoError>;
type Registration =
    Box<dyn FnOnce(&mut State, &PayloadHead<'_>, &RawValue) -> Result<(), PaseoError> + Send>;

struct Pending {
    response_type: &'static str,
    request_type: &'static str,
    reply: oneshot::Sender<Reply>,
    registration: Option<Registration>,
    tracking: Option<Tracking>,
}

pub(crate) enum Tracking {
    Download(Download),
    Progress {
        kind: &'static str,
        deliver: Deliver,
    },
}

pub(crate) struct Download {
    path: String,
    metadata: Option<TransferMetadata>,
    bytes: Vec<u8>,
    finished: oneshot::Sender<Result<FileContent, PaseoError>>,
}

impl Download {
    const MAX_RESERVED_BYTES: u64 = 16 * 1024 * 1024;

    pub(crate) fn new(
        path: &str,
        finished: oneshot::Sender<Result<FileContent, PaseoError>>,
    ) -> Download {
        Download {
            path: path.to_owned(),
            metadata: None,
            bytes: Vec::new(),
            finished,
        }
    }

    fn begin(&mut self, metadata: TransferMetadata) {
        let reserved = metadata.size().min(Download::MAX_RESERVED_BYTES);
        self.bytes = Vec::with_capacity(
            usize::try_from(reserved).expect("the reservation is capped at 16 MiB"),
        );
        self.metadata = Some(metadata);
    }

    fn append(&mut self, payload: &[u8]) {
        self.bytes.extend_from_slice(payload);
    }

    fn finish(self) {
        let Download {
            path,
            metadata,
            bytes,
            finished,
        } = self;
        let received = u64::try_from(bytes.len()).expect("a download's length fits in 64 bits");
        let content = match metadata {
            None => Err(ProtocolViolation::FileTransferWithoutBegin.into()),
            Some(metadata) if metadata.size() != received => {
                Err(ProtocolViolation::FileTransferIncomplete {
                    expected: metadata.size(),
                    received,
                }
                .into())
            }
            Some(metadata) => Ok(FileContent::streamed(&path, metadata, bytes)),
        };
        finished.send(content).ok();
    }

    fn fail(self, error: PaseoError) {
        self.finished.send(Err(error)).ok();
    }
}

#[derive(Default)]
pub(crate) struct State {
    connected: bool,
    pending: HashMap<String, Pending>,
    routes: HashMap<String, Route>,
    terminal_slots: HashMap<u8, SlotOwner>,
    channels: HashMap<u32, Arc<dyn ChannelEndpoint>>,
    creations: HashMap<String, Updates<CreationSnapshot>>,
}

struct Driver {
    socket: Socket,
    wire: Wire,
    outgoing: flume::Receiver<Outgoing>,
    pongs: flume::Receiver<()>,
}

pub(crate) enum Outgoing {
    Close,
    Frame(Frame),
}

pub(crate) struct Inner {
    server_info: ServerInfo,
    security_mode: Option<SecurityMode>,
    outgoing: flume::Sender<Outgoing>,
    pongs: flume::Sender<()>,
    state: Mutex<State>,
    driver: Mutex<Option<Driver>>,
}

const REQUIRED_FEATURES: [DaemonFeature; 2] = [
    DaemonFeature::OwnedSubscriptions,
    DaemonFeature::ExplicitEventSubscriptions,
];

#[derive(Clone)]
pub struct PaseoClient {
    pub(crate) inner: Arc<Inner>,
}

impl PaseoClient {
    /// Whether the daemon lacks features the client is built on, which only a connection made
    /// with [`ConnectOptions::with_outdated_daemons`] allows.
    pub fn is_outdated(&self) -> bool {
        REQUIRED_FEATURES
            .iter()
            .any(|feature| !self.server_info().supports(*feature))
    }

    pub async fn connect(
        target: &ConnectTarget,
        options: &ConnectOptions,
    ) -> Result<PaseoClient, PaseoError> {
        let connect = async {
            let connection = Connection::open(target.endpoint(), &options.tls).await?;
            Self::handshake(connection, target.hello_auth(), options).await
        };
        timed(connect, CONNECT_TIMEOUT)
            .await
            .ok_or(PaseoError::Timeout {
                request_type: "connect",
            })?
    }

    pub(crate) async fn handshake(
        connection: Connection,
        auth: Option<HelloAuth<'_>>,
        options: &ConnectOptions,
    ) -> Result<PaseoClient, PaseoError> {
        let Connection { mut socket, wire } = connection;
        let hello = Hello::new(&options.client_id, auth, options.app_version.as_deref());
        socket
            .send(wire.encode(Frame::Text(serde_json::to_string(&hello)?))?)
            .await?;
        let server_info = Self::await_server_info(&mut socket, &wire).await?;
        let security_mode = wire.security_mode();
        if security_mode == Some(SecurityMode::Legacy)
            && server_info.supports(DaemonFeature::RelayProtectedSecurity)
        {
            return Err(ProtocolViolation::LegacySecurityRefused.into());
        }
        for feature in REQUIRED_FEATURES {
            if !server_info.supports(feature) && !options.outdated_daemons {
                return Err(PaseoError::UnsupportedDaemon {
                    feature: feature.wire_name(),
                });
            }
        }
        let (outgoing, outgoing_rx) = flume::unbounded();
        let (pongs, pongs_rx) = flume::bounded(1);
        Ok(PaseoClient {
            inner: Arc::new(Inner {
                server_info,
                security_mode,
                outgoing,
                pongs,
                state: Mutex::new(State {
                    connected: true,
                    ..State::default()
                }),
                driver: Mutex::new(Some(Driver {
                    socket,
                    wire,
                    outgoing: outgoing_rx,
                    pongs: pongs_rx,
                })),
            }),
        })
    }

    async fn await_server_info(socket: &mut Socket, wire: &Wire) -> Result<ServerInfo, PaseoError> {
        while let Some(message) = socket.next().await {
            let message = message?;
            if let Message::Close(_) = message {
                break;
            }
            let Some(Frame::Text(text)) = wire.decode(message)? else {
                continue;
            };
            let envelope = serde_json::from_str::<Envelope<'_>>(&text)?;
            if envelope.kind == "hello.rejected" {
                let reason = envelope.reason.ok_or_else(|| {
                    PaseoError::decode("hello.rejected", serde_json::Error::missing_field("reason"))
                })?;
                return Err(PaseoError::HelloRejected(reason));
            }
            let Some(message) = envelope.message.filter(|_| envelope.kind == "session") else {
                continue;
            };
            let head = serde_json::from_str::<MessageHead<'_>>(message.get())?;
            let Some(payload) = head.payload.filter(|_| head.kind == "status") else {
                continue;
            };
            let status = serde_json::from_str::<PayloadHead<'_>>(payload.get())?;
            if status.status.as_deref() == Some("server_info") {
                return serde_json::from_str::<ServerInfo>(payload.get())
                    .map_err(|error| PaseoError::decode("server_info", error));
            }
        }
        Err(ProtocolViolation::ClosedDuringHandshake.into())
    }

    pub fn server_info(&self) -> &ServerInfo {
        &self.inner.server_info
    }

    pub fn security_mode(&self) -> Option<SecurityMode> {
        self.inner.security_mode
    }

    pub fn is_connected(&self) -> bool {
        self.inner.state.lock().connected
    }

    pub fn close(&self) {
        self.inner.disconnect();
        self.inner.outgoing.send(Outgoing::Close).ok();
    }

    pub async fn run(&self) -> Result<(), PaseoError> {
        let driver = self.inner.driver.lock().take().ok_or(PaseoError::Closed)?;
        let _disconnect = Disconnect { inner: &self.inner };
        let Driver {
            socket,
            wire,
            outgoing,
            pongs,
        } = driver;
        let (mut sink, mut stream) = socket.split();
        let reader = async {
            while let Some(message) = stream.next().await {
                let message = message?;
                if let Message::Close(_) = message {
                    return Ok(());
                }
                if let Some(frame) = wire.decode(message)? {
                    self.inner.dispatch(frame);
                }
            }
            Ok(())
        };
        let writer = async {
            while let Ok(outgoing) = outgoing.recv_async().await {
                match outgoing {
                    Outgoing::Frame(frame) => sink.send(wire.encode(frame)?).await?,
                    Outgoing::Close => {
                        sink.send(Message::Close(None)).await?;
                        return Ok(());
                    }
                }
            }
            Ok(())
        };
        let liveness = async {
            let mut failures = 0;
            loop {
                Timer::after(LIVENESS_INTERVAL).await;
                pongs.drain();
                self.inner.send(Frame::Text(PING.to_owned()))?;
                if timed(pongs.recv_async(), LIVENESS_TIMEOUT).await.is_some() {
                    failures = 0;
                    continue;
                }
                failures += 1;
                if failures >= LIVENESS_FAILURE_THRESHOLD {
                    return Err(PaseoError::LivenessTimeout);
                }
            }
        };
        match future::select(
            pin!(reader),
            pin!(future::select(pin!(writer), pin!(liveness))),
        )
        .await
        {
            Either::Left((result, _)) => result,
            Either::Right((Either::Left((result, _)) | Either::Right((result, _)), _)) => result,
        }
    }
}

struct Disconnect<'a> {
    inner: &'a Inner,
}

impl Drop for Disconnect<'_> {
    fn drop(&mut self) {
        self.inner.disconnect();
    }
}

impl Inner {
    pub(crate) fn send(&self, frame: Frame) -> Result<(), PaseoError> {
        self.outgoing
            .send(Outgoing::Frame(frame))
            .map_err(|_| PaseoError::Closed)
    }

    fn disconnect(&self) {
        let mut state = self.state.lock();
        state.connected = false;
        for (_, pending) in state.pending.drain() {
            pending.reply.send(Err(PaseoError::Closed)).ok();
        }
        state.routes.clear();
        state.terminal_slots.clear();
        state.creations.clear();
        for (_, channel) in state.channels.drain() {
            channel.disconnect();
        }
    }

    fn dispatch(&self, frame: Frame) {
        match frame {
            Frame::Text(text) => self.dispatch_text(&text),
            Frame::Binary(bytes) => self.dispatch_binary(&bytes),
        }
    }

    fn dispatch_text(&self, text: &str) {
        let envelope = match serde_json::from_str::<Envelope<'_>>(text) {
            Ok(envelope) => envelope,
            Err(error) => {
                log::warn!("dropping an undecodable daemon message: {error}");
                return;
            }
        };
        if envelope.kind == "pong" {
            self.pongs.try_send(()).ok();
            return;
        }
        let Some(message) = envelope.message.filter(|_| envelope.kind == "session") else {
            return;
        };
        let heads = serde_json::from_str::<MessageHead<'_>>(message.get()).and_then(|head| {
            let payload_head = match head.payload {
                Some(payload) => serde_json::from_str::<PayloadHead<'_>>(payload.get())?,
                None => PayloadHead::default(),
            };
            Ok((head, payload_head))
        });
        let (head, payload_head) = match heads {
            Ok(heads) => heads,
            Err(error) => {
                self.fail_undecodable(message, error);
                return;
            }
        };
        if head.kind == "subscription.release.response" {
            return;
        }
        let mut state = self.state.lock();
        if let Some(request_id) = payload_head.request_id.as_deref()
            && let Entry::Occupied(entry) = state.pending.entry(request_id.to_owned())
        {
            if head.kind == entry.get().response_type || head.kind == "rpc_error" {
                let pending = entry.remove();
                let reply = Self::reply(
                    &mut state,
                    pending.registration,
                    &head,
                    &payload_head,
                    pending.request_type,
                );
                pending.reply.send(reply).ok();
                return;
            }
            let progress = match &entry.get().tracking {
                Some(Tracking::Progress { kind, deliver }) if head.kind == *kind => {
                    Some(deliver.clone())
                }
                _ => None,
            };
            if let Some(deliver) = progress {
                drop(state);
                deliver(&head.kind, message);
                return;
            }
        }
        if let Some(subscription_id) = payload_head.subscription_id.as_deref() {
            match state
                .routes
                .get(subscription_id)
                .map(|route| route.deliver.clone())
            {
                Some(deliver) => {
                    drop(state);
                    deliver(&head.kind, message);
                }
                None if payload_head.request_id.is_some() => {
                    drop(state);
                    self.release(subscription_id);
                }
                None => {}
            }
            return;
        }
        if head.kind != "agent.create.update" && head.kind != "workspace.create.update" {
            return;
        }
        let Some(payload) = head.payload else {
            return;
        };
        let key = match serde_json::from_str::<CreationKey<'_>>(payload.get()) {
            Ok(key) => key,
            Err(error) => {
                log::warn!("dropping an agent creation update without its key: {error}");
                return;
            }
        };
        if let Some(updates) = state.creations.get(key.idempotency_key.as_ref()) {
            let snapshot = serde_json::from_str::<CreationSnapshot>(payload.get())
                .map_err(|error| PaseoError::decode(head.kind.as_ref(), error));
            updates.send(snapshot).ok();
        }
    }

    fn fail_undecodable(&self, message: &RawValue, error: serde_json::Error) {
        let value = serde_json::from_str::<Value>(message.get()).ok();
        let field = |pointer: &str| {
            value
                .as_ref()
                .and_then(|value| value.pointer(pointer))
                .and_then(Value::as_str)
                .map(str::to_owned)
        };
        let kind = field("/type").unwrap_or_else(|| "session message".to_owned());
        let pending = field("/payload/requestId")
            .and_then(|request_id| self.state.lock().pending.remove(&request_id));
        match pending {
            Some(pending) => {
                pending
                    .reply
                    .send(Err(PaseoError::decode(kind, error)))
                    .ok();
            }
            None => log::warn!("dropping an undecodable {kind}: {error}"),
        }
    }

    fn reply(
        state: &mut State,
        registration: Option<Registration>,
        head: &MessageHead<'_>,
        payload_head: &PayloadHead<'_>,
        request_type: &'static str,
    ) -> Reply {
        let payload = head.payload.ok_or_else(|| {
            PaseoError::decode(
                head.kind.as_ref(),
                serde_json::Error::missing_field("payload"),
            )
        })?;
        if head.kind == "rpc_error" {
            let error = serde_json::from_str::<RpcError>(payload.get())
                .map_err(|error| PaseoError::decode("rpc_error", error))?;
            return Err(PaseoError::Rpc(error.with_request_type(request_type)));
        }
        if let Some(registration) = registration {
            registration(state, payload_head, payload)?;
        }
        Ok(payload.to_owned())
    }

    fn dispatch_binary(&self, bytes: &[u8]) {
        if let Some(frame) = ChannelFrame::decode(bytes) {
            let channel = self.state.lock().channels.get(&frame.channel).cloned();
            if let Some(channel) = channel {
                channel.receive(&frame);
            }
            return;
        }
        if let Some(frame) = HostRouteFrame::decode(bytes) {
            let route = self
                .state
                .lock()
                .routes
                .get(frame.route_id)
                .and_then(|route| route.host_route.clone());
            if let Some(route) = route {
                route
                    .send(RouteEvent::Frame(Frame::Binary(frame.payload.to_vec())))
                    .ok();
            }
            return;
        }
        if let Some(frame) = FileTransferFrame::decode(bytes) {
            self.receive_file_frame(&frame);
            return;
        }
        let Some(frame) = TerminalFrame::decode(bytes) else {
            return;
        };
        let state = self.state.lock();
        let Some(SlotOwner { events, .. }) = state.terminal_slots.get(&frame.slot) else {
            return;
        };
        let event = match frame.opcode {
            Opcode::Output => Ok(TerminalStreamEvent::Output(frame.payload.to_vec())),
            Opcode::Restore => Ok(TerminalStreamEvent::Restore(frame.payload.to_vec())),
            Opcode::Snapshot => serde_json::from_slice(frame.payload)
                .map(TerminalStreamEvent::Snapshot)
                .map_err(|error| PaseoError::decode("terminal snapshot", error)),
            Opcode::Input | Opcode::Resize => return,
        };
        events.send(event).ok();
    }

    fn receive_file_frame(&self, frame: &FileTransferFrame<'_>) {
        let mut state = self.state.lock();
        let Entry::Occupied(mut entry) = state.pending.entry(frame.request_id().to_owned()) else {
            return;
        };
        let Some(Tracking::Download(download)) = &mut entry.get_mut().tracking else {
            return;
        };
        let outcome = match frame {
            FileTransferFrame::Begin { metadata, .. } => {
                match serde_json::from_slice::<TransferMetadata>(metadata) {
                    Ok(metadata) => {
                        download.begin(metadata);
                        return;
                    }
                    Err(error) => Err(PaseoError::decode("file transfer metadata", error)),
                }
            }
            FileTransferFrame::Chunk { payload, .. } => {
                download.append(payload);
                return;
            }
            FileTransferFrame::End { .. } => Ok(()),
        };
        if let Some(Tracking::Download(download)) = entry.remove().tracking {
            match outcome {
                Ok(()) => download.finish(),
                Err(error) => download.fail(error),
            }
        }
    }

    pub(crate) fn release(&self, subscription_id: &str) {
        let mut state = self.state.lock();
        if let Some(route) = state.routes.remove(subscription_id) {
            if let Some(slot) = route.terminal_slot
                && state
                    .terminal_slots
                    .get(&slot)
                    .is_some_and(|owner| owner.subscription_id == subscription_id)
            {
                state.terminal_slots.remove(&slot);
            }
            if let Some(channel) = route.channel {
                state.channels.remove(&channel);
            }
        }
        if !state.connected {
            return;
        }
        drop(state);
        let body = SubscriptionRelease { subscription_id };
        let text = session_message("subscription.release.request", Some(&new_id()), &body)
            .expect("a subscription release serialises");
        if let Err(error) = self.send(Frame::Text(text)) {
            log::debug!("could not release subscription {subscription_id}: {error}");
        }
    }

    pub(crate) fn submit<B: Serialize>(
        self: &Arc<Self>,
        request: &Request<'_, B>,
        registration: Option<Registration>,
        tracking: Option<Tracking>,
    ) -> Result<PendingReply, PaseoError> {
        let request_id = new_id();
        let text = session_message(request.kind, Some(&request_id), request.body)?;
        let (reply, receiver) = oneshot::channel();
        {
            let mut state = self.state.lock();
            if !state.connected {
                return Err(PaseoError::Closed);
            }
            state.pending.insert(
                request_id.clone(),
                Pending {
                    response_type: request.response_type,
                    request_type: request.kind,
                    reply,
                    registration,
                    tracking,
                },
            );
        }
        let pending = PendingReply {
            request_id,
            request_type: request.kind,
            receiver,
            client: Arc::downgrade(self),
        };
        self.send(Frame::Text(text))?;
        Ok(pending)
    }

    pub(crate) fn register_creation(&self, key: String, updates: Updates<CreationSnapshot>) {
        self.state.lock().creations.insert(key, updates);
    }

    pub(crate) fn unregister_creation(&self, key: &str) {
        self.state.lock().creations.remove(key);
    }

    pub(crate) fn route_registration<U: Routed>(updates: Updates<U>) -> Registration {
        Box::new(move |state, head, _payload| {
            if let Some(id) = head.subscription_id.as_deref() {
                state.routes.insert(
                    id.to_owned(),
                    Route {
                        deliver: Self::deliver(updates),
                        terminal_slot: None,
                        channel: None,
                        host_route: None,
                    },
                );
            }
            Ok(())
        })
    }

    pub(crate) fn host_route_registration(
        events: flume::Sender<RouteEvent>,
        response_type: &'static str,
        route: oneshot::Sender<String>,
    ) -> Registration {
        Box::new(move |state, _head, payload| {
            let opened = serde_json::from_str::<RouteOpened>(payload.get())
                .map_err(|error| PaseoError::decode(response_type, error))?;
            if opened.route_id.is_empty()
                || opened.route_id.len() > HostRouteFrame::MAX_ROUTE_ID_BYTES
            {
                return Err(ProtocolViolation::InvalidRouteId.into());
            }
            let frames = events.clone();
            let deliver = Arc::new(move |kind: &str, message: &RawValue| {
                let event = match kind {
                    "hosts.route.frame" => {
                        serde_json::from_str::<Payload<RouteText>>(message.get())
                            .map(|message| RouteEvent::Frame(Frame::Text(message.payload.data)))
                    }
                    "hosts.route.closed" => {
                        serde_json::from_str::<Payload<HostRouteClosed>>(message.get())
                            .map(|message| RouteEvent::Closed(message.payload))
                    }
                    _ => return,
                };
                match event {
                    Ok(event) => {
                        frames.send(event).ok();
                    }
                    Err(error) => log::warn!("dropping an undecodable {kind}: {error}"),
                }
            });
            route.send(opened.route_id.clone()).ok();
            state.routes.insert(
                opened.route_id,
                Route {
                    deliver,
                    terminal_slot: None,
                    channel: None,
                    host_route: Some(events),
                },
            );
            Ok(())
        })
    }

    pub(crate) fn channel_registration(
        channel: Arc<dyn ChannelEndpoint>,
        events: Deliver,
        request_type: &'static str,
        response_type: &'static str,
    ) -> Registration {
        Box::new(move |state, head, payload| {
            let opened = serde_json::from_str::<ChannelOpened>(payload.get())
                .map_err(|error| PaseoError::decode(response_type, error))?;
            if let Some(error) = opened.error {
                return Err(PaseoError::rpc(request_type, error));
            }
            let id = head
                .subscription_id
                .as_deref()
                .ok_or(ProtocolViolation::MissingSubscriptionId { response_type })?;
            let number = opened.channel.ok_or_else(|| {
                PaseoError::decode(response_type, serde_json::Error::missing_field("channel"))
            })?;
            channel.open(number);
            state.channels.insert(number, channel);
            state.routes.insert(
                id.to_owned(),
                Route {
                    deliver: events,
                    terminal_slot: None,
                    channel: Some(number),
                    host_route: None,
                },
            );
            Ok(())
        })
    }

    pub(crate) fn terminal_registration(events: Updates<TerminalStreamEvent>) -> Registration {
        Box::new(move |state, head, payload| {
            let subscribed = serde_json::from_str::<TerminalSubscribed>(payload.get())
                .map_err(|error| PaseoError::decode("subscribe_terminal_response", error))?;
            if let Some(error) = subscribed.error {
                return Err(PaseoError::rpc("subscribe_terminal_request", error));
            }
            let id = head.subscription_id.as_deref().ok_or(
                ProtocolViolation::MissingSubscriptionId {
                    response_type: "subscribe_terminal_response",
                },
            )?;
            let slot = subscribed.slot.ok_or_else(|| {
                PaseoError::decode(
                    "subscribe_terminal_response",
                    serde_json::Error::missing_field("slot"),
                )
            })?;
            state.terminal_slots.insert(
                slot,
                SlotOwner {
                    subscription_id: id.to_owned(),
                    events: events.clone(),
                },
            );
            state.routes.insert(
                id.to_owned(),
                Route {
                    deliver: Self::deliver(events),
                    terminal_slot: Some(slot),
                    channel: None,
                    host_route: None,
                },
            );
            Ok(())
        })
    }

    pub(crate) fn deliver<U: Routed>(updates: Updates<U>) -> Deliver {
        Arc::new(move |kind, message| {
            updates.send(U::decode(kind, message)).ok();
        })
    }

    pub(crate) fn send_terminal_frame(
        &self,
        opcode: Opcode,
        slot: u8,
        payload: &[u8],
    ) -> Result<(), PaseoError> {
        let frame = TerminalFrame {
            opcode,
            slot,
            payload,
        };
        self.send(Frame::Binary(frame.encode()))
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SubscriptionRelease<'a> {
    subscription_id: &'a str,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreationKey<'a> {
    #[serde(borrow)]
    idempotency_key: Cow<'a, str>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RouteOpened {
    route_id: String,
}

#[derive(Deserialize)]
struct RouteText {
    data: String,
}

#[derive(Deserialize)]
struct ChannelOpened {
    #[serde(default)]
    channel: Option<u32>,
    #[serde(default)]
    error: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct TerminalSubscribed {
    #[serde(default)]
    slot: Option<u8>,
    #[serde(default)]
    error: Option<String>,
}

#[derive(Deserialize)]
pub(crate) struct Payload<T> {
    pub(crate) payload: T,
}

pub(crate) struct PendingReply {
    request_id: String,
    request_type: &'static str,
    receiver: oneshot::Receiver<Reply>,
    client: Weak<Inner>,
}

impl PendingReply {
    pub(crate) fn request_id(&self) -> &str {
        &self.request_id
    }

    pub(crate) async fn wait(mut self, timeout: Option<Duration>) -> Reply {
        let receiver = &mut self.receiver;
        let reply = match timeout {
            Some(timeout) => timed(receiver, timeout).await.ok_or(PaseoError::Timeout {
                request_type: self.request_type,
            })?,
            None => receiver.await,
        };
        reply.map_err(|_| PaseoError::Closed)?
    }
}

impl Drop for PendingReply {
    fn drop(&mut self) {
        let Some(client) = self.client.upgrade() else {
            return;
        };
        client.state.lock().pending.remove(&self.request_id);
        let Ok(Some(Ok(payload))) = self.receiver.try_recv() else {
            return;
        };
        if let Ok(head) = serde_json::from_str::<PayloadHead<'_>>(payload.get())
            && let Some(subscription_id) = head.subscription_id
        {
            client.release(&subscription_id);
        }
    }
}

pub(crate) struct Request<'a, B> {
    pub(crate) kind: &'static str,
    pub(crate) response_type: &'static str,
    pub(crate) body: &'a B,
}

pub struct Subscription<S, U> {
    id: String,
    snapshot: S,
    updates: flume::Receiver<Result<U, PaseoError>>,
    client: Weak<Inner>,
}

impl<S, U> Subscription<S, U> {
    pub(crate) fn new(
        id: String,
        snapshot: S,
        updates: flume::Receiver<Result<U, PaseoError>>,
        client: &Arc<Inner>,
    ) -> Subscription<S, U> {
        Subscription {
            id,
            snapshot,
            updates,
            client: Arc::downgrade(client),
        }
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn snapshot(&self) -> &S {
        &self.snapshot
    }

    pub fn updates(&self) -> &flume::Receiver<Result<U, PaseoError>> {
        &self.updates
    }
}

impl<S, U> Drop for Subscription<S, U> {
    fn drop(&mut self) {
        if let Some(client) = self.client.upgrade() {
            client.release(&self.id);
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalSlot {
    terminal_id: String,
    slot: u8,
}

impl TerminalSlot {
    pub fn terminal_id(&self) -> &str {
        &self.terminal_id
    }
}

pub struct TerminalHandle {
    subscription: Subscription<TerminalSlot, TerminalStreamEvent>,
    writer: TerminalWriter,
}

impl TerminalHandle {
    pub(crate) fn new(
        subscription: Subscription<TerminalSlot, TerminalStreamEvent>,
        client: &Arc<Inner>,
    ) -> TerminalHandle {
        let writer = TerminalWriter {
            client: Arc::downgrade(client),
            slot: subscription.snapshot().slot,
        };
        TerminalHandle {
            subscription,
            writer,
        }
    }

    pub fn terminal_id(&self) -> &str {
        self.subscription.snapshot().terminal_id()
    }

    pub fn events(&self) -> &flume::Receiver<Result<TerminalStreamEvent, PaseoError>> {
        self.subscription.updates()
    }

    pub fn writer(&self) -> TerminalWriter {
        self.writer.clone()
    }
}

#[derive(Clone)]
pub struct TerminalWriter {
    client: Weak<Inner>,
    slot: u8,
}

impl TerminalWriter {
    pub fn send_input(&self, data: &[u8]) -> Result<(), PaseoError> {
        self.client
            .upgrade()
            .ok_or(PaseoError::Closed)?
            .send_terminal_frame(Opcode::Input, self.slot, data)
    }

    pub fn resize(&self, size: TerminalSize, intent: ResizeIntent) -> Result<(), PaseoError> {
        let payload = serde_json::to_vec(&ResizePayload { size, intent })?;
        self.client
            .upgrade()
            .ok_or(PaseoError::Closed)?
            .send_terminal_frame(Opcode::Resize, self.slot, &payload)
    }
}
