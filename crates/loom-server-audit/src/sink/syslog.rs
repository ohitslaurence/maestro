// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use std::io::Write;
use std::net::SocketAddr;
use std::sync::Arc;

use async_trait::async_trait;
use loom_server_config::{SyslogConfig, SyslogProtocol};
use tokio::io::AsyncWriteExt;
use tokio::net::{TcpStream, UdpSocket};
use tokio::sync::Mutex;

#[cfg(feature = "sink-syslog-tls")]
use rustls::ClientConfig as TlsClientConfig;
#[cfg(feature = "sink-syslog-tls")]
use rustls_pki_types::ServerName;
#[cfg(feature = "sink-syslog-tls")]
use std::sync::Arc as StdArc;
#[cfg(feature = "sink-syslog-tls")]
use tokio_rustls::client::TlsStream;
#[cfg(feature = "sink-syslog-tls")]
use tokio_rustls::TlsConnector;

use super::{AuditSink, AuditSinkError};
use crate::enrichment::EnrichedAuditEvent;
use crate::event::AuditSeverity;
use crate::filter::AuditFilterConfig;

/// Enum to hold different stream types for polymorphic handling.
enum SyslogStream {
	Tcp(TcpStream),
	#[cfg(feature = "sink-syslog-tls")]
	Tls(TlsStream<TcpStream>),
}

pub struct SyslogAuditSink {
	config: SyslogConfig,
	filter: AuditFilterConfig,
	udp_socket: Option<UdpSocket>,
	stream: Mutex<Option<SyslogStream>>,
	target_addr: SocketAddr,
	#[cfg(feature = "sink-syslog-tls")]
	tls_connector: Option<TlsConnector>,
}

impl SyslogAuditSink {
	pub async fn new(
		config: SyslogConfig,
		filter: AuditFilterConfig,
	) -> Result<Self, AuditSinkError> {
		let target_addr = format!("{}:{}", config.host, config.port)
			.parse()
			.map_err(|e| AuditSinkError::Permanent(format!("invalid syslog address: {e}")))?;

		let udp_socket = if config.protocol == SyslogProtocol::Udp {
			let socket = UdpSocket::bind("0.0.0.0:0")
				.await
				.map_err(|e| AuditSinkError::Transient(format!("failed to bind UDP socket: {e}")))?;
			Some(socket)
		} else {
			None
		};

		#[cfg(feature = "sink-syslog-tls")]
		let tls_connector = if config.protocol == SyslogProtocol::Tls {
			let root_store =
				rustls::RootCertStore::from_iter(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
			let tls_config = TlsClientConfig::builder()
				.with_root_certificates(root_store)
				.with_no_client_auth();
			Some(TlsConnector::from(StdArc::new(tls_config)))
		} else {
			None
		};

		#[cfg(not(feature = "sink-syslog-tls"))]
		if config.protocol == SyslogProtocol::Tls {
			return Err(AuditSinkError::Permanent(
				"TLS protocol requires the sink-syslog-tls feature".to_string(),
			));
		}

		Ok(Self {
			config,
			filter,
			udp_socket,
			stream: Mutex::new(None),
			target_addr,
			#[cfg(feature = "sink-syslog-tls")]
			tls_connector,
		})
	}

	async fn send_message(&self, message: &[u8]) -> Result<(), AuditSinkError> {
		match self.config.protocol {
			SyslogProtocol::Udp => self.send_udp(message).await,
			SyslogProtocol::Tcp => self.send_tcp(message).await,
			SyslogProtocol::Tls => self.send_tls(message).await,
		}
	}

	async fn send_udp(&self, message: &[u8]) -> Result<(), AuditSinkError> {
		let socket = self
			.udp_socket
			.as_ref()
			.ok_or_else(|| AuditSinkError::Permanent("UDP socket not initialized".to_string()))?;

		socket
			.send_to(message, self.target_addr)
			.await
			.map_err(|e| AuditSinkError::Transient(format!("failed to send UDP message: {e}")))?;

		Ok(())
	}

	async fn send_tcp(&self, message: &[u8]) -> Result<(), AuditSinkError> {
		let mut stream_guard = self.stream.lock().await;

		if stream_guard.is_none() {
			let stream = TcpStream::connect(self.target_addr)
				.await
				.map_err(|e| AuditSinkError::Transient(format!("TCP connect failed: {e}")))?;
			*stream_guard = Some(SyslogStream::Tcp(stream));
		}

		let framed_message = self.frame_message(message)?;

		match stream_guard.as_mut() {
			Some(SyslogStream::Tcp(stream)) => {
				if let Err(e) = stream.write_all(&framed_message).await {
					*stream_guard = None;
					return Err(AuditSinkError::Transient(format!(
						"TCP write failed (will reconnect): {e}"
					)));
				}
			}
			#[cfg(feature = "sink-syslog-tls")]
			Some(SyslogStream::Tls(_)) => {
				return Err(AuditSinkError::Permanent(
					"stream type mismatch: expected TCP".to_string(),
				));
			}
			None => unreachable!(),
		}

		Ok(())
	}

	#[cfg(feature = "sink-syslog-tls")]
	async fn send_tls(&self, message: &[u8]) -> Result<(), AuditSinkError> {
		let mut stream_guard = self.stream.lock().await;

		if stream_guard.is_none() {
			let connector = self
				.tls_connector
				.as_ref()
				.ok_or_else(|| AuditSinkError::Permanent("TLS connector not initialized".to_string()))?;

			let tcp_stream = TcpStream::connect(self.target_addr)
				.await
				.map_err(|e| AuditSinkError::Transient(format!("TCP connect failed: {e}")))?;

			let server_name = ServerName::try_from(self.config.host.clone())
				.map_err(|e| AuditSinkError::Permanent(format!("invalid server name: {e}")))?;

			let tls_stream = connector
				.connect(server_name, tcp_stream)
				.await
				.map_err(|e| AuditSinkError::Transient(format!("TLS handshake failed: {e}")))?;

			*stream_guard = Some(SyslogStream::Tls(tls_stream));
		}

		let framed_message = self.frame_message(message)?;

		match stream_guard.as_mut() {
			Some(SyslogStream::Tls(stream)) => {
				if let Err(e) = stream.write_all(&framed_message).await {
					*stream_guard = None;
					return Err(AuditSinkError::Transient(format!(
						"TLS write failed (will reconnect): {e}"
					)));
				}
			}
			Some(SyslogStream::Tcp(_)) => {
				return Err(AuditSinkError::Permanent(
					"stream type mismatch: expected TLS".to_string(),
				));
			}
			None => unreachable!(),
		}

		Ok(())
	}

	#[cfg(not(feature = "sink-syslog-tls"))]
	async fn send_tls(&self, _message: &[u8]) -> Result<(), AuditSinkError> {
		Err(AuditSinkError::Permanent(
			"TLS protocol requires the sink-syslog-tls feature".to_string(),
		))
	}

	fn frame_message(&self, message: &[u8]) -> Result<Vec<u8>, AuditSinkError> {
		let mut framed_message = Vec::with_capacity(message.len() + 10);
		write!(&mut framed_message, "{} ", message.len())
			.map_err(|e| AuditSinkError::Transient(format!("framing error: {e}")))?;
		framed_message.extend_from_slice(message);
		Ok(framed_message)
	}
}

#[async_trait]
impl AuditSink for SyslogAuditSink {
	fn name(&self) -> &str {
		"syslog"
	}

	fn filter(&self) -> &AuditFilterConfig {
		&self.filter
	}

	async fn publish(&self, event: Arc<EnrichedAuditEvent>) -> Result<(), AuditSinkError> {
		let message = if self.config.use_cef {
			format_cef(&event, &self.config.app_name)
		} else {
			format_rfc5424(&event, &self.config.facility, &self.config.app_name)
		};

		self.send_message(message.as_bytes()).await
	}

	async fn health_check(&self) -> Result<(), AuditSinkError> {
		match self.config.protocol {
			SyslogProtocol::Udp => Ok(()),
			SyslogProtocol::Tcp => {
				let mut stream_guard = self.stream.lock().await;
				if stream_guard.is_none() {
					let stream = TcpStream::connect(self.target_addr)
						.await
						.map_err(|e| AuditSinkError::Transient(format!("TCP connect failed: {e}")))?;
					*stream_guard = Some(SyslogStream::Tcp(stream));
				}
				Ok(())
			}
			SyslogProtocol::Tls => {
				#[cfg(feature = "sink-syslog-tls")]
				{
					let mut stream_guard = self.stream.lock().await;
					if stream_guard.is_none() {
						let connector = self.tls_connector.as_ref().ok_or_else(|| {
							AuditSinkError::Permanent("TLS connector not initialized".to_string())
						})?;

						let tcp_stream = TcpStream::connect(self.target_addr)
							.await
							.map_err(|e| AuditSinkError::Transient(format!("TCP connect failed: {e}")))?;

						let server_name = ServerName::try_from(self.config.host.clone())
							.map_err(|e| AuditSinkError::Permanent(format!("invalid server name: {e}")))?;

						let tls_stream = connector
							.connect(server_name, tcp_stream)
							.await
							.map_err(|e| AuditSinkError::Transient(format!("TLS handshake failed: {e}")))?;

						*stream_guard = Some(SyslogStream::Tls(tls_stream));
					}
					Ok(())
				}
				#[cfg(not(feature = "sink-syslog-tls"))]
				{
					Err(AuditSinkError::Permanent(
						"TLS protocol requires the sink-syslog-tls feature".to_string(),
					))
				}
			}
		}
	}
}

pub fn format_rfc5424(event: &EnrichedAuditEvent, facility: &str, app_name: &str) -> String {
	let base = &event.base;

	let facility_code = facility_to_code(facility);
	let severity_code = severity_to_syslog(base.severity);
	let pri = (facility_code * 8) + severity_code;

	let timestamp = base.timestamp.format("%Y-%m-%dT%H:%M:%S%.3fZ");

	let hostname = std::env::var("HOSTNAME").unwrap_or_else(|_| "-".to_string());

	let procid = std::process::id();

	let msgid = base.event_type.to_string().to_uppercase().replace('_', "-");

	let mut sd_params = Vec::new();
	sd_params.push(format!("event_type=\"{}\"", base.event_type));
	sd_params.push(format!("severity=\"{}\"", base.severity));

	if let Some(actor) = &base.actor_user_id {
		sd_params.push(format!("actor=\"{actor}\""));
	}
	if let Some(ip) = &base.ip_address {
		sd_params.push(format!("ip=\"{}\"", escape_sd_value(ip)));
	}
	if let Some(resource_type) = &base.resource_type {
		sd_params.push(format!("resource_type=\"{resource_type}\""));
	}
	if let Some(resource_id) = &base.resource_id {
		sd_params.push(format!("resource_id=\"{}\"", escape_sd_value(resource_id)));
	}
	if let Some(trace_id) = &base.trace_id {
		sd_params.push(format!("trace_id=\"{trace_id}\""));
	}
	if let Some(request_id) = &base.request_id {
		sd_params.push(format!("request_id=\"{request_id}\""));
	}

	if let Some(session) = &event.session {
		if let Some(session_id) = &session.session_id {
			sd_params.push(format!("session_id=\"{session_id}\""));
		}
		if let Some(session_type) = &session.session_type {
			sd_params.push(format!("session_type=\"{session_type}\""));
		}
		if let Some(geo) = &session.geo {
			if let Some(country) = &geo.country_code {
				sd_params.push(format!("geo_country=\"{country}\""));
			}
		}
	}

	if let Some(org) = &event.org {
		if let Some(org_id) = &org.org_id {
			sd_params.push(format!("org_id=\"{org_id}\""));
		}
		if let Some(org_slug) = &org.org_slug {
			sd_params.push(format!("org_slug=\"{}\"", escape_sd_value(org_slug)));
		}
	}

	let structured_data = format!("[audit@loom {}]", sd_params.join(" "));

	format!(
		"<{pri}>1 {timestamp} {hostname} {app_name} {procid} {msgid} {structured_data} {action}",
		action = base.action
	)
}

pub fn format_cef(event: &EnrichedAuditEvent, app_name: &str) -> String {
	let base = &event.base;

	let signature_id = base.event_type.to_string().to_uppercase();
	let event_name = base.event_type.to_string().to_uppercase().replace('_', " ");

	let cef_severity = match base.severity {
		AuditSeverity::Debug => 1,
		AuditSeverity::Info => 1,
		AuditSeverity::Notice => 3,
		AuditSeverity::Warning => 5,
		AuditSeverity::Error => 7,
		AuditSeverity::Critical => 10,
	};

	let mut extensions = Vec::new();
	extensions.push(format!("cs1Label=event_type cs1={}", base.event_type));

	if let Some(ip) = &base.ip_address {
		extensions.push(format!("src={}", escape_cef_value(ip)));
	}
	if let Some(actor) = &base.actor_user_id {
		extensions.push(format!("suser={actor}"));
	}
	if let Some(resource_id) = &base.resource_id {
		extensions.push(format!(
			"cs2Label=resource_id cs2={}",
			escape_cef_value(resource_id)
		));
	}
	if let Some(request_id) = &base.request_id {
		extensions.push(format!("cs3Label=request_id cs3={request_id}"));
	}
	if let Some(trace_id) = &base.trace_id {
		extensions.push(format!("cs4Label=trace_id cs4={trace_id}"));
	}

	if let Some(org) = &event.org {
		if let Some(org_id) = &org.org_id {
			extensions.push(format!("cs5Label=org_id cs5={org_id}"));
		}
	}

	format!(
		"CEF:0|Loom|{app_name}|1.0|{signature_id}|{event_name}|{cef_severity}|{}",
		extensions.join(" ")
	)
}

pub fn severity_to_syslog(severity: AuditSeverity) -> u8 {
	match severity {
		AuditSeverity::Debug => 7,
		AuditSeverity::Info => 6,
		AuditSeverity::Notice => 5,
		AuditSeverity::Warning => 4,
		AuditSeverity::Error => 3,
		AuditSeverity::Critical => 2,
	}
}

pub fn facility_to_code(facility: &str) -> u8 {
	match facility.to_lowercase().as_str() {
		"kern" => 0,
		"user" => 1,
		"mail" => 2,
		"daemon" => 3,
		"auth" | "security" => 4,
		"syslog" => 5,
		"lpr" => 6,
		"news" => 7,
		"uucp" => 8,
		"cron" => 9,
		"authpriv" => 10,
		"ftp" => 11,
		"ntp" => 12,
		"audit" => 13,
		"alert" => 14,
		"clock" => 15,
		"local0" => 16,
		"local1" => 17,
		"local2" => 18,
		"local3" => 19,
		"local4" => 20,
		"local5" => 21,
		"local6" => 22,
		"local7" => 23,
		_ => 16,
	}
}

fn escape_sd_value(s: &str) -> String {
	s.replace('\\', "\\\\")
		.replace('"', "\\\"")
		.replace(']', "\\]")
}

fn escape_cef_value(s: &str) -> String {
	s.replace('\\', "\\\\")
		.replace('=', "\\=")
		.replace('\n', "\\n")
		.replace('\r', "\\r")
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::enrichment::{OrgContext, SessionContext};
	use crate::event::{AuditEventType, AuditLogEntry};

	fn make_test_event() -> EnrichedAuditEvent {
		EnrichedAuditEvent {
			base: AuditLogEntry::builder(AuditEventType::LoginFailed)
				.ip_address("192.168.1.1")
				.action("User login failed: invalid credentials")
				.resource("session", "sess-123")
				.trace_id("trace-abc")
				.build(),
			session: Some(SessionContext {
				session_id: Some("sid-123".to_string()),
				session_type: Some("web".to_string()),
				device_label: None,
				geo: None,
			}),
			org: Some(OrgContext {
				org_id: Some("org-456".to_string()),
				org_slug: Some("acme-corp".to_string()),
				org_role: None,
				team_id: None,
				team_role: None,
			}),
		}
	}

	#[test]
	fn test_severity_to_syslog() {
		assert_eq!(severity_to_syslog(AuditSeverity::Critical), 2);
		assert_eq!(severity_to_syslog(AuditSeverity::Error), 3);
		assert_eq!(severity_to_syslog(AuditSeverity::Warning), 4);
		assert_eq!(severity_to_syslog(AuditSeverity::Notice), 5);
		assert_eq!(severity_to_syslog(AuditSeverity::Info), 6);
		assert_eq!(severity_to_syslog(AuditSeverity::Debug), 7);
	}

	#[test]
	fn test_facility_to_code() {
		assert_eq!(facility_to_code("kern"), 0);
		assert_eq!(facility_to_code("auth"), 4);
		assert_eq!(facility_to_code("security"), 4);
		assert_eq!(facility_to_code("local0"), 16);
		assert_eq!(facility_to_code("local7"), 23);
		assert_eq!(facility_to_code("LOCAL0"), 16);
		assert_eq!(facility_to_code("unknown"), 16);
	}

	#[test]
	fn test_format_rfc5424_structure() {
		let event = make_test_event();
		let message = format_rfc5424(&event, "local0", "loom");

		assert!(message.starts_with('<'));
		assert!(message.contains(">1 "));
		assert!(message.contains(" loom "));
		assert!(message.contains("LOGIN-FAILED"));
		assert!(message.contains("[audit@loom"));
		assert!(message.contains("event_type=\"login_failed\""));
		assert!(message.contains("severity=\"warning\""));
		assert!(message.contains("ip=\"192.168.1.1\""));
		assert!(message.contains("resource_type=\"session\""));
		assert!(message.contains("User login failed"));
	}

	#[test]
	fn test_format_rfc5424_pri_calculation() {
		let event = make_test_event();

		let message_local0 = format_rfc5424(&event, "local0", "loom");
		let pri_local0_warning = (16 * 8) + 4;
		assert!(message_local0.starts_with(&format!("<{pri_local0_warning}>")));

		let message_auth = format_rfc5424(&event, "auth", "loom");
		let pri_auth_warning = (4 * 8) + 4;
		assert!(message_auth.starts_with(&format!("<{pri_auth_warning}>")));
	}

	#[test]
	fn test_format_cef_structure() {
		let event = make_test_event();
		let message = format_cef(&event, "Loom Server");

		assert!(message.starts_with("CEF:0|Loom|Loom Server|1.0|"));
		assert!(message.contains("|LOGIN_FAILED|"));
		assert!(message.contains("|LOGIN FAILED|"));
		assert!(message.contains("|5|"));
		assert!(message.contains("src=192.168.1.1"));
		assert!(message.contains("cs1Label=event_type cs1=login_failed"));
	}

	#[test]
	fn test_format_cef_severity_mapping() {
		let mut event = make_test_event();
		event.base = AuditLogEntry::builder(AuditEventType::LlmRequestFailed)
			.severity(AuditSeverity::Error)
			.build();
		let message = format_cef(&event, "loom");
		assert!(message.contains("|7|"));

		event.base = AuditLogEntry::builder(AuditEventType::Login)
			.severity(AuditSeverity::Info)
			.build();
		let message = format_cef(&event, "loom");
		assert!(message.contains("|1|"));

		event.base = AuditLogEntry::builder(AuditEventType::AccessDenied)
			.severity(AuditSeverity::Critical)
			.build();
		let message = format_cef(&event, "loom");
		assert!(message.contains("|10|"));
	}

	#[test]
	fn test_escape_sd_value() {
		assert_eq!(escape_sd_value(r#"test"value"#), r#"test\"value"#);
		assert_eq!(escape_sd_value(r"test\value"), r"test\\value");
		assert_eq!(escape_sd_value("test]value"), r"test\]value");
		assert_eq!(escape_sd_value("normal"), "normal");
	}

	#[test]
	fn test_escape_cef_value() {
		assert_eq!(escape_cef_value("test=value"), r"test\=value");
		assert_eq!(escape_cef_value(r"test\value"), r"test\\value");
		assert_eq!(escape_cef_value("line1\nline2"), r"line1\nline2");
		assert_eq!(escape_cef_value("normal"), "normal");
	}

	#[test]
	fn test_syslog_protocol_serde() {
		let udp: SyslogProtocol = serde_json::from_str(r#""udp""#).unwrap();
		assert_eq!(udp, SyslogProtocol::Udp);

		let tcp: SyslogProtocol = serde_json::from_str(r#""tcp""#).unwrap();
		assert_eq!(tcp, SyslogProtocol::Tcp);

		let json = serde_json::to_string(&SyslogProtocol::Tcp).unwrap();
		assert_eq!(json, r#""tcp""#);
	}

	#[test]
	fn test_format_rfc5424_with_org_context() {
		let event = make_test_event();
		let message = format_rfc5424(&event, "local0", "loom");

		assert!(message.contains("org_id=\"org-456\""));
		assert!(message.contains("org_slug=\"acme-corp\""));
	}

	#[test]
	fn test_format_rfc5424_minimal_event() {
		let event = EnrichedAuditEvent {
			base: AuditLogEntry::builder(AuditEventType::Login).build(),
			session: None,
			org: None,
		};
		let message = format_rfc5424(&event, "auth", "loom");

		assert!(message.starts_with('<'));
		assert!(message.contains("[audit@loom"));
		assert!(message.contains("event_type=\"login\""));
		assert!(message.contains("severity=\"info\""));
	}
}
