// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

#![cfg(feature = "sink-file")]

use std::sync::Arc;

use async_trait::async_trait;
use chrono::{Datelike, Timelike, Utc};
use loom_server_config::{FileFormat, FileSinkConfig};
use tokio::fs::OpenOptions;
use tokio::io::AsyncWriteExt;
use tokio::sync::Mutex;

use crate::enrichment::EnrichedAuditEvent;
use crate::error::AuditSinkError;
use crate::event::AuditSeverity;
use crate::filter::AuditFilterConfig;
use crate::sink::AuditSink;

struct FileHandle {
	path: String,
	file: tokio::fs::File,
}

pub struct FileAuditSink {
	config: FileSinkConfig,
	filter: AuditFilterConfig,
	handle: Mutex<Option<FileHandle>>,
}

impl FileAuditSink {
	pub fn new(config: FileSinkConfig, filter: AuditFilterConfig) -> Self {
		Self {
			config,
			filter,
			handle: Mutex::new(None),
		}
	}

	async fn get_or_open_file(&self, expanded_path: &str) -> Result<(), AuditSinkError> {
		let mut guard = self.handle.lock().await;

		let needs_reopen = match &*guard {
			Some(handle) => handle.path != expanded_path,
			None => true,
		};

		if needs_reopen {
			let file = OpenOptions::new()
				.create(true)
				.append(true)
				.open(expanded_path)
				.await
				.map_err(|e| AuditSinkError::Transient(format!("failed to open file: {e}")))?;

			*guard = Some(FileHandle {
				path: expanded_path.to_string(),
				file,
			});
		}

		Ok(())
	}

	async fn write_line(&self, line: &str) -> Result<(), AuditSinkError> {
		let mut guard = self.handle.lock().await;
		let handle = guard
			.as_mut()
			.ok_or_else(|| AuditSinkError::Permanent("file handle not initialized".to_string()))?;

		handle
			.file
			.write_all(line.as_bytes())
			.await
			.map_err(|e| AuditSinkError::Transient(format!("failed to write to file: {e}")))?;

		handle
			.file
			.flush()
			.await
			.map_err(|e| AuditSinkError::Transient(format!("failed to flush file: {e}")))?;

		Ok(())
	}
}

#[async_trait]
impl AuditSink for FileAuditSink {
	fn name(&self) -> &str {
		"file"
	}

	fn filter(&self) -> &AuditFilterConfig {
		&self.filter
	}

	async fn publish(&self, event: Arc<EnrichedAuditEvent>) -> Result<(), AuditSinkError> {
		let expanded_path = expand_path(&self.config.path);
		self.get_or_open_file(&expanded_path).await?;

		let line = match self.config.format {
			FileFormat::JsonLines => format_json_line(&event)?,
			FileFormat::Cef => format_cef(&event),
		};

		self.write_line(&line).await
	}
}

pub fn format_json_line(event: &EnrichedAuditEvent) -> Result<String, AuditSinkError> {
	let json = serde_json::to_string(event)
		.map_err(|e| AuditSinkError::Permanent(format!("JSON serialization failed: {e}")))?;
	Ok(format!("{json}\n"))
}

pub fn format_cef(event: &EnrichedAuditEvent) -> String {
	let event_type = event.base.event_type.to_string().to_uppercase();
	let action = escape_cef_value(&event.base.action);
	let severity = severity_to_cef(event.base.severity);

	let mut extensions = Vec::new();

	extensions.push(format!("rt={}", event.base.timestamp.timestamp_millis()));
	extensions.push(format!("eventId={}", event.base.id));

	if let Some(ref actor) = event.base.actor_user_id {
		extensions.push(format!("suser={}", actor));
	}

	if let Some(ref ip) = event.base.ip_address {
		extensions.push(format!("src={}", escape_cef_value(ip)));
	}

	if let Some(ref resource_type) = event.base.resource_type {
		extensions.push(format!(
			"cs1Label=resourceType cs1={}",
			escape_cef_value(resource_type)
		));
	}

	if let Some(ref resource_id) = event.base.resource_id {
		extensions.push(format!(
			"cs2Label=resourceId cs2={}",
			escape_cef_value(resource_id)
		));
	}

	if let Some(ref trace_id) = event.base.trace_id {
		extensions.push(format!(
			"cs3Label=traceId cs3={}",
			escape_cef_value(trace_id)
		));
	}

	if let Some(ref session) = event.session {
		if let Some(ref session_id) = session.session_id {
			extensions.push(format!(
				"cs4Label=sessionId cs4={}",
				escape_cef_value(session_id)
			));
		}
	}

	if let Some(ref org) = event.org {
		if let Some(ref org_id) = org.org_id {
			extensions.push(format!("cs5Label=orgId cs5={}", escape_cef_value(org_id)));
		}
	}

	let extension_str = extensions.join(" ");

	format!("CEF:0|Loom|Loom Server|1.0|{event_type}|{action}|{severity}|{extension_str}\n")
}

pub fn expand_path(path: &str) -> String {
	let now = Utc::now();

	path
		.replace("%Y", &format!("{:04}", now.year()))
		.replace("%m", &format!("{:02}", now.month()))
		.replace("%d", &format!("{:02}", now.day()))
		.replace("%H", &format!("{:02}", now.hour()))
		.replace("%M", &format!("{:02}", now.minute()))
		.replace("%S", &format!("{:02}", now.second()))
}

fn severity_to_cef(severity: AuditSeverity) -> u8 {
	match severity {
		AuditSeverity::Debug => 1,
		AuditSeverity::Info => 3,
		AuditSeverity::Notice => 4,
		AuditSeverity::Warning => 6,
		AuditSeverity::Error => 8,
		AuditSeverity::Critical => 10,
	}
}

fn escape_cef_value(value: &str) -> String {
	value
		.replace('\\', "\\\\")
		.replace('|', "\\|")
		.replace('\n', "\\n")
		.replace('\r', "\\r")
		.replace('=', "\\=")
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::event::{AuditEventType, AuditLogEntry};

	fn make_test_event() -> EnrichedAuditEvent {
		EnrichedAuditEvent {
			base: AuditLogEntry::builder(AuditEventType::Login)
				.ip_address("192.168.1.1")
				.action("User logged in")
				.resource("session", "sess-123")
				.trace_id("trace-abc")
				.build(),
			session: None,
			org: None,
		}
	}

	#[test]
	fn test_format_json_line_produces_valid_json() {
		let event = make_test_event();
		let line = format_json_line(&event).unwrap();

		assert!(line.ends_with('\n'));
		let json: serde_json::Value = serde_json::from_str(line.trim()).unwrap();
		assert_eq!(json["base"]["event_type"], "login");
		assert_eq!(json["base"]["ip_address"], "192.168.1.1");
	}

	#[test]
	fn test_format_json_line_is_single_line() {
		let event = make_test_event();
		let line = format_json_line(&event).unwrap();
		let trimmed = line.trim_end_matches('\n');
		assert!(
			!trimmed.contains('\n'),
			"JSON line should not contain embedded newlines"
		);
	}

	#[test]
	fn test_format_cef_header() {
		let event = make_test_event();
		let cef = format_cef(&event);

		assert!(cef.starts_with("CEF:0|Loom|Loom Server|1.0|LOGIN|"));
		assert!(cef.ends_with('\n'));
	}

	#[test]
	fn test_format_cef_contains_event_fields() {
		let event = make_test_event();
		let cef = format_cef(&event);

		assert!(
			cef.contains("src=192.168.1.1"),
			"CEF should contain source IP"
		);
		assert!(
			cef.contains("cs1Label=resourceType cs1=session"),
			"CEF should contain resource type"
		);
		assert!(
			cef.contains("cs2Label=resourceId cs2=sess-123"),
			"CEF should contain resource ID"
		);
		assert!(
			cef.contains("cs3Label=traceId cs3=trace-abc"),
			"CEF should contain trace ID"
		);
	}

	#[test]
	fn test_format_cef_severity_mapping() {
		let mut event = make_test_event();
		event.base = AuditLogEntry::builder(AuditEventType::AccessDenied)
			.severity(AuditSeverity::Warning)
			.build();

		let cef = format_cef(&event);
		assert!(cef.contains("|6|"), "Warning severity should map to CEF 6");
	}

	#[test]
	fn test_expand_path_with_date_patterns() {
		let path = "/var/log/audit-%Y-%m-%d.log";
		let expanded = expand_path(path);

		assert!(!expanded.contains("%Y"));
		assert!(!expanded.contains("%m"));
		assert!(!expanded.contains("%d"));
		assert!(expanded.starts_with("/var/log/audit-"));
		assert!(expanded.ends_with(".log"));
	}

	#[test]
	fn test_expand_path_without_patterns() {
		let path = "/var/log/audit.log";
		let expanded = expand_path(path);
		assert_eq!(expanded, path);
	}

	#[test]
	fn test_escape_cef_value_escapes_pipe() {
		assert_eq!(escape_cef_value("a|b"), "a\\|b");
	}

	#[test]
	fn test_escape_cef_value_escapes_backslash() {
		assert_eq!(escape_cef_value("a\\b"), "a\\\\b");
	}

	#[test]
	fn test_escape_cef_value_escapes_newlines() {
		assert_eq!(escape_cef_value("a\nb\rc"), "a\\nb\\rc");
	}

	#[test]
	fn test_escape_cef_value_escapes_equals() {
		assert_eq!(escape_cef_value("key=value"), "key\\=value");
	}

	#[test]
	fn test_file_format_default() {
		let format: FileFormat = Default::default();
		assert_eq!(format, FileFormat::JsonLines);
	}

	#[test]
	fn test_file_format_serde() {
		let json_lines: FileFormat = serde_json::from_str("\"json_lines\"").unwrap();
		assert_eq!(json_lines, FileFormat::JsonLines);

		let cef: FileFormat = serde_json::from_str("\"cef\"").unwrap();
		assert_eq!(cef, FileFormat::Cef);
	}

	#[test]
	fn test_severity_to_cef_mapping() {
		assert_eq!(severity_to_cef(AuditSeverity::Debug), 1);
		assert_eq!(severity_to_cef(AuditSeverity::Info), 3);
		assert_eq!(severity_to_cef(AuditSeverity::Notice), 4);
		assert_eq!(severity_to_cef(AuditSeverity::Warning), 6);
		assert_eq!(severity_to_cef(AuditSeverity::Error), 8);
		assert_eq!(severity_to_cef(AuditSeverity::Critical), 10);
	}
}
