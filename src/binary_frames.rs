use std::str;

use serde::Serialize;

use crate::protocol::terminals::TerminalSize;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Opcode {
    Input,
    Output,
    Resize,
    Restore,
    Snapshot,
}

impl Opcode {
    fn from_byte(value: u8) -> Option<Opcode> {
        match value {
            0x01 => Some(Opcode::Output),
            0x02 => Some(Opcode::Input),
            0x03 => Some(Opcode::Resize),
            0x04 => Some(Opcode::Snapshot),
            0x05 => Some(Opcode::Restore),
            _ => None,
        }
    }

    fn byte(self) -> u8 {
        match self {
            Opcode::Output => 0x01,
            Opcode::Input => 0x02,
            Opcode::Resize => 0x03,
            Opcode::Snapshot => 0x04,
            Opcode::Restore => 0x05,
        }
    }
}

pub(crate) struct TerminalFrame<'a> {
    pub(crate) opcode: Opcode,
    pub(crate) slot: u8,
    pub(crate) payload: &'a [u8],
}

impl<'a> TerminalFrame<'a> {
    pub(crate) fn decode(bytes: &'a [u8]) -> Option<TerminalFrame<'a>> {
        let [opcode, slot, payload @ ..] = bytes else {
            return None;
        };
        Some(TerminalFrame {
            opcode: Opcode::from_byte(*opcode)?,
            slot: *slot,
            payload,
        })
    }

    pub(crate) fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(2 + self.payload.len());
        out.push(self.opcode.byte());
        out.push(self.slot);
        out.extend_from_slice(self.payload);
        out
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ResizeIntent {
    Claim,
    Update,
}

#[derive(Serialize)]
pub(crate) struct ResizePayload {
    #[serde(flatten)]
    pub(crate) size: TerminalSize,
    pub(crate) intent: ResizeIntent,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ChannelOpcode {
    Credit,
    End,
    ErrorOutput,
    Input,
    Output,
    Resize,
}

impl ChannelOpcode {
    fn from_byte(value: u8) -> Option<ChannelOpcode> {
        match value {
            0x20 => Some(ChannelOpcode::Input),
            0x21 => Some(ChannelOpcode::Output),
            0x22 => Some(ChannelOpcode::ErrorOutput),
            0x23 => Some(ChannelOpcode::End),
            0x24 => Some(ChannelOpcode::Credit),
            0x25 => Some(ChannelOpcode::Resize),
            _ => None,
        }
    }

    fn byte(self) -> u8 {
        match self {
            ChannelOpcode::Input => 0x20,
            ChannelOpcode::Output => 0x21,
            ChannelOpcode::ErrorOutput => 0x22,
            ChannelOpcode::End => 0x23,
            ChannelOpcode::Credit => 0x24,
            ChannelOpcode::Resize => 0x25,
        }
    }
}

pub(crate) struct ChannelFrame<'a> {
    pub(crate) opcode: ChannelOpcode,
    pub(crate) channel: u32,
    pub(crate) payload: &'a [u8],
}

impl<'a> ChannelFrame<'a> {
    const HEADER_BYTES: usize = 5;

    pub(crate) fn decode(bytes: &'a [u8]) -> Option<ChannelFrame<'a>> {
        let [opcode, a, b, c, d, payload @ ..] = bytes else {
            return None;
        };
        Some(ChannelFrame {
            opcode: ChannelOpcode::from_byte(*opcode)?,
            channel: u32::from_be_bytes([*a, *b, *c, *d]),
            payload,
        })
    }

    pub(crate) fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(ChannelFrame::HEADER_BYTES + self.payload.len());
        out.push(self.opcode.byte());
        out.extend_from_slice(&self.channel.to_be_bytes());
        out.extend_from_slice(self.payload);
        out
    }
}

pub(crate) enum FileTransferFrame<'a> {
    Begin {
        request_id: &'a str,
        metadata: &'a [u8],
    },
    Chunk {
        request_id: &'a str,
        payload: &'a [u8],
    },
    End {
        request_id: &'a str,
    },
}

impl FileTransferFrame<'_> {
    const BEGIN: u8 = 0x10;
    const CHUNK: u8 = 0x11;
    const END: u8 = 0x12;

    pub(crate) fn encode(&self) -> Option<Vec<u8>> {
        let (opcode, request_id, metadata, payload): (u8, &str, Option<&[u8]>, &[u8]) = match self {
            FileTransferFrame::Begin {
                request_id,
                metadata,
            } => (FileTransferFrame::BEGIN, request_id, Some(metadata), &[]),
            FileTransferFrame::Chunk {
                request_id,
                payload,
            } => (FileTransferFrame::CHUNK, request_id, None, payload),
            FileTransferFrame::End { request_id } => {
                (FileTransferFrame::END, request_id, None, &[])
            }
        };
        let request_id_length = u8::try_from(request_id.len()).ok()?;
        let mut out =
            Vec::with_capacity(4 + request_id.len() + metadata.map_or(payload.len(), <[u8]>::len));
        out.push(opcode);
        out.push(request_id_length);
        out.extend_from_slice(request_id.as_bytes());
        if let Some(metadata) = metadata {
            out.extend_from_slice(&u16::try_from(metadata.len()).ok()?.to_be_bytes());
            out.extend_from_slice(metadata);
        }
        out.extend_from_slice(payload);
        Some(out)
    }
}

impl<'a> FileTransferFrame<'a> {
    pub(crate) fn decode(bytes: &'a [u8]) -> Option<FileTransferFrame<'a>> {
        let [
            opcode @ (FileTransferFrame::BEGIN | FileTransferFrame::CHUNK | FileTransferFrame::END),
            request_id_length,
            rest @ ..,
        ] = bytes
        else {
            return None;
        };
        let length = usize::from(*request_id_length);
        if length == 0 || rest.len() < length {
            return None;
        }
        let (request_id, body) = rest.split_at(length);
        let request_id = str::from_utf8(request_id).ok()?;
        match *opcode {
            FileTransferFrame::BEGIN => {
                let [high, low, metadata @ ..] = body else {
                    return None;
                };
                (usize::from(u16::from_be_bytes([*high, *low])) == metadata.len()).then_some(
                    FileTransferFrame::Begin {
                        request_id,
                        metadata,
                    },
                )
            }
            FileTransferFrame::CHUNK => Some(FileTransferFrame::Chunk {
                request_id,
                payload: body,
            }),
            _ => body
                .is_empty()
                .then_some(FileTransferFrame::End { request_id }),
        }
    }

    pub(crate) fn request_id(&self) -> &'a str {
        match self {
            FileTransferFrame::Begin { request_id, .. }
            | FileTransferFrame::Chunk { request_id, .. }
            | FileTransferFrame::End { request_id } => request_id,
        }
    }
}

pub(crate) struct HostRouteFrame<'a> {
    pub(crate) route_id: &'a str,
    pub(crate) payload: &'a [u8],
}

impl<'a> HostRouteFrame<'a> {
    const OPCODE: u8 = 0x30;
    pub(crate) const MAX_ROUTE_ID_BYTES: usize = 256;

    pub(crate) fn decode(bytes: &'a [u8]) -> Option<HostRouteFrame<'a>> {
        let [HostRouteFrame::OPCODE, high, low, rest @ ..] = bytes else {
            return None;
        };
        let length = usize::from(u16::from_be_bytes([*high, *low]));
        if length == 0 || length > HostRouteFrame::MAX_ROUTE_ID_BYTES || rest.len() < length {
            return None;
        }
        let (route_id, payload) = rest.split_at(length);
        Some(HostRouteFrame {
            route_id: str::from_utf8(route_id).ok()?,
            payload,
        })
    }

    pub(crate) fn encode(&self) -> Vec<u8> {
        let length = u16::try_from(self.route_id.len())
            .expect("route ids are checked against their length limit when the route opens");
        let mut out = Vec::with_capacity(3 + self.route_id.len() + self.payload.len());
        out.push(HostRouteFrame::OPCODE);
        out.extend_from_slice(&length.to_be_bytes());
        out.extend_from_slice(self.route_id.as_bytes());
        out.extend_from_slice(self.payload);
        out
    }
}
