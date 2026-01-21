// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use crate::error::{DerpError, Result, MAX_FRAME_SIZE};
use crate::protocol::{decode_frame_header, DerpFrame};
use loom_wgtunnel_common::{DerpNode, WgKeyPair, WgPublicKey};
use rustls::pki_types::ServerName;
use std::sync::Arc;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio_rustls::TlsConnector;
use tracing::{debug, instrument};

pub trait AsyncReadWrite: AsyncRead + AsyncWrite + Unpin + Send {}
impl<T: AsyncRead + AsyncWrite + Unpin + Send> AsyncReadWrite for T {}

pub struct DerpClient {
	stream: Box<dyn AsyncReadWrite>,
	our_key: WgKeyPair,
	server_key: [u8; 32],
	home_region: u16,
}

impl DerpClient {
	#[instrument(skip(our_key), fields(host = %node.host_name, region = node.region_id))]
	pub async fn connect(node: &DerpNode, our_key: &WgKeyPair) -> Result<Self> {
		let port = if node.derp_port > 0 {
			node.derp_port
		} else {
			443
		};

		let addr = if let Some(ipv4) = node.ipv4 {
			format!("{}:{}", ipv4, port)
		} else if let Some(ipv6) = node.ipv6 {
			format!("[{}]:{}", ipv6, port)
		} else {
			format!("{}:{}", node.host_name, port)
		};

		debug!("connecting to DERP server at {}", addr);

		let tcp_stream = TcpStream::connect(&addr)
			.await
			.map_err(DerpError::Connection)?;

		let tls_config = rustls::ClientConfig::builder()
			.with_root_certificates(rustls::RootCertStore {
				roots: webpki_roots::TLS_SERVER_ROOTS.to_vec(),
			})
			.with_no_client_auth();

		let connector = TlsConnector::from(Arc::new(tls_config));
		let server_name: ServerName<'_> = node
			.host_name
			.clone()
			.try_into()
			.map_err(|e| DerpError::Tls(format!("invalid server name: {}", e)))?;

		let tls_stream = connector
			.connect(server_name, tcp_stream)
			.await
			.map_err(|e| DerpError::Tls(format!("TLS handshake failed: {}", e)))?;

		let mut stream: Box<dyn AsyncReadWrite> = Box::new(tls_stream);

		let http_upgrade = format!(
			"GET /derp HTTP/1.1\r\n\
			 Host: {}\r\n\
			 Connection: Upgrade\r\n\
			 Upgrade: DERP\r\n\
			 \r\n",
			node.host_name
		);
		stream
			.write_all(http_upgrade.as_bytes())
			.await
			.map_err(DerpError::Connection)?;

		let mut response_buf = vec![0u8; 4096];
		let mut total_read = 0;
		loop {
			let n = stream
				.read(&mut response_buf[total_read..])
				.await
				.map_err(DerpError::Connection)?;
			if n == 0 {
				return Err(DerpError::ConnectionClosed);
			}
			total_read += n;
			let response_str = String::from_utf8_lossy(&response_buf[..total_read]);
			if response_str.contains("\r\n\r\n") {
				let status_line = response_str.lines().next().unwrap_or("");
				if !status_line.starts_with("HTTP/1.") || !status_line.contains(" 101 ") {
					return Err(DerpError::Handshake(format!(
						"HTTP upgrade failed: {}",
						status_line
					)));
				}
				break;
			}
			if total_read >= response_buf.len() {
				return Err(DerpError::Handshake("HTTP response too large".to_string()));
			}
		}

		debug!("HTTP upgrade successful, reading server key");

		let server_key = Self::read_server_key(&mut stream).await?;
		let _server_info = Self::read_server_info(&mut stream).await?;

		debug!("sending client info");
		let client_info_frame = DerpFrame::ClientInfo {
			public_key: *our_key.public_key().as_bytes(),
		};
		stream
			.write_all(&client_info_frame.encode())
			.await
			.map_err(DerpError::Connection)?;

		debug!("DERP handshake complete");

		Ok(Self {
			stream,
			our_key: our_key.clone(),
			server_key,
			home_region: node.region_id,
		})
	}

	async fn read_server_key(stream: &mut Box<dyn AsyncReadWrite>) -> Result<[u8; 32]> {
		let frame = Self::read_frame_from_stream(stream).await?;
		match frame {
			DerpFrame::ServerKey { public_key } => Ok(public_key),
			_ => Err(DerpError::UnexpectedFrameType {
				expected: "ServerKey",
				actual: frame.frame_type(),
			}),
		}
	}

	async fn read_server_info(stream: &mut Box<dyn AsyncReadWrite>) -> Result<DerpFrame> {
		let frame = Self::read_frame_from_stream(stream).await?;
		match &frame {
			DerpFrame::ServerInfo { .. } => Ok(frame),
			_ => Err(DerpError::UnexpectedFrameType {
				expected: "ServerInfo",
				actual: frame.frame_type(),
			}),
		}
	}

	async fn read_frame_from_stream(stream: &mut Box<dyn AsyncReadWrite>) -> Result<DerpFrame> {
		let mut header = [0u8; 4];
		stream
			.read_exact(&mut header)
			.await
			.map_err(DerpError::Connection)?;

		let (_frame_type, payload_len) = decode_frame_header(&header);

		if payload_len > MAX_FRAME_SIZE {
			return Err(DerpError::FrameTooLarge(payload_len, MAX_FRAME_SIZE));
		}

		let mut payload = vec![0u8; payload_len];
		if payload_len > 0 {
			stream
				.read_exact(&mut payload)
				.await
				.map_err(DerpError::Connection)?;
		}

		let mut full_frame = Vec::with_capacity(4 + payload_len);
		full_frame.extend_from_slice(&header);
		full_frame.extend_from_slice(&payload);

		DerpFrame::decode(&full_frame)
	}

	pub async fn send(&mut self, dst: &WgPublicKey, data: &[u8]) -> Result<()> {
		let frame = DerpFrame::SendPacket {
			dst_key: *dst.as_bytes(),
			data: data.to_vec(),
		};
		self
			.stream
			.write_all(&frame.encode())
			.await
			.map_err(DerpError::Connection)
	}

	pub async fn recv(&mut self) -> Result<DerpFrame> {
		Self::read_frame_from_stream(&mut self.stream).await
	}

	pub async fn send_keepalive(&mut self) -> Result<()> {
		let frame = DerpFrame::KeepAlive;
		self
			.stream
			.write_all(&frame.encode())
			.await
			.map_err(DerpError::Connection)
	}

	pub async fn note_preferred(&mut self, preferred: bool) -> Result<()> {
		let frame = DerpFrame::NotePreferred { preferred };
		self
			.stream
			.write_all(&frame.encode())
			.await
			.map_err(DerpError::Connection)
	}

	pub async fn watch_conns(&mut self) -> Result<()> {
		let frame = DerpFrame::WatchConns;
		self
			.stream
			.write_all(&frame.encode())
			.await
			.map_err(DerpError::Connection)
	}

	pub async fn close_peer(&mut self, peer: &WgPublicKey) -> Result<()> {
		let frame = DerpFrame::ClosePeer {
			peer_key: *peer.as_bytes(),
		};
		self
			.stream
			.write_all(&frame.encode())
			.await
			.map_err(DerpError::Connection)
	}

	pub fn server_key(&self) -> &[u8; 32] {
		&self.server_key
	}

	pub fn home_region(&self) -> u16 {
		self.home_region
	}

	pub fn our_public_key(&self) -> &WgPublicKey {
		self.our_key.public_key()
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_trait_bounds() {
		fn assert_send<T: Send>() {}
		assert_send::<DerpClient>();
	}
}
