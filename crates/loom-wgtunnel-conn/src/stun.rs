// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV4, SocketAddrV6};
use std::time::Duration;
use thiserror::Error;
use tokio::net::UdpSocket;
use tokio::time::timeout;
use tracing::{debug, instrument, warn};

const STUN_TIMEOUT: Duration = Duration::from_secs(3);
const STUN_MAGIC_COOKIE: u32 = 0x2112A442;

const ATTR_MAPPED_ADDRESS: u16 = 0x0001;
const ATTR_XOR_MAPPED_ADDRESS: u16 = 0x0020;

const ADDR_FAMILY_IPV4: u8 = 0x01;
const ADDR_FAMILY_IPV6: u8 = 0x02;

pub const DEFAULT_STUN_SERVERS: &[&str] = &[
	"stun.l.google.com:19302",
	"stun1.l.google.com:19302",
	"stun.cloudflare.com:3478",
];

#[derive(Debug, Error)]
pub enum StunError {
	#[error("I/O error: {0}")]
	Io(#[from] std::io::Error),

	#[error("timeout waiting for STUN response")]
	Timeout,

	#[error("invalid STUN response")]
	InvalidResponse,

	#[error("no STUN servers available")]
	NoServers,

	#[error("failed to resolve STUN server: {0}")]
	Resolution(String),
}

pub type Result<T> = std::result::Result<T, StunError>;

#[instrument(skip(socket), fields(servers = ?stun_servers.len()))]
pub async fn discover_endpoint(
	socket: &UdpSocket,
	stun_servers: &[SocketAddr],
) -> Result<SocketAddr> {
	if stun_servers.is_empty() {
		return Err(StunError::NoServers);
	}

	let transaction_id: [u8; 12] = fastrand::u128(..).to_le_bytes()[..12]
		.try_into()
		.expect("12 bytes from 16");

	let request = build_binding_request(&transaction_id);

	for server in stun_servers {
		debug!(?server, "sending STUN binding request");

		if let Err(e) = socket.send_to(&request, server).await {
			warn!(?server, error = %e, "failed to send STUN request");
			continue;
		}

		let mut buf = [0u8; 1024];
		match timeout(STUN_TIMEOUT, socket.recv_from(&mut buf)).await {
			Ok(Ok((len, from))) => {
				if from != *server {
					warn!(?from, expected = ?server, "STUN response from unexpected source");
					continue;
				}

				match parse_binding_response(&buf[..len], &transaction_id) {
					Ok(addr) => {
						debug!(?addr, "discovered public endpoint");
						return Ok(addr);
					}
					Err(e) => {
						warn!(?server, error = %e, "invalid STUN response");
						continue;
					}
				}
			}
			Ok(Err(e)) => {
				warn!(?server, error = %e, "failed to receive STUN response");
				continue;
			}
			Err(_) => {
				debug!(?server, "STUN request timed out");
				continue;
			}
		}
	}

	Err(StunError::Timeout)
}

pub fn build_binding_request(transaction_id: &[u8; 12]) -> Vec<u8> {
	let mut request = Vec::with_capacity(20);

	request.extend_from_slice(&0x0001u16.to_be_bytes());

	request.extend_from_slice(&0u16.to_be_bytes());

	request.extend_from_slice(&STUN_MAGIC_COOKIE.to_be_bytes());

	request.extend_from_slice(transaction_id);

	request
}

pub fn parse_binding_response(
	data: &[u8],
	expected_transaction_id: &[u8; 12],
) -> Result<SocketAddr> {
	if data.len() < 20 {
		return Err(StunError::InvalidResponse);
	}

	let message_type = u16::from_be_bytes([data[0], data[1]]);
	if message_type != 0x0101 {
		return Err(StunError::InvalidResponse);
	}

	let message_length = u16::from_be_bytes([data[2], data[3]]) as usize;

	let magic = u32::from_be_bytes([data[4], data[5], data[6], data[7]]);
	if magic != STUN_MAGIC_COOKIE {
		return Err(StunError::InvalidResponse);
	}

	if &data[8..20] != expected_transaction_id {
		return Err(StunError::InvalidResponse);
	}

	if data.len() < 20 + message_length {
		return Err(StunError::InvalidResponse);
	}

	let mut offset = 20;
	let end = 20 + message_length;

	while offset + 4 <= end {
		let attr_type = u16::from_be_bytes([data[offset], data[offset + 1]]);
		let attr_length = u16::from_be_bytes([data[offset + 2], data[offset + 3]]) as usize;
		offset += 4;

		if offset + attr_length > end {
			return Err(StunError::InvalidResponse);
		}

		if attr_type == ATTR_XOR_MAPPED_ADDRESS || attr_type == ATTR_MAPPED_ADDRESS {
			let xor = attr_type == ATTR_XOR_MAPPED_ADDRESS;
			if let Some(addr) = parse_mapped_address(&data[offset..offset + attr_length], xor) {
				return Ok(addr);
			}
		}

		let padded_length = (attr_length + 3) & !3;
		offset += padded_length;
	}

	Err(StunError::InvalidResponse)
}

fn parse_mapped_address(data: &[u8], xor: bool) -> Option<SocketAddr> {
	if data.len() < 4 {
		return None;
	}

	let family = data[1];
	let port = u16::from_be_bytes([data[2], data[3]]);

	let port = if xor {
		port ^ ((STUN_MAGIC_COOKIE >> 16) as u16)
	} else {
		port
	};

	match family {
		ADDR_FAMILY_IPV4 if data.len() >= 8 => {
			let mut ip_bytes = [data[4], data[5], data[6], data[7]];
			if xor {
				let magic_bytes = STUN_MAGIC_COOKIE.to_be_bytes();
				for (i, b) in ip_bytes.iter_mut().enumerate() {
					*b ^= magic_bytes[i];
				}
			}
			let ip = Ipv4Addr::from(ip_bytes);
			Some(SocketAddr::V4(SocketAddrV4::new(ip, port)))
		}
		ADDR_FAMILY_IPV6 if data.len() >= 20 => {
			let mut ip_bytes: [u8; 16] = data[4..20].try_into().ok()?;
			if xor {
				let magic_bytes = STUN_MAGIC_COOKIE.to_be_bytes();
				for (i, b) in ip_bytes.iter_mut().enumerate().take(4) {
					*b ^= magic_bytes[i];
				}
			}
			let ip = Ipv6Addr::from(ip_bytes);
			Some(SocketAddr::V6(SocketAddrV6::new(ip, port, 0, 0)))
		}
		_ => None,
	}
}

pub async fn resolve_stun_servers(servers: &[&str]) -> Vec<SocketAddr> {
	let mut addrs = Vec::new();

	for server in servers {
		match tokio::net::lookup_host(server).await {
			Ok(mut resolved) => {
				if let Some(addr) = resolved.next() {
					addrs.push(addr);
				}
			}
			Err(e) => {
				warn!(server, error = %e, "failed to resolve STUN server");
			}
		}
	}

	addrs
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_build_binding_request() {
		let transaction_id = [0u8; 12];
		let request = build_binding_request(&transaction_id);

		assert_eq!(request.len(), 20);
		assert_eq!(&request[0..2], &[0x00, 0x01]);
		assert_eq!(&request[2..4], &[0x00, 0x00]);
		assert_eq!(&request[4..8], &STUN_MAGIC_COOKIE.to_be_bytes());
		assert_eq!(&request[8..20], &transaction_id);
	}

	#[test]
	fn test_parse_binding_response_xor_mapped_ipv4() {
		let mut response = Vec::new();
		response.extend_from_slice(&0x0101u16.to_be_bytes());
		response.extend_from_slice(&12u16.to_be_bytes());
		response.extend_from_slice(&STUN_MAGIC_COOKIE.to_be_bytes());
		response.extend_from_slice(&[0u8; 12]);

		response.extend_from_slice(&ATTR_XOR_MAPPED_ADDRESS.to_be_bytes());
		response.extend_from_slice(&8u16.to_be_bytes());

		response.push(0x00);
		response.push(ADDR_FAMILY_IPV4);

		let port: u16 = 12345;
		let xor_port = port ^ ((STUN_MAGIC_COOKIE >> 16) as u16);
		response.extend_from_slice(&xor_port.to_be_bytes());

		let ip = Ipv4Addr::new(203, 0, 113, 1);
		let magic_bytes = STUN_MAGIC_COOKIE.to_be_bytes();
		let ip_bytes = ip.octets();
		let xor_ip: [u8; 4] = [
			ip_bytes[0] ^ magic_bytes[0],
			ip_bytes[1] ^ magic_bytes[1],
			ip_bytes[2] ^ magic_bytes[2],
			ip_bytes[3] ^ magic_bytes[3],
		];
		response.extend_from_slice(&xor_ip);

		let result = parse_binding_response(&response, &[0u8; 12]).unwrap();
		assert_eq!(result, SocketAddr::V4(SocketAddrV4::new(ip, port)));
	}
}
