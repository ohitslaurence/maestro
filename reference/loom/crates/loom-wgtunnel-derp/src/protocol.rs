// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use crate::error::{DerpError, Result, MAX_FRAME_SIZE};
use serde::{Deserialize, Serialize};

pub const FRAME_TYPE_SERVER_KEY: u8 = 0x01;
pub const FRAME_TYPE_SERVER_INFO: u8 = 0x02;
pub const FRAME_TYPE_SEND_PACKET: u8 = 0x03;
pub const FRAME_TYPE_RECV_PACKET: u8 = 0x04;
pub const FRAME_TYPE_KEEP_ALIVE: u8 = 0x05;
pub const FRAME_TYPE_NOTE_PREFERRED: u8 = 0x06;
pub const FRAME_TYPE_PEER_GONE: u8 = 0x07;
pub const FRAME_TYPE_PEER_PRESENT: u8 = 0x08;
pub const FRAME_TYPE_WATCH_CONNS: u8 = 0x09;
pub const FRAME_TYPE_CLOSE_PEER: u8 = 0x0a;
pub const FRAME_TYPE_CLIENT_INFO: u8 = 0x0b;

#[derive(Debug, Clone)]
pub enum DerpFrame {
	ServerKey { public_key: [u8; 32] },
	ServerInfo { info: ServerInfoPayload },
	SendPacket { dst_key: [u8; 32], data: Vec<u8> },
	RecvPacket { src_key: [u8; 32], data: Vec<u8> },
	KeepAlive,
	NotePreferred { preferred: bool },
	PeerGone { peer_key: [u8; 32] },
	PeerPresent { peer_key: [u8; 32] },
	WatchConns,
	ClosePeer { peer_key: [u8; 32] },
	ClientInfo { public_key: [u8; 32] },
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ServerInfoPayload {
	#[serde(default)]
	pub version: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientInfoPayload {
	#[serde(rename = "version")]
	pub version: i32,
	#[serde(rename = "meshKey", skip_serializing_if = "Option::is_none")]
	pub mesh_key: Option<String>,
}

impl Default for ClientInfoPayload {
	fn default() -> Self {
		Self {
			version: 2,
			mesh_key: None,
		}
	}
}

impl DerpFrame {
	pub fn frame_type(&self) -> u8 {
		match self {
			DerpFrame::ServerKey { .. } => FRAME_TYPE_SERVER_KEY,
			DerpFrame::ServerInfo { .. } => FRAME_TYPE_SERVER_INFO,
			DerpFrame::SendPacket { .. } => FRAME_TYPE_SEND_PACKET,
			DerpFrame::RecvPacket { .. } => FRAME_TYPE_RECV_PACKET,
			DerpFrame::KeepAlive => FRAME_TYPE_KEEP_ALIVE,
			DerpFrame::NotePreferred { .. } => FRAME_TYPE_NOTE_PREFERRED,
			DerpFrame::PeerGone { .. } => FRAME_TYPE_PEER_GONE,
			DerpFrame::PeerPresent { .. } => FRAME_TYPE_PEER_PRESENT,
			DerpFrame::WatchConns => FRAME_TYPE_WATCH_CONNS,
			DerpFrame::ClosePeer { .. } => FRAME_TYPE_CLOSE_PEER,
			DerpFrame::ClientInfo { .. } => FRAME_TYPE_CLIENT_INFO,
		}
	}

	pub fn encode(&self) -> Vec<u8> {
		let payload = self.encode_payload();
		let header = encode_frame_header(self.frame_type(), payload.len());
		let mut buf = Vec::with_capacity(4 + payload.len());
		buf.extend_from_slice(&header);
		buf.extend_from_slice(&payload);
		buf
	}

	fn encode_payload(&self) -> Vec<u8> {
		match self {
			DerpFrame::ServerKey { public_key } => public_key.to_vec(),
			DerpFrame::ServerInfo { info } => serde_json::to_vec(info).unwrap_or_default(),
			DerpFrame::SendPacket { dst_key, data } => {
				let mut buf = Vec::with_capacity(32 + data.len());
				buf.extend_from_slice(dst_key);
				buf.extend_from_slice(data);
				buf
			}
			DerpFrame::RecvPacket { src_key, data } => {
				let mut buf = Vec::with_capacity(32 + data.len());
				buf.extend_from_slice(src_key);
				buf.extend_from_slice(data);
				buf
			}
			DerpFrame::KeepAlive => Vec::new(),
			DerpFrame::NotePreferred { preferred } => vec![if *preferred { 1 } else { 0 }],
			DerpFrame::PeerGone { peer_key } => peer_key.to_vec(),
			DerpFrame::PeerPresent { peer_key } => peer_key.to_vec(),
			DerpFrame::WatchConns => Vec::new(),
			DerpFrame::ClosePeer { peer_key } => peer_key.to_vec(),
			DerpFrame::ClientInfo { public_key } => {
				let info = ClientInfoPayload::default();
				let info_json = serde_json::to_vec(&info).unwrap_or_default();
				let mut buf = Vec::with_capacity(32 + info_json.len());
				buf.extend_from_slice(public_key);
				buf.extend_from_slice(&info_json);
				buf
			}
		}
	}

	pub fn decode(data: &[u8]) -> Result<Self> {
		if data.len() < 4 {
			return Err(DerpError::PayloadTooShort(0));
		}

		let mut header = [0u8; 4];
		header.copy_from_slice(&data[..4]);
		let (frame_type, payload_len) = decode_frame_header(&header);

		if payload_len > MAX_FRAME_SIZE {
			return Err(DerpError::FrameTooLarge(payload_len, MAX_FRAME_SIZE));
		}

		if data.len() < 4 + payload_len {
			return Err(DerpError::PayloadTooShort(frame_type));
		}

		let payload = &data[4..4 + payload_len];
		Self::decode_payload(frame_type, payload)
	}

	fn decode_payload(frame_type: u8, payload: &[u8]) -> Result<Self> {
		match frame_type {
			FRAME_TYPE_SERVER_KEY => {
				if payload.len() < 32 {
					return Err(DerpError::PayloadTooShort(frame_type));
				}
				let mut public_key = [0u8; 32];
				public_key.copy_from_slice(&payload[..32]);
				Ok(DerpFrame::ServerKey { public_key })
			}
			FRAME_TYPE_SERVER_INFO => {
				let info: ServerInfoPayload = serde_json::from_slice(payload)?;
				Ok(DerpFrame::ServerInfo { info })
			}
			FRAME_TYPE_SEND_PACKET => {
				if payload.len() < 32 {
					return Err(DerpError::PayloadTooShort(frame_type));
				}
				let mut dst_key = [0u8; 32];
				dst_key.copy_from_slice(&payload[..32]);
				let data = payload[32..].to_vec();
				Ok(DerpFrame::SendPacket { dst_key, data })
			}
			FRAME_TYPE_RECV_PACKET => {
				if payload.len() < 32 {
					return Err(DerpError::PayloadTooShort(frame_type));
				}
				let mut src_key = [0u8; 32];
				src_key.copy_from_slice(&payload[..32]);
				let data = payload[32..].to_vec();
				Ok(DerpFrame::RecvPacket { src_key, data })
			}
			FRAME_TYPE_KEEP_ALIVE => Ok(DerpFrame::KeepAlive),
			FRAME_TYPE_NOTE_PREFERRED => {
				if payload.is_empty() {
					return Err(DerpError::PayloadTooShort(frame_type));
				}
				Ok(DerpFrame::NotePreferred {
					preferred: payload[0] != 0,
				})
			}
			FRAME_TYPE_PEER_GONE => {
				if payload.len() < 32 {
					return Err(DerpError::PayloadTooShort(frame_type));
				}
				let mut peer_key = [0u8; 32];
				peer_key.copy_from_slice(&payload[..32]);
				Ok(DerpFrame::PeerGone { peer_key })
			}
			FRAME_TYPE_PEER_PRESENT => {
				if payload.len() < 32 {
					return Err(DerpError::PayloadTooShort(frame_type));
				}
				let mut peer_key = [0u8; 32];
				peer_key.copy_from_slice(&payload[..32]);
				Ok(DerpFrame::PeerPresent { peer_key })
			}
			FRAME_TYPE_WATCH_CONNS => Ok(DerpFrame::WatchConns),
			FRAME_TYPE_CLOSE_PEER => {
				if payload.len() < 32 {
					return Err(DerpError::PayloadTooShort(frame_type));
				}
				let mut peer_key = [0u8; 32];
				peer_key.copy_from_slice(&payload[..32]);
				Ok(DerpFrame::ClosePeer { peer_key })
			}
			FRAME_TYPE_CLIENT_INFO => {
				if payload.len() < 32 {
					return Err(DerpError::PayloadTooShort(frame_type));
				}
				let mut public_key = [0u8; 32];
				public_key.copy_from_slice(&payload[..32]);
				Ok(DerpFrame::ClientInfo { public_key })
			}
			_ => Err(DerpError::InvalidFrameType(frame_type)),
		}
	}
}

pub fn encode_frame_header(frame_type: u8, payload_len: usize) -> [u8; 4] {
	let len = payload_len as u32;
	[
		frame_type,
		((len >> 16) & 0xff) as u8,
		((len >> 8) & 0xff) as u8,
		(len & 0xff) as u8,
	]
}

pub fn decode_frame_header(header: &[u8; 4]) -> (u8, usize) {
	let frame_type = header[0];
	let len = ((header[1] as usize) << 16) | ((header[2] as usize) << 8) | (header[3] as usize);
	(frame_type, len)
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_encode_decode_header() {
		let header = encode_frame_header(0x03, 1000);
		let (frame_type, len) = decode_frame_header(&header);
		assert_eq!(frame_type, 0x03);
		assert_eq!(len, 1000);
	}

	#[test]
	fn test_encode_decode_server_key() {
		let public_key = [0xab; 32];
		let frame = DerpFrame::ServerKey { public_key };
		let encoded = frame.encode();
		let decoded = DerpFrame::decode(&encoded).unwrap();
		if let DerpFrame::ServerKey { public_key: pk } = decoded {
			assert_eq!(pk, public_key);
		} else {
			panic!("Wrong frame type");
		}
	}

	#[test]
	fn test_encode_decode_keep_alive() {
		let frame = DerpFrame::KeepAlive;
		let encoded = frame.encode();
		let decoded = DerpFrame::decode(&encoded).unwrap();
		assert!(matches!(decoded, DerpFrame::KeepAlive));
	}

	#[test]
	fn test_encode_decode_send_packet() {
		let dst_key = [0xcd; 32];
		let data = vec![1, 2, 3, 4, 5];
		let frame = DerpFrame::SendPacket {
			dst_key,
			data: data.clone(),
		};
		let encoded = frame.encode();
		let decoded = DerpFrame::decode(&encoded).unwrap();
		if let DerpFrame::SendPacket {
			dst_key: dk,
			data: d,
		} = decoded
		{
			assert_eq!(dk, dst_key);
			assert_eq!(d, data);
		} else {
			panic!("Wrong frame type");
		}
	}

	#[test]
	fn test_encode_decode_note_preferred() {
		let frame = DerpFrame::NotePreferred { preferred: true };
		let encoded = frame.encode();
		let decoded = DerpFrame::decode(&encoded).unwrap();
		if let DerpFrame::NotePreferred { preferred } = decoded {
			assert!(preferred);
		} else {
			panic!("Wrong frame type");
		}
	}

	#[test]
	fn test_invalid_frame_type() {
		let data = [0xff, 0, 0, 0];
		let result = DerpFrame::decode(&data);
		assert!(matches!(result, Err(DerpError::InvalidFrameType(0xff))));
	}

	#[test]
	fn test_frame_too_large() {
		let header = encode_frame_header(0x01, MAX_FRAME_SIZE + 1);
		let mut data = header.to_vec();
		data.resize(4 + MAX_FRAME_SIZE + 1, 0);
		let result = DerpFrame::decode(&data);
		assert!(matches!(result, Err(DerpError::FrameTooLarge(_, _))));
	}
}
