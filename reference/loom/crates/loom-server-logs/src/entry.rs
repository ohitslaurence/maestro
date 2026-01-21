// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights
// reserved. SPDX-License-Identifier: Proprietary

//! Log entry types.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Log level matching tracing levels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
	Trace,
	Debug,
	Info,
	Warn,
	Error,
}

impl LogLevel {
	/// Convert from tracing Level.
	pub fn from_tracing(level: &tracing::Level) -> Self {
		match *level {
			tracing::Level::TRACE => LogLevel::Trace,
			tracing::Level::DEBUG => LogLevel::Debug,
			tracing::Level::INFO => LogLevel::Info,
			tracing::Level::WARN => LogLevel::Warn,
			tracing::Level::ERROR => LogLevel::Error,
		}
	}

	/// Get the string representation.
	pub fn as_str(&self) -> &'static str {
		match self {
			LogLevel::Trace => "trace",
			LogLevel::Debug => "debug",
			LogLevel::Info => "info",
			LogLevel::Warn => "warn",
			LogLevel::Error => "error",
		}
	}
}

impl std::fmt::Display for LogLevel {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(f, "{}", self.as_str())
	}
}

/// A structured log entry.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct LogEntry {
	/// Unique sequential ID for this log entry.
	pub id: u64,
	/// Timestamp when the log was recorded.
	pub timestamp: DateTime<Utc>,
	/// Log level.
	pub level: LogLevel,
	/// The module/target that emitted the log.
	pub target: String,
	/// The log message.
	pub message: String,
	/// Additional structured fields as key-value pairs.
	#[serde(skip_serializing_if = "Vec::is_empty", default)]
	pub fields: Vec<(String, String)>,
}

impl LogEntry {
	/// Create a new log entry.
	pub fn new(
		id: u64,
		level: LogLevel,
		target: impl Into<String>,
		message: impl Into<String>,
		fields: Vec<(String, String)>,
	) -> Self {
		Self {
			id,
			timestamp: Utc::now(),
			level,
			target: target.into(),
			message: message.into(),
			fields,
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_log_level_ordering() {
		assert!(LogLevel::Trace < LogLevel::Debug);
		assert!(LogLevel::Debug < LogLevel::Info);
		assert!(LogLevel::Info < LogLevel::Warn);
		assert!(LogLevel::Warn < LogLevel::Error);
	}

	#[test]
	fn test_log_level_display() {
		assert_eq!(LogLevel::Info.to_string(), "info");
		assert_eq!(LogLevel::Error.to_string(), "error");
	}

	#[test]
	fn test_log_entry_serialization() {
		let entry = LogEntry::new(1, LogLevel::Info, "test::module", "Test message", vec![]);

		let json = serde_json::to_string(&entry).unwrap();
		assert!(json.contains("\"level\":\"info\""));
		assert!(json.contains("\"target\":\"test::module\""));
		assert!(json.contains("\"message\":\"Test message\""));
	}
}
