use serde::{Deserialize, Serialize};

use crate::error::{PaseoError, Result};

pub const OP_OUTPUT: u8 = 0x01;
pub const OP_INPUT: u8 = 0x02;
pub const OP_RESIZE: u8 = 0x03;
pub const OP_SNAPSHOT: u8 = 0x04;
pub const OP_RESTORE: u8 = 0x05;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Opcode {
    Output,
    Input,
    Resize,
    Snapshot,
    Restore,
}

impl Opcode {
    pub fn from_u8(value: u8) -> Option<Opcode> {
        match value {
            OP_OUTPUT => Some(Opcode::Output),
            OP_INPUT => Some(Opcode::Input),
            OP_RESIZE => Some(Opcode::Resize),
            OP_SNAPSHOT => Some(Opcode::Snapshot),
            OP_RESTORE => Some(Opcode::Restore),
            _ => None,
        }
    }

    pub fn to_u8(self) -> u8 {
        match self {
            Opcode::Output => OP_OUTPUT,
            Opcode::Input => OP_INPUT,
            Opcode::Resize => OP_RESIZE,
            Opcode::Snapshot => OP_SNAPSHOT,
            Opcode::Restore => OP_RESTORE,
        }
    }
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Frame {
    pub opcode: Opcode,
    pub slot: u8,
    pub payload: Vec<u8>,
}

pub fn is_terminal_frame(bytes: &[u8]) -> bool {
    !bytes.is_empty() && Opcode::from_u8(bytes[0]).is_some()
}

pub fn encode(opcode: Opcode, slot: u8, payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(2 + payload.len());
    out.push(opcode.to_u8());
    out.push(slot);
    out.extend_from_slice(payload);
    out
}

pub fn decode(bytes: &[u8]) -> Option<Frame> {
    if bytes.len() < 2 {
        return None;
    }
    let opcode = Opcode::from_u8(bytes[0])?;
    Some(Frame {
        opcode,
        slot: bytes[1],
        payload: bytes[2..].to_vec(),
    })
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct ResizePayload {
    pub rows: u32,
    pub cols: u32,
}

pub fn encode_input(slot: u8, data: &[u8]) -> Vec<u8> {
    encode(Opcode::Input, slot, data)
}

pub fn encode_resize(slot: u8, rows: u32, cols: u32) -> Result<Vec<u8>> {
    let json = serde_json::to_vec(&ResizePayload { rows, cols })?;
    Ok(encode(Opcode::Resize, slot, &json))
}

pub fn decode_resize(payload: &[u8]) -> Result<ResizePayload> {
    serde_json::from_slice(payload).map_err(PaseoError::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_layout_round_trips() {
        let frame = encode(Opcode::Output, 7, b"hello");
        assert_eq!(frame[0], OP_OUTPUT);
        assert_eq!(frame[1], 7);
        assert_eq!(&frame[2..], b"hello");

        let decoded = decode(&frame).expect("decode");
        assert_eq!(decoded.opcode, Opcode::Output);
        assert_eq!(decoded.slot, 7);
        assert_eq!(decoded.payload, b"hello");
    }

    #[test]
    fn short_or_unknown_frames_are_rejected() {
        assert!(decode(&[]).is_none());
        assert!(decode(&[0x01]).is_none());
        assert!(decode(&[0x00, 0x00]).is_none());
    }

    #[test]
    fn slot_is_a_single_byte() {
        let frame = encode(Opcode::Input, 255, b"");
        assert_eq!(frame, vec![OP_INPUT, 255]);
    }

    #[test]
    fn resize_payload_is_json() {
        let frame = encode_resize(3, 40, 120).expect("encode");
        let decoded = decode(&frame).expect("decode");
        assert_eq!(decoded.opcode, Opcode::Resize);
        assert_eq!(decoded.slot, 3);
        let payload = decode_resize(&decoded.payload).expect("resize");
        assert_eq!(payload.rows, 40);
        assert_eq!(payload.cols, 120);
        assert_eq!(decoded.payload, br#"{"rows":40,"cols":120}"#.to_vec());
    }

    #[test]
    fn terminal_frame_detection_uses_leading_opcode() {
        assert!(is_terminal_frame(&[OP_OUTPUT, 0]));
        assert!(is_terminal_frame(&[OP_RESTORE, 1, 2, 3]));
        assert!(!is_terminal_frame(&[0x40, 0, 0]));
        assert!(!is_terminal_frame(&[]));
    }
}
