// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! Audit logging configuration section.

use serde::{Deserialize, Serialize};

const DEFAULT_QUEUE_CAPACITY: usize = 10000;
const DEFAULT_RETENTION_DAYS: i64 = 90;

fn default_queue_capacity() -> usize {
	DEFAULT_QUEUE_CAPACITY
}

fn default_retention_days() -> i64 {
	DEFAULT_RETENTION_DAYS
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum QueueOverflowPolicy {
	#[default]
	DropNewest,
	DropOldest,
	Block,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct AuditConfigLayer {
	pub enabled: Option<bool>,
	pub retention_days: Option<i64>,
	pub queue_capacity: Option<usize>,
	pub queue_overflow_policy: Option<QueueOverflowPolicy>,
	pub min_severity: Option<String>,
	pub syslog: Option<SyslogConfigLayer>,
	pub http_sinks: Option<Vec<HttpSinkConfigLayer>>,
	pub json_stream_sinks: Option<Vec<JsonStreamConfigLayer>>,
	pub file_sinks: Option<Vec<FileSinkConfigLayer>>,
}

impl AuditConfigLayer {
	pub fn merge(&mut self, other: Self) {
		if other.enabled.is_some() {
			self.enabled = other.enabled;
		}
		if other.retention_days.is_some() {
			self.retention_days = other.retention_days;
		}
		if other.queue_capacity.is_some() {
			self.queue_capacity = other.queue_capacity;
		}
		if other.queue_overflow_policy.is_some() {
			self.queue_overflow_policy = other.queue_overflow_policy;
		}
		if other.min_severity.is_some() {
			self.min_severity = other.min_severity;
		}
		if other.syslog.is_some() {
			self.syslog = other.syslog;
		}
		if other.http_sinks.is_some() {
			self.http_sinks = other.http_sinks;
		}
		if other.json_stream_sinks.is_some() {
			self.json_stream_sinks = other.json_stream_sinks;
		}
		if other.file_sinks.is_some() {
			self.file_sinks = other.file_sinks;
		}
	}

	pub fn finalize(self) -> AuditConfig {
		let syslog = self.syslog.and_then(|l| l.finalize());
		let http_sinks = self
			.http_sinks
			.map(|sinks| sinks.into_iter().filter_map(|s| s.finalize()).collect())
			.unwrap_or_default();
		let json_stream_sinks = self
			.json_stream_sinks
			.map(|sinks| sinks.into_iter().filter_map(|s| s.finalize()).collect())
			.unwrap_or_default();
		let file_sinks = self
			.file_sinks
			.map(|sinks| sinks.into_iter().filter_map(|s| s.finalize()).collect())
			.unwrap_or_default();

		AuditConfig {
			enabled: self.enabled.unwrap_or(true),
			retention_days: self.retention_days.unwrap_or(default_retention_days()),
			queue_capacity: self.queue_capacity.unwrap_or_else(default_queue_capacity),
			queue_overflow_policy: self.queue_overflow_policy.unwrap_or_default(),
			min_severity: self.min_severity.unwrap_or_else(|| "info".to_string()),
			syslog,
			http_sinks,
			json_stream_sinks,
			file_sinks,
		}
	}
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AuditConfig {
	pub enabled: bool,
	pub retention_days: i64,
	pub queue_capacity: usize,
	pub queue_overflow_policy: QueueOverflowPolicy,
	pub min_severity: String,
	pub syslog: Option<SyslogConfig>,
	pub http_sinks: Vec<HttpSinkConfig>,
	pub json_stream_sinks: Vec<JsonStreamConfig>,
	pub file_sinks: Vec<FileSinkConfig>,
}

impl Default for AuditConfig {
	fn default() -> Self {
		Self {
			enabled: true,
			retention_days: default_retention_days(),
			queue_capacity: default_queue_capacity(),
			queue_overflow_policy: QueueOverflowPolicy::default(),
			min_severity: "info".to_string(),
			syslog: None,
			http_sinks: Vec::new(),
			json_stream_sinks: Vec::new(),
			file_sinks: Vec::new(),
		}
	}
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum SyslogProtocol {
	#[default]
	Udp,
	Tcp,
	/// TCP with TLS encryption (requires sink-syslog-tls feature in loom-server-audit)
	Tls,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct SyslogConfigLayer {
	pub enabled: Option<bool>,
	pub host: Option<String>,
	pub port: Option<u16>,
	pub protocol: Option<SyslogProtocol>,
	pub facility: Option<String>,
	pub app_name: Option<String>,
	pub use_cef: Option<bool>,
}

impl SyslogConfigLayer {
	pub fn finalize(self) -> Option<SyslogConfig> {
		if !self.enabled.unwrap_or(false) {
			return None;
		}

		Some(SyslogConfig {
			host: self.host.unwrap_or_else(|| "localhost".to_string()),
			port: self.port.unwrap_or(514),
			protocol: self.protocol.unwrap_or_default(),
			facility: self.facility.unwrap_or_else(|| "local0".to_string()),
			app_name: self.app_name.unwrap_or_else(|| "loom".to_string()),
			use_cef: self.use_cef.unwrap_or(false),
		})
	}
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SyslogConfig {
	pub host: String,
	pub port: u16,
	pub protocol: SyslogProtocol,
	pub facility: String,
	pub app_name: String,
	pub use_cef: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct HttpSinkConfigLayer {
	pub name: Option<String>,
	pub url: Option<String>,
	pub method: Option<String>,
	pub headers: Option<Vec<(String, String)>>,
	pub timeout_ms: Option<u64>,
	pub retry_max_attempts: Option<u32>,
	pub min_severity: Option<String>,
}

impl HttpSinkConfigLayer {
	pub fn finalize(self) -> Option<HttpSinkConfig> {
		let name = self.name?;
		let url = self.url?;

		Some(HttpSinkConfig {
			name,
			url,
			method: self.method.unwrap_or_else(|| "POST".to_string()),
			headers: self.headers.unwrap_or_default(),
			timeout_ms: self.timeout_ms.unwrap_or(5000),
			retry_max_attempts: self.retry_max_attempts.unwrap_or(3),
			min_severity: self.min_severity.unwrap_or_else(|| "info".to_string()),
		})
	}
}

/// HTTP sink configuration.
///
/// # Security Note
///
/// This struct intentionally does NOT derive Debug because `headers` may contain
/// API keys, tokens, or other secrets. Use the manual Debug impl which redacts headers.
#[derive(Clone, Serialize, Deserialize, PartialEq)]
pub struct HttpSinkConfig {
	pub name: String,
	pub url: String,
	pub method: String,
	/// Headers to send with each request. May contain secrets (API keys, tokens).
	/// These are redacted in Debug output.
	pub headers: Vec<(String, String)>,
	pub timeout_ms: u64,
	pub retry_max_attempts: u32,
	pub min_severity: String,
}

impl std::fmt::Debug for HttpSinkConfig {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.debug_struct("HttpSinkConfig")
			.field("name", &self.name)
			.field("url", &self.url)
			.field("method", &self.method)
			.field(
				"headers",
				&format!("[{} header(s) REDACTED]", self.headers.len()),
			)
			.field("timeout_ms", &self.timeout_ms)
			.field("retry_max_attempts", &self.retry_max_attempts)
			.field("min_severity", &self.min_severity)
			.finish()
	}
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StreamProtocol {
	Tcp,
	Udp,
	/// TCP with TLS encryption (requires sink-json-stream-tls feature in loom-server-audit)
	Tls,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct JsonStreamConfigLayer {
	pub name: Option<String>,
	pub host: Option<String>,
	pub port: Option<u16>,
	pub protocol: Option<StreamProtocol>,
	pub min_severity: Option<String>,
}

impl JsonStreamConfigLayer {
	pub fn finalize(self) -> Option<JsonStreamConfig> {
		let name = self.name?;
		let host = self.host?;
		let port = self.port?;
		let protocol = self.protocol?;

		Some(JsonStreamConfig {
			name,
			host,
			port,
			protocol,
			min_severity: self.min_severity.unwrap_or_else(|| "info".to_string()),
		})
	}
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct JsonStreamConfig {
	pub name: String,
	pub host: String,
	pub port: u16,
	pub protocol: StreamProtocol,
	pub min_severity: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum FileFormat {
	#[default]
	JsonLines,
	Cef,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct FileSinkConfigLayer {
	pub path: Option<String>,
	pub format: Option<FileFormat>,
	pub min_severity: Option<String>,
}

impl FileSinkConfigLayer {
	pub fn finalize(self) -> Option<FileSinkConfig> {
		let path = self.path?;

		Some(FileSinkConfig {
			path,
			format: self.format.unwrap_or_default(),
			min_severity: self.min_severity.unwrap_or_else(|| "info".to_string()),
		})
	}
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FileSinkConfig {
	pub path: String,
	pub format: FileFormat,
	pub min_severity: String,
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_default_values() {
		let config = AuditConfig::default();
		assert!(config.enabled);
		assert_eq!(config.retention_days, 90);
		assert_eq!(config.queue_capacity, 10000);
		assert_eq!(
			config.queue_overflow_policy,
			QueueOverflowPolicy::DropNewest
		);
		assert_eq!(config.min_severity, "info");
		assert!(config.syslog.is_none());
		assert!(config.http_sinks.is_empty());
	}

	#[test]
	fn test_layer_finalize_defaults() {
		let layer = AuditConfigLayer::default();
		let config = layer.finalize();
		assert!(config.enabled);
		assert_eq!(config.retention_days, 90);
		assert_eq!(config.queue_capacity, 10000);
	}

	#[test]
	fn test_layer_finalize_with_values() {
		let layer = AuditConfigLayer {
			enabled: Some(false),
			retention_days: Some(30),
			queue_capacity: Some(5000),
			queue_overflow_policy: Some(QueueOverflowPolicy::Block),
			min_severity: Some("warning".to_string()),
			..Default::default()
		};
		let config = layer.finalize();
		assert!(!config.enabled);
		assert_eq!(config.retention_days, 30);
		assert_eq!(config.queue_capacity, 5000);
		assert_eq!(config.queue_overflow_policy, QueueOverflowPolicy::Block);
		assert_eq!(config.min_severity, "warning");
	}

	#[test]
	fn test_merge_overwrites() {
		let mut base = AuditConfigLayer {
			enabled: Some(true),
			retention_days: Some(90),
			..Default::default()
		};
		let overlay = AuditConfigLayer {
			enabled: Some(false),
			queue_capacity: Some(5000),
			..Default::default()
		};
		base.merge(overlay);
		assert_eq!(base.enabled, Some(false));
		assert_eq!(base.retention_days, Some(90));
		assert_eq!(base.queue_capacity, Some(5000));
	}

	#[test]
	fn test_syslog_config_finalize_disabled() {
		let layer = SyslogConfigLayer {
			enabled: Some(false),
			host: Some("syslog.example.com".to_string()),
			port: Some(514),
			..Default::default()
		};
		assert!(layer.finalize().is_none());
	}

	#[test]
	fn test_syslog_config_finalize_enabled() {
		let layer = SyslogConfigLayer {
			enabled: Some(true),
			host: Some("syslog.example.com".to_string()),
			port: Some(1514),
			protocol: Some(SyslogProtocol::Tcp),
			facility: Some("auth".to_string()),
			app_name: Some("loom-server".to_string()),
			use_cef: Some(true),
		};
		let config = layer.finalize().unwrap();
		assert_eq!(config.host, "syslog.example.com");
		assert_eq!(config.port, 1514);
		assert_eq!(config.protocol, SyslogProtocol::Tcp);
		assert_eq!(config.facility, "auth");
		assert_eq!(config.app_name, "loom-server");
		assert!(config.use_cef);
	}

	#[test]
	fn test_http_sink_config_finalize() {
		let layer = HttpSinkConfigLayer {
			name: Some("datadog".to_string()),
			url: Some("https://http-intake.logs.datadoghq.com/api/v2/logs".to_string()),
			headers: Some(vec![("DD-API-KEY".to_string(), "secret".to_string())]),
			..Default::default()
		};
		let config = layer.finalize().unwrap();
		assert_eq!(config.name, "datadog");
		assert_eq!(config.method, "POST");
		assert_eq!(config.timeout_ms, 5000);
		assert_eq!(config.retry_max_attempts, 3);
	}

	#[test]
	fn test_http_sink_config_finalize_missing_required() {
		let layer = HttpSinkConfigLayer {
			name: Some("test".to_string()),
			url: None,
			..Default::default()
		};
		assert!(layer.finalize().is_none());
	}

	#[test]
	fn test_json_stream_config_finalize() {
		let layer = JsonStreamConfigLayer {
			name: Some("logstash".to_string()),
			host: Some("logstash.example.com".to_string()),
			port: Some(5044),
			protocol: Some(StreamProtocol::Tcp),
			min_severity: Some("warning".to_string()),
		};
		let config = layer.finalize().unwrap();
		assert_eq!(config.name, "logstash");
		assert_eq!(config.host, "logstash.example.com");
		assert_eq!(config.port, 5044);
		assert_eq!(config.protocol, StreamProtocol::Tcp);
		assert_eq!(config.min_severity, "warning");
	}

	#[test]
	fn test_file_sink_config_finalize() {
		let layer = FileSinkConfigLayer {
			path: Some("/var/log/audit-%Y-%m-%d.log".to_string()),
			format: Some(FileFormat::Cef),
			min_severity: Some("error".to_string()),
		};
		let config = layer.finalize().unwrap();
		assert_eq!(config.path, "/var/log/audit-%Y-%m-%d.log");
		assert_eq!(config.format, FileFormat::Cef);
		assert_eq!(config.min_severity, "error");
	}

	#[test]
	fn test_queue_overflow_policy_serde() {
		let drop_newest: QueueOverflowPolicy = serde_json::from_str(r#""drop_newest""#).unwrap();
		assert_eq!(drop_newest, QueueOverflowPolicy::DropNewest);

		let drop_oldest: QueueOverflowPolicy = serde_json::from_str(r#""drop_oldest""#).unwrap();
		assert_eq!(drop_oldest, QueueOverflowPolicy::DropOldest);

		let block: QueueOverflowPolicy = serde_json::from_str(r#""block""#).unwrap();
		assert_eq!(block, QueueOverflowPolicy::Block);
	}

	#[test]
	fn test_syslog_protocol_serde() {
		let udp: SyslogProtocol = serde_json::from_str(r#""udp""#).unwrap();
		assert_eq!(udp, SyslogProtocol::Udp);

		let tcp: SyslogProtocol = serde_json::from_str(r#""tcp""#).unwrap();
		assert_eq!(tcp, SyslogProtocol::Tcp);
	}

	#[test]
	fn test_stream_protocol_serde() {
		let tcp: StreamProtocol = serde_json::from_str(r#""tcp""#).unwrap();
		assert_eq!(tcp, StreamProtocol::Tcp);

		let udp: StreamProtocol = serde_json::from_str(r#""udp""#).unwrap();
		assert_eq!(udp, StreamProtocol::Udp);
	}

	#[test]
	fn test_file_format_serde() {
		let json_lines: FileFormat = serde_json::from_str(r#""json_lines""#).unwrap();
		assert_eq!(json_lines, FileFormat::JsonLines);

		let cef: FileFormat = serde_json::from_str(r#""cef""#).unwrap();
		assert_eq!(cef, FileFormat::Cef);
	}

	#[test]
	fn test_toml_roundtrip() {
		let config = AuditConfig {
			enabled: true,
			retention_days: 60,
			queue_capacity: 5000,
			queue_overflow_policy: QueueOverflowPolicy::DropOldest,
			min_severity: "warning".to_string(),
			syslog: Some(SyslogConfig {
				host: "syslog.local".to_string(),
				port: 514,
				protocol: SyslogProtocol::Udp,
				facility: "local0".to_string(),
				app_name: "loom".to_string(),
				use_cef: false,
			}),
			http_sinks: vec![],
			json_stream_sinks: vec![],
			file_sinks: vec![],
		};
		let toml_str = toml::to_string(&config).unwrap();
		let parsed: AuditConfig = toml::from_str(&toml_str).unwrap();
		assert_eq!(config, parsed);
	}
}
