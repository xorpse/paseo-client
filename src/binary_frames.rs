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
    pub(crate) fn encode(&self) -> Option<Vec<u8>> {
        let (opcode, request_id, metadata, payload): (u8, &str, Option<&[u8]>, &[u8]) = match self {
            FileTransferFrame::Begin {
                request_id,
                metadata,
            } => (0x10, request_id, Some(metadata), &[]),
            FileTransferFrame::Chunk {
                request_id,
                payload,
            } => (0x11, request_id, None, payload),
            FileTransferFrame::End { request_id } => (0x12, request_id, None, &[]),
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
