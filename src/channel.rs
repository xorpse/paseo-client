use std::collections::VecDeque;
use std::future::poll_fn;
use std::io;
use std::pin::Pin;
use std::sync::{Arc, OnceLock, Weak};
use std::task::{Context, Poll, Waker};

use futures::io::{AsyncRead, AsyncWrite};
use parking_lot::Mutex;
use serde::de::DeserializeOwned;
use serde_json::value::RawValue;

use crate::binary_frames::{ChannelFrame, ChannelOpcode};
use crate::client::{Deliver, Inner, Payload};
use crate::error::{ArgumentError, PaseoError};
use crate::protocol::channels::{NetClosed, ProcessExit};
use crate::protocol::terminals::TerminalSize;
use crate::transport::Frame;

const INITIAL_CREDIT: u64 = 256 * 1024;
const MAX_FRAME_PAYLOAD: usize = 64 * 1024;
const GRANT_THRESHOLD: u64 = INITIAL_CREDIT / 2;

pub(crate) trait Termination: DeserializeOwned + Clone + Send + Sync + 'static {
    const EVENT: &'static str;
}

impl Termination for ProcessExit {
    const EVENT: &'static str = "process.exited";
}

impl Termination for NetClosed {
    const EVENT: &'static str = "net.closed";
}

pub(crate) trait ChannelEndpoint: Send + Sync {
    fn open(&self, number: u32);
    fn receive(&self, frame: &ChannelFrame<'_>);
    fn disconnect(&self);
}

#[derive(Clone, Copy)]
enum OutputStream {
    ErrorOutput,
    Output,
}

#[derive(Clone, Copy, Default, Eq, PartialEq)]
enum ReaderMode {
    #[default]
    Held,
    Discarding,
    Reading,
}

#[derive(Default)]
struct Buffer {
    bytes: VecDeque<u8>,
    mode: ReaderMode,
    waker: Option<Waker>,
}

impl Buffer {
    fn wake(&mut self) {
        if let Some(waker) = self.waker.take() {
            waker.wake();
        }
    }
}

enum Lifecycle<T> {
    Disconnected,
    Finished(T),
    Open,
}

struct ChannelState<T> {
    output: Buffer,
    error_output: Buffer,
    output_ended: bool,
    lifecycle: Lifecycle<T>,
    lifecycle_wakers: Vec<Waker>,
    input_credit: u64,
    input_closed: bool,
    input_waker: Option<Waker>,
    output_allowance: u64,
    ungranted: u64,
}

impl<T> ChannelState<T> {
    fn buffer(&mut self, stream: OutputStream) -> &mut Buffer {
        match stream {
            OutputStream::ErrorOutput => &mut self.error_output,
            OutputStream::Output => &mut self.output,
        }
    }

    fn ended(&self, stream: OutputStream) -> bool {
        match self.lifecycle {
            Lifecycle::Finished(_) => true,
            Lifecycle::Disconnected | Lifecycle::Open => {
                matches!(stream, OutputStream::Output) && self.output_ended
            }
        }
    }

    fn consume(&mut self, bytes: usize, reader_waiting: bool) -> Option<u32> {
        self.ungranted += bytes as u64;
        let due = self.ungranted >= GRANT_THRESHOLD || (reader_waiting && self.ungranted > 0);
        if !due || !matches!(self.lifecycle, Lifecycle::Open) {
            return None;
        }
        let credit = std::mem::take(&mut self.ungranted);
        self.output_allowance += credit;
        Some(
            u32::try_from(credit)
                .expect("ungranted output never exceeds the initial credit, which fits in u32"),
        )
    }

    fn wake_all(&mut self) {
        self.output.wake();
        self.error_output.wake();
        if let Some(waker) = self.input_waker.take() {
            waker.wake();
        }
        for waker in self.lifecycle_wakers.drain(..) {
            waker.wake();
        }
    }
}

pub(crate) struct Channel<T> {
    number: OnceLock<u32>,
    client: Weak<Inner>,
    input_ends: bool,
    state: Mutex<ChannelState<T>>,
}

impl<T: Termination> Channel<T> {
    pub(crate) fn new(client: Weak<Inner>, input_ends: bool) -> Channel<T> {
        Channel {
            number: OnceLock::new(),
            client,
            input_ends,
            state: Mutex::new(ChannelState {
                output: Buffer::default(),
                error_output: Buffer::default(),
                output_ended: false,
                lifecycle: Lifecycle::Open,
                lifecycle_wakers: Vec::new(),
                input_credit: INITIAL_CREDIT,
                input_closed: false,
                input_waker: None,
                output_allowance: INITIAL_CREDIT,
                ungranted: 0,
            }),
        }
    }

    pub(crate) fn events(self: &Arc<Self>) -> Deliver {
        let channel = Arc::downgrade(self);
        Arc::new(move |kind: &str, message: &RawValue| {
            if kind != T::EVENT {
                return;
            }
            let Some(channel) = channel.upgrade() else {
                return;
            };
            match serde_json::from_str::<Payload<T>>(message.get()) {
                Ok(event) => channel.finish(event.payload),
                Err(error) => log::warn!("dropping an undecodable {kind}: {error}"),
            }
        })
    }

    fn finish(&self, termination: T) {
        let mut state = self.state.lock();
        if matches!(state.lifecycle, Lifecycle::Open) {
            state.lifecycle = Lifecycle::Finished(termination);
        }
        state.wake_all();
    }

    fn number(&self) -> u32 {
        *self
            .number
            .get()
            .expect("a channel is opened before it is used")
    }

    fn send(&self, opcode: ChannelOpcode, payload: &[u8]) -> Result<(), PaseoError> {
        let client = self.client.upgrade().ok_or(PaseoError::Closed)?;
        let frame = ChannelFrame {
            opcode,
            channel: self.number(),
            payload,
        };
        client.send(Frame::Binary(frame.encode()))
    }

    fn grant(&self, credit: Option<u32>) {
        let Some(credit) = credit else {
            return;
        };
        if let Err(error) = self.send(ChannelOpcode::Credit, &credit.to_be_bytes()) {
            log::debug!("could not grant channel credit: {error}");
        }
    }

    fn start_reading(&self, stream: OutputStream) {
        self.state.lock().buffer(stream).mode = ReaderMode::Reading;
    }

    fn discard(&self, stream: OutputStream) {
        let credit = {
            let mut state = self.state.lock();
            let buffer = state.buffer(stream);
            buffer.mode = ReaderMode::Discarding;
            let discarded = buffer.bytes.len();
            buffer.bytes.clear();
            state.consume(discarded, true)
        };
        self.grant(credit);
    }

    fn poll_read(
        &self,
        stream: OutputStream,
        cx: &mut Context<'_>,
        out: &mut [u8],
    ) -> Poll<io::Result<usize>> {
        let (poll, credit) = {
            let mut state = self.state.lock();
            let ended = state.ended(stream);
            let disconnected = matches!(state.lifecycle, Lifecycle::Disconnected);
            let buffer = state.buffer(stream);
            if buffer.bytes.is_empty() {
                if ended {
                    (Poll::Ready(Ok(0)), None)
                } else if disconnected {
                    (
                        Poll::Ready(Err(io::Error::from(io::ErrorKind::ConnectionAborted))),
                        None,
                    )
                } else {
                    buffer.waker = Some(cx.waker().clone());
                    (Poll::Pending, state.consume(0, true))
                }
            } else {
                let read = out.len().min(buffer.bytes.len());
                for (slot, byte) in out.iter_mut().zip(buffer.bytes.drain(..read)) {
                    *slot = byte;
                }
                let emptied = buffer.bytes.is_empty();
                (Poll::Ready(Ok(read)), state.consume(read, emptied))
            }
        };
        self.grant(credit);
        poll
    }

    fn poll_write(&self, cx: &mut Context<'_>, data: &[u8]) -> Poll<io::Result<usize>> {
        if data.is_empty() {
            return Poll::Ready(Ok(0));
        }
        let mut state = self.state.lock();
        if state.input_closed || !matches!(state.lifecycle, Lifecycle::Open) {
            return Poll::Ready(Err(io::Error::from(io::ErrorKind::BrokenPipe)));
        }
        if state.input_credit == 0 {
            state.input_waker = Some(cx.waker().clone());
            return Poll::Pending;
        }
        let written = data
            .len()
            .min(MAX_FRAME_PAYLOAD)
            .min(usize::try_from(state.input_credit).unwrap_or(usize::MAX));
        state.input_credit -= written as u64;
        drop(state);
        Poll::Ready(
            self.send(ChannelOpcode::Input, &data[..written])
                .map(|()| written)
                .map_err(io::Error::other),
        )
    }

    fn close_input(&self) -> Result<(), PaseoError> {
        let mut state = self.state.lock();
        if state.input_closed {
            return Ok(());
        }
        state.input_closed = true;
        let open = matches!(state.lifecycle, Lifecycle::Open);
        drop(state);
        if open && self.input_ends {
            self.send(ChannelOpcode::End, &[])?;
        }
        Ok(())
    }

    fn resize(&self, size: TerminalSize) -> Result<(), PaseoError> {
        let payload = serde_json::to_vec(&size)?;
        self.send(ChannelOpcode::Resize, &payload)
    }

    fn poll_finished(&self, cx: &mut Context<'_>) -> Poll<Result<T, PaseoError>> {
        let mut state = self.state.lock();
        match &state.lifecycle {
            Lifecycle::Disconnected => Poll::Ready(Err(PaseoError::Closed)),
            Lifecycle::Finished(termination) => Poll::Ready(Ok(termination.clone())),
            Lifecycle::Open => {
                if !state
                    .lifecycle_wakers
                    .iter()
                    .any(|waker| waker.will_wake(cx.waker()))
                {
                    state.lifecycle_wakers.push(cx.waker().clone());
                }
                Poll::Pending
            }
        }
    }

    fn receive_output(&self, stream: OutputStream, payload: &[u8]) -> Option<u32> {
        let mut state = self.state.lock();
        if state.ended(stream) {
            return None;
        }
        let bytes = payload.len() as u64;
        if bytes > state.output_allowance {
            log::warn!(
                "dropping {bytes} bytes of output on channel {} that exceed the granted credit",
                self.number()
            );
            return None;
        }
        state.output_allowance -= bytes;
        let buffer = state.buffer(stream);
        if buffer.mode == ReaderMode::Discarding {
            return state.consume(payload.len(), true);
        }
        buffer.bytes.extend(payload);
        buffer.wake();
        None
    }
}

impl<T: Termination> ChannelEndpoint for Channel<T> {
    fn open(&self, number: u32) {
        self.number
            .set(number)
            .expect("a channel is opened exactly once");
    }

    fn receive(&self, frame: &ChannelFrame<'_>) {
        let credit = match frame.opcode {
            ChannelOpcode::Output => self.receive_output(OutputStream::Output, frame.payload),
            ChannelOpcode::ErrorOutput => {
                self.receive_output(OutputStream::ErrorOutput, frame.payload)
            }
            ChannelOpcode::End => {
                let mut state = self.state.lock();
                state.output_ended = true;
                state.output.wake();
                None
            }
            ChannelOpcode::Credit => {
                if let Ok(bytes) = <[u8; 4]>::try_from(frame.payload) {
                    let mut state = self.state.lock();
                    state.input_credit += u64::from(u32::from_be_bytes(bytes));
                    if let Some(waker) = state.input_waker.take() {
                        waker.wake();
                    }
                }
                None
            }
            ChannelOpcode::Input | ChannelOpcode::Resize => None,
        };
        self.grant(credit);
    }

    fn disconnect(&self) {
        let mut state = self.state.lock();
        if matches!(state.lifecycle, Lifecycle::Open) {
            state.lifecycle = Lifecycle::Disconnected;
        }
        state.wake_all();
    }
}

trait ChannelIo: Send + Sync {
    fn start_reading(&self, stream: OutputStream);
    fn discard(&self, stream: OutputStream);
    fn poll_read(
        &self,
        stream: OutputStream,
        cx: &mut Context<'_>,
        out: &mut [u8],
    ) -> Poll<io::Result<usize>>;
    fn poll_write(&self, cx: &mut Context<'_>, data: &[u8]) -> Poll<io::Result<usize>>;
    fn close_input(&self) -> Result<(), PaseoError>;
}

pub(crate) struct ChannelLease<T: Termination> {
    subscription_id: String,
    channel: Arc<Channel<T>>,
}

impl<T: Termination> ChannelLease<T> {
    pub(crate) fn new(subscription_id: String, channel: Arc<Channel<T>>) -> ChannelLease<T> {
        ChannelLease {
            subscription_id,
            channel,
        }
    }
}

impl<T: Termination> ChannelIo for ChannelLease<T> {
    fn start_reading(&self, stream: OutputStream) {
        self.channel.start_reading(stream);
    }

    fn discard(&self, stream: OutputStream) {
        self.channel.discard(stream);
    }

    fn poll_read(
        &self,
        stream: OutputStream,
        cx: &mut Context<'_>,
        out: &mut [u8],
    ) -> Poll<io::Result<usize>> {
        self.channel.poll_read(stream, cx, out)
    }

    fn poll_write(&self, cx: &mut Context<'_>, data: &[u8]) -> Poll<io::Result<usize>> {
        self.channel.poll_write(cx, data)
    }

    fn close_input(&self) -> Result<(), PaseoError> {
        self.channel.close_input()
    }
}

impl<T: Termination> Drop for ChannelLease<T> {
    fn drop(&mut self) {
        if let Some(client) = self.channel.client.upgrade() {
            client.release(&self.subscription_id);
        }
    }
}

pub struct ChannelReader {
    io: Arc<dyn ChannelIo>,
    stream: OutputStream,
}

impl ChannelReader {
    fn start(io: Arc<dyn ChannelIo>, stream: OutputStream) -> ChannelReader {
        io.start_reading(stream);
        ChannelReader { io, stream }
    }
}

impl AsyncRead for ChannelReader {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<io::Result<usize>> {
        self.io.poll_read(self.stream, cx, buf)
    }
}

impl Drop for ChannelReader {
    fn drop(&mut self) {
        self.io.discard(self.stream);
    }
}

pub struct ChannelWriter {
    io: Arc<dyn ChannelIo>,
}

impl AsyncWrite for ChannelWriter {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        self.io.poll_write(cx, buf)
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }

    fn poll_close(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(self.io.close_input().map_err(io::Error::other))
    }
}

impl Drop for ChannelWriter {
    fn drop(&mut self) {
        if let Err(error) = self.io.close_input() {
            log::debug!("could not end channel input: {error}");
        }
    }
}

pub struct ProcessChannel {
    lease: Arc<ChannelLease<ProcessExit>>,
    pid: u64,
    pseudo_terminal: bool,
}

impl ProcessChannel {
    pub(crate) fn new(
        lease: ChannelLease<ProcessExit>,
        pid: u64,
        pseudo_terminal: bool,
    ) -> ProcessChannel {
        ProcessChannel {
            lease: Arc::new(lease),
            pid,
            pseudo_terminal,
        }
    }

    pub fn pid(&self) -> u64 {
        self.pid
    }

    pub fn is_pseudo_terminal(&self) -> bool {
        self.pseudo_terminal
    }

    pub fn into_parts(self) -> ProcessParts {
        let ProcessChannel {
            lease,
            pid,
            pseudo_terminal,
        } = self;
        let error_output = if pseudo_terminal {
            lease.discard(OutputStream::ErrorOutput);
            None
        } else {
            Some(ChannelReader::start(
                lease.clone(),
                OutputStream::ErrorOutput,
            ))
        };
        ProcessParts {
            input: ChannelWriter { io: lease.clone() },
            output: ChannelReader::start(lease.clone(), OutputStream::Output),
            error_output,
            process: ProcessHandle {
                lease,
                pid,
                pseudo_terminal,
            },
        }
    }
}

pub struct ProcessParts {
    pub input: ChannelWriter,
    pub output: ChannelReader,
    pub error_output: Option<ChannelReader>,
    pub process: ProcessHandle,
}

pub struct ProcessHandle {
    lease: Arc<ChannelLease<ProcessExit>>,
    pid: u64,
    pseudo_terminal: bool,
}

impl ProcessHandle {
    pub fn pid(&self) -> u64 {
        self.pid
    }

    pub fn is_pseudo_terminal(&self) -> bool {
        self.pseudo_terminal
    }

    pub fn resize(&self, size: TerminalSize) -> Result<(), PaseoError> {
        if !self.pseudo_terminal {
            return Err(ArgumentError::NotPseudoTerminal.into());
        }
        self.lease.channel.resize(size)
    }

    pub async fn exited(&self) -> Result<ProcessExit, PaseoError> {
        poll_fn(|cx| self.lease.channel.poll_finished(cx)).await
    }
}

pub struct NetChannel {
    lease: Arc<ChannelLease<NetClosed>>,
}

impl NetChannel {
    pub(crate) fn new(lease: ChannelLease<NetClosed>) -> NetChannel {
        NetChannel {
            lease: Arc::new(lease),
        }
    }

    pub fn into_parts(self) -> NetParts {
        let lease = self.lease;
        NetParts {
            input: ChannelWriter { io: lease.clone() },
            output: ChannelReader::start(lease.clone(), OutputStream::Output),
            connection: NetHandle { lease },
        }
    }
}

pub struct NetParts {
    pub input: ChannelWriter,
    pub output: ChannelReader,
    pub connection: NetHandle,
}

pub struct NetHandle {
    lease: Arc<ChannelLease<NetClosed>>,
}

impl NetHandle {
    pub async fn closed(&self) -> Result<NetClosed, PaseoError> {
        poll_fn(|cx| self.lease.channel.poll_finished(cx)).await
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Weak;
    use std::task::Context;

    use futures::task::noop_waker;

    use super::Channel;
    use crate::protocol::channels::ProcessExit;

    #[test]
    fn polling_the_exit_repeatedly_keeps_one_waker() {
        let channel = Channel::<ProcessExit>::new(Weak::new(), true);
        let waker = noop_waker();
        let mut cx = Context::from_waker(&waker);
        for _ in 0..100 {
            assert!(channel.poll_finished(&mut cx).is_pending());
        }
        assert_eq!(channel.state.lock().lifecycle_wakers.len(), 1);
    }
}
