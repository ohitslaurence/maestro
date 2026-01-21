// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! Check-in types for cron monitoring.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;
use uuid::Uuid;

use crate::MonitorId;

/// Unique identifier for a check-in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct CheckInId(pub Uuid);

impl CheckInId {
	pub fn new() -> Self {
		Self(Uuid::new_v4())
	}
}

impl Default for CheckInId {
	fn default() -> Self {
		Self::new()
	}
}

impl fmt::Display for CheckInId {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		write!(f, "{}", self.0)
	}
}

impl FromStr for CheckInId {
	type Err = uuid::Error;

	fn from_str(s: &str) -> Result<Self, Self::Err> {
		Ok(Self(Uuid::parse_str(s)?))
	}
}

/// A single job execution report.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct CheckIn {
	pub id: CheckInId,
	pub monitor_id: MonitorId,

	pub status: CheckInStatus,

	pub started_at: Option<DateTime<Utc>>,
	pub finished_at: DateTime<Utc>,
	pub duration_ms: Option<u64>,

	/// Environment: "production", "staging"
	pub environment: Option<String>,
	/// App version/release
	pub release: Option<String>,

	/// Exit code for failed jobs
	pub exit_code: Option<i32>,
	/// Truncated stdout/stderr (max 10KB)
	pub output: Option<String>,
	/// Link to crash system
	pub crash_event_id: Option<String>,

	pub source: CheckInSource,

	pub created_at: DateTime<Utc>,
}

/// Status of a check-in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum CheckInStatus {
	/// Job started, not yet finished
	InProgress,
	/// Job completed successfully
	Ok,
	/// Job failed (explicit error)
	Error,
	/// System-generated: expected ping didn't arrive
	Missed,
	/// System-generated: max_runtime exceeded
	Timeout,
}

impl fmt::Display for CheckInStatus {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		match self {
			Self::InProgress => write!(f, "in_progress"),
			Self::Ok => write!(f, "ok"),
			Self::Error => write!(f, "error"),
			Self::Missed => write!(f, "missed"),
			Self::Timeout => write!(f, "timeout"),
		}
	}
}

impl FromStr for CheckInStatus {
	type Err = String;

	fn from_str(s: &str) -> Result<Self, Self::Err> {
		match s {
			"in_progress" => Ok(Self::InProgress),
			"ok" => Ok(Self::Ok),
			"error" => Ok(Self::Error),
			"missed" => Ok(Self::Missed),
			"timeout" => Ok(Self::Timeout),
			_ => Err(format!("unknown check-in status: {}", s)),
		}
	}
}

/// Source of a check-in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum CheckInSource {
	/// Simple HTTP ping
	Ping,
	/// SDK check-in
	Sdk,
	/// Manual via API/UI
	Manual,
	/// System-generated (missed, timeout)
	System,
}

impl fmt::Display for CheckInSource {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		match self {
			Self::Ping => write!(f, "ping"),
			Self::Sdk => write!(f, "sdk"),
			Self::Manual => write!(f, "manual"),
			Self::System => write!(f, "system"),
		}
	}
}

impl FromStr for CheckInSource {
	type Err = String;

	fn from_str(s: &str) -> Result<Self, Self::Err> {
		match s {
			"ping" => Ok(Self::Ping),
			"sdk" => Ok(Self::Sdk),
			"manual" => Ok(Self::Manual),
			"system" => Ok(Self::System),
			_ => Err(format!("unknown check-in source: {}", s)),
		}
	}
}

/// Maximum output size in bytes (10KB).
pub const MAX_OUTPUT_BYTES: usize = 10 * 1024;

/// Truncate output to the maximum size.
pub fn truncate_output(output: &str) -> String {
	if output.len() <= MAX_OUTPUT_BYTES {
		output.to_string()
	} else {
		let truncated = &output[..MAX_OUTPUT_BYTES];
		// Find last valid UTF-8 boundary
		let valid_len = truncated
			.char_indices()
			.filter(|(i, _)| *i <= MAX_OUTPUT_BYTES)
			.next_back()
			.map(|(i, c)| i + c.len_utf8())
			.unwrap_or(0);
		format!("{}...[truncated]", &output[..valid_len])
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use proptest::prelude::*;

	proptest! {
		#[test]
		fn checkin_id_roundtrip(uuid_bytes in any::<[u8; 16]>()) {
			let uuid = Uuid::from_bytes(uuid_bytes);
			let id = CheckInId(uuid);
			let s = id.to_string();
			let parsed: CheckInId = s.parse().unwrap();
			prop_assert_eq!(id, parsed);
		}

		#[test]
		fn checkin_status_roundtrip(status in prop_oneof![
			Just(CheckInStatus::InProgress),
			Just(CheckInStatus::Ok),
			Just(CheckInStatus::Error),
			Just(CheckInStatus::Missed),
			Just(CheckInStatus::Timeout),
		]) {
			let s = status.to_string();
			let parsed: CheckInStatus = s.parse().unwrap();
			prop_assert_eq!(status, parsed);
		}

		#[test]
		fn checkin_source_roundtrip(source in prop_oneof![
			Just(CheckInSource::Ping),
			Just(CheckInSource::Sdk),
			Just(CheckInSource::Manual),
			Just(CheckInSource::System),
		]) {
			let s = source.to_string();
			let parsed: CheckInSource = s.parse().unwrap();
			prop_assert_eq!(source, parsed);
		}

		#[test]
		fn truncate_output_preserves_small_strings(s in ".{0,100}") {
			let truncated = truncate_output(&s);
			prop_assert_eq!(truncated, s);
		}
	}

	#[test]
	fn truncate_output_truncates_large_strings() {
		let large = "a".repeat(20_000);
		let truncated = truncate_output(&large);
		assert!(truncated.len() < large.len());
		assert!(truncated.ends_with("...[truncated]"));
	}
}
