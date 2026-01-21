// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

#![cfg(feature = "sink-json-stream")]

use std::sync::Arc;

use async_trait::async_trait;
use loom_server_config::{JsonStreamConfig, StreamProtocol};
use tokio::io::AsyncWriteExt;
use tokio::net::{TcpStream, UdpSocket};
use tokio::sync::Mutex;

#[cfg(feature = "sink-json-stream-tls")]
use rustls::ClientConfig as TlsClientConfig;
#[cfg(feature = "sink-json-stream-tls")]
use rustls_pki_types::ServerName;
#[cfg(feature = "sink-json-stream-tls")]
use std::sync::Arc as StdArc;
#[cfg(feature = "sink-json-stream-tls")]
use tokio_rustls::client::TlsStream;
#[cfg(feature = "sink-json-stream-tls")]
use tokio_rustls::TlsConnector;

use crate::enrichment::EnrichedAuditEvent;
use crate::error::AuditSinkError;
use crate::filter::AuditFilterConfig;
use crate::sink::AuditSink;

/// Enum to hold different stream types for polymorphic handling.
enum JsonStream {
	Tcp(TcpStream),
	#[cfg(feature = "sink-json-stream-tls")]
	Tls(TlsStream<TcpStream>),
}

pub struct JsonStreamSink {
	config: JsonStreamConfig,
	filter: AuditFilterConfig,
	stream: Mutex<Option<JsonStream>>,
	udp_socket: Mutex<Option<UdpSocket>>,
	#[cfg(feature = "sink-json-stream-tls")]
	tls_connector: Option<TlsConnector>,
}

impl JsonStreamSink {
	pub fn new(config: JsonStreamConfig, filter: AuditFilterConfig) -> Result<Self, AuditSinkError> {
		#[cfg(feature = "sink-json-stream-tls")]
		let tls_connector = if config.protocol == StreamProtocol::Tls {
			let root_store =
				rustls::RootCertStore::from_iter(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
			let tls_config = TlsClientConfig::builder()
				.with_root_certificates(root_store)
				.with_no_client_auth();
			Some(TlsConnector::from(StdArc::new(tls_config)))
		} else {
			None
		};

		#[cfg(not(feature = "sink-json-stream-tls"))]
		if config.protocol == StreamProtocol::Tls {
			return Err(AuditSinkError::Permanent(
				"TLS protocol requires the sink-json-stream-tls feature".to_string(),
			));
		}

		Ok(Self {
			config,
			filter,
			stream: Mutex::new(None),
			udp_socket: Mutex::new(None),
			#[cfg(feature = "sink-json-stream-tls")]
			tls_connector,
		})
	}

	fn address(&self) -> String {
		format!("{}:{}", self.config.host, self.config.port)
	}

	async fn connect_tcp(&self) -> Result<TcpStream, AuditSinkError> {
		TcpStream::connect(self.address())
			.await
			.map_err(|e| AuditSinkError::Transient(format!("TCP connect failed: {e}")))
	}

	async fn get_or_connect_tcp(&self) -> Result<(), AuditSinkError> {
		let mut guard = self.stream.lock().await;
		if guard.is_none() {
			*guard = Some(JsonStream::Tcp(self.connect_tcp().await?));
		}
		Ok(())
	}

	#[cfg(feature = "sink-json-stream-tls")]
	async fn get_or_connect_tls(&self) -> Result<(), AuditSinkError> {
		let mut guard = self.stream.lock().await;
		if guard.is_none() {
			let connector = self
				.tls_connector
				.as_ref()
				.ok_or_else(|| AuditSinkError::Permanent("TLS connector not initialized".to_string()))?;

			let tcp_stream = self.connect_tcp().await?;

			let server_name = ServerName::try_from(self.config.host.clone())
				.map_err(|e| AuditSinkError::Permanent(format!("invalid server name: {e}")))?;

			let tls_stream = connector
				.connect(server_name, tcp_stream)
				.await
				.map_err(|e| AuditSinkError::Transient(format!("TLS handshake failed: {e}")))?;

			*guard = Some(JsonStream::Tls(tls_stream));
		}
		Ok(())
	}

	async fn send_tcp(&self, data: &[u8]) -> Result<(), AuditSinkError> {
		let mut guard = self.stream.lock().await;

		if guard.is_none() {
			*guard = Some(JsonStream::Tcp(self.connect_tcp().await?));
		}

		match guard.as_mut() {
			Some(JsonStream::Tcp(stream)) => match stream.write_all(data).await {
				Ok(()) => Ok(()),
				Err(e) => {
					*guard = None;
					Err(AuditSinkError::Transient(format!("TCP write failed: {e}")))
				}
			},
			#[cfg(feature = "sink-json-stream-tls")]
			Some(JsonStream::Tls(_)) => Err(AuditSinkError::Permanent(
				"stream type mismatch: expected TCP".to_string(),
			)),
			None => unreachable!(),
		}
	}

	#[cfg(feature = "sink-json-stream-tls")]
	async fn send_tls(&self, data: &[u8]) -> Result<(), AuditSinkError> {
		let mut guard = self.stream.lock().await;

		if guard.is_none() {
			let connector = self
				.tls_connector
				.as_ref()
				.ok_or_else(|| AuditSinkError::Permanent("TLS connector not initialized".to_string()))?;

			let tcp_stream = self.connect_tcp().await?;

			let server_name = ServerName::try_from(self.config.host.clone())
				.map_err(|e| AuditSinkError::Permanent(format!("invalid server name: {e}")))?;

			let tls_stream = connector
				.connect(server_name, tcp_stream)
				.await
				.map_err(|e| AuditSinkError::Transient(format!("TLS handshake failed: {e}")))?;

			*guard = Some(JsonStream::Tls(tls_stream));
		}

		match guard.as_mut() {
			Some(JsonStream::Tls(stream)) => match stream.write_all(data).await {
				Ok(()) => Ok(()),
				Err(e) => {
					*guard = None;
					Err(AuditSinkError::Transient(format!("TLS write failed: {e}")))
				}
			},
			Some(JsonStream::Tcp(_)) => Err(AuditSinkError::Permanent(
				"stream type mismatch: expected TLS".to_string(),
			)),
			None => unreachable!(),
		}
	}

	#[cfg(not(feature = "sink-json-stream-tls"))]
	async fn send_tls(&self, _data: &[u8]) -> Result<(), AuditSinkError> {
		Err(AuditSinkError::Permanent(
			"TLS protocol requires the sink-json-stream-tls feature".to_string(),
		))
	}

	async fn send_udp(&self, data: &[u8]) -> Result<(), AuditSinkError> {
		let mut guard = self.udp_socket.lock().await;

		if guard.is_none() {
			let socket = UdpSocket::bind("0.0.0.0:0")
				.await
				.map_err(|e| AuditSinkError::Transient(format!("UDP bind failed: {e}")))?;
			socket
				.connect(self.address())
				.await
				.map_err(|e| AuditSinkError::Transient(format!("UDP connect failed: {e}")))?;
			*guard = Some(socket);
		}

		let socket = guard.as_ref().unwrap();
		socket
			.send(data)
			.await
			.map_err(|e| AuditSinkError::Transient(format!("UDP send failed: {e}")))?;
		Ok(())
	}
}

#[async_trait]
impl AuditSink for JsonStreamSink {
	fn name(&self) -> &str {
		&self.config.name
	}

	fn filter(&self) -> &AuditFilterConfig {
		&self.filter
	}

	async fn publish(&self, event: Arc<EnrichedAuditEvent>) -> Result<(), AuditSinkError> {
		let mut json = serde_json::to_string(event.as_ref())
			.map_err(|e| AuditSinkError::Permanent(format!("JSON serialization failed: {e}")))?;
		json.push('\n');
		let data = json.as_bytes();

		match self.config.protocol {
			StreamProtocol::Tcp => self.send_tcp(data).await,
			StreamProtocol::Udp => self.send_udp(data).await,
			StreamProtocol::Tls => self.send_tls(data).await,
		}
	}

	async fn health_check(&self) -> Result<(), AuditSinkError> {
		match self.config.protocol {
			StreamProtocol::Tcp => {
				self.get_or_connect_tcp().await?;
				Ok(())
			}
			StreamProtocol::Udp => {
				let socket = UdpSocket::bind("0.0.0.0:0")
					.await
					.map_err(|e| AuditSinkError::Transient(format!("UDP bind failed: {e}")))?;
				socket
					.connect(self.address())
					.await
					.map_err(|e| AuditSinkError::Transient(format!("UDP connect failed: {e}")))?;
				socket
					.send(b"")
					.await
					.map_err(|e| AuditSinkError::Transient(format!("UDP ping failed: {e}")))?;
				Ok(())
			}
			StreamProtocol::Tls => {
				#[cfg(feature = "sink-json-stream-tls")]
				{
					self.get_or_connect_tls().await?;
					Ok(())
				}
				#[cfg(not(feature = "sink-json-stream-tls"))]
				{
					Err(AuditSinkError::Permanent(
						"TLS protocol requires the sink-json-stream-tls feature".to_string(),
					))
				}
			}
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::enrichment::EnrichedAuditEvent;
	use crate::event::{AuditEventType, AuditLogEntry, AuditSeverity};

	fn make_config(protocol: StreamProtocol) -> JsonStreamConfig {
		JsonStreamConfig {
			name: "test-stream".to_string(),
			host: "127.0.0.1".to_string(),
			port: 9999,
			protocol,
			min_severity: "info".to_string(),
		}
	}

	fn make_event() -> EnrichedAuditEvent {
		EnrichedAuditEvent {
			base: AuditLogEntry::builder(AuditEventType::Login)
				.severity(AuditSeverity::Info)
				.build(),
			session: None,
			org: None,
		}
	}

	#[test]
	fn test_stream_protocol_serde() {
		let tcp_json = serde_json::to_string(&StreamProtocol::Tcp).unwrap();
		assert_eq!(tcp_json, "\"tcp\"");

		let udp_json = serde_json::to_string(&StreamProtocol::Udp).unwrap();
		assert_eq!(udp_json, "\"udp\"");

		let tcp: StreamProtocol = serde_json::from_str("\"tcp\"").unwrap();
		assert_eq!(tcp, StreamProtocol::Tcp);

		let udp: StreamProtocol = serde_json::from_str("\"udp\"").unwrap();
		assert_eq!(udp, StreamProtocol::Udp);
	}

	#[test]
	fn test_json_stream_config_serde() {
		let config = make_config(StreamProtocol::Tcp);
		let json = serde_json::to_string(&config).unwrap();
		let parsed: JsonStreamConfig = serde_json::from_str(&json).unwrap();

		assert_eq!(parsed.name, config.name);
		assert_eq!(parsed.host, config.host);
		assert_eq!(parsed.port, config.port);
		assert_eq!(parsed.protocol, config.protocol);
	}

	#[test]
	fn test_sink_name_and_filter() {
		let config = make_config(StreamProtocol::Udp);
		let filter = AuditFilterConfig::default();
		let sink = JsonStreamSink::new(config, filter).unwrap();

		assert_eq!(sink.name(), "test-stream");
		assert_eq!(sink.filter().min_severity, AuditSeverity::Info);
	}

	#[test]
	fn test_address_formatting() {
		let config = JsonStreamConfig {
			name: "test".to_string(),
			host: "logstash.example.com".to_string(),
			port: 5044,
			protocol: StreamProtocol::Tcp,
			min_severity: "info".to_string(),
		};
		let sink = JsonStreamSink::new(config, AuditFilterConfig::default()).unwrap();
		assert_eq!(sink.address(), "logstash.example.com:5044");
	}

	#[tokio::test]
	async fn test_event_serialization_has_newline() {
		let event = make_event();
		let mut json = serde_json::to_string(&event).unwrap();
		json.push('\n');

		assert!(json.ends_with('\n'));
		assert!(json.contains("\"event_type\""));
		assert!(json.contains("\"severity\""));
	}

	#[tokio::test]
	async fn test_tcp_connect_fails_gracefully() {
		let config = make_config(StreamProtocol::Tcp);
		let sink = JsonStreamSink::new(config, AuditFilterConfig::default()).unwrap();
		let event = Arc::new(make_event());

		let result = sink.publish(event).await;
		assert!(result.is_err());

		if let Err(AuditSinkError::Transient(msg)) = result {
			assert!(msg.contains("TCP"));
		} else {
			panic!("Expected transient TCP error");
		}
	}

	#[tokio::test]
	async fn test_health_check_tcp_fails_gracefully() {
		let config = make_config(StreamProtocol::Tcp);
		let sink = JsonStreamSink::new(config, AuditFilterConfig::default()).unwrap();

		let result = sink.health_check().await;
		assert!(result.is_err());
	}

	#[test]
	fn test_tls_without_feature_fails() {
		#[cfg(not(feature = "sink-json-stream-tls"))]
		{
			let config = make_config(StreamProtocol::Tls);
			let result = JsonStreamSink::new(config, AuditFilterConfig::default());
			assert!(result.is_err());
		}
	}
}
