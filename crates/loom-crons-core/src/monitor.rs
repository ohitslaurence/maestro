// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! Monitor types for cron monitoring.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;
use uuid::Uuid;

use crate::CheckInStatus;

/// Unique identifier for a monitor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct MonitorId(pub Uuid);

impl MonitorId {
	pub fn new() -> Self {
		Self(Uuid::new_v4())
	}
}

impl Default for MonitorId {
	fn default() -> Self {
		Self::new()
	}
}

impl fmt::Display for MonitorId {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		write!(f, "{}", self.0)
	}
}

impl FromStr for MonitorId {
	type Err = uuid::Error;

	fn from_str(s: &str) -> Result<Self, Self::Err> {
		Ok(Self(Uuid::parse_str(s)?))
	}
}

/// Organization ID (for multi-tenant isolation).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct OrgId(pub Uuid);

impl OrgId {
	pub fn new() -> Self {
		Self(Uuid::new_v4())
	}
}

impl Default for OrgId {
	fn default() -> Self {
		Self::new()
	}
}

impl fmt::Display for OrgId {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		write!(f, "{}", self.0)
	}
}

impl FromStr for OrgId {
	type Err = uuid::Error;

	fn from_str(s: &str) -> Result<Self, Self::Err> {
		Ok(Self(Uuid::parse_str(s)?))
	}
}

/// A monitored job or scheduled task.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct Monitor {
	pub id: MonitorId,
	pub org_id: OrgId,

	/// URL-safe identifier: "daily-cleanup"
	pub slug: String,
	/// Human-readable name: "Daily Cleanup Job"
	pub name: String,
	pub description: Option<String>,

	pub status: MonitorStatus,
	pub health: MonitorHealth,

	/// Schedule configuration (cron or interval)
	pub schedule: MonitorSchedule,
	/// IANA timezone: "America/New_York"
	pub timezone: String,

	/// Grace period before marking as missed (default: 5 minutes)
	pub checkin_margin_minutes: u32,
	/// Alert if job exceeds this duration
	pub max_runtime_minutes: Option<u32>,

	/// UUID for /ping/{key} endpoints
	pub ping_key: String,

	/// Environment filter (empty = all)
	pub environments: Vec<String>,

	// Denormalized stats for quick display
	pub last_checkin_at: Option<DateTime<Utc>>,
	pub last_checkin_status: Option<CheckInStatus>,
	pub next_expected_at: Option<DateTime<Utc>>,
	pub consecutive_failures: u32,
	pub total_checkins: u64,
	pub total_failures: u64,

	pub created_at: DateTime<Utc>,
	pub updated_at: DateTime<Utc>,
}

impl Monitor {
	/// Generate a unique ping key (UUIDv4).
	pub fn generate_ping_key() -> String {
		Uuid::new_v4().to_string()
	}

	/// Get the full ping URL for this monitor.
	pub fn ping_url(&self, base_url: &str) -> String {
		format!("{}/ping/{}", base_url, self.ping_key)
	}

	/// Validate a slug (URL-safe identifier).
	pub fn validate_slug(slug: &str) -> bool {
		if slug.len() < 3 || slug.len() > 50 {
			return false;
		}
		slug
			.chars()
			.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
			&& slug.starts_with(|c: char| c.is_ascii_lowercase())
	}
}

/// Monitor status (whether monitoring is active).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum MonitorStatus {
	/// Monitoring enabled
	Active,
	/// Temporarily disabled (won't alert on missed)
	Paused,
	/// Fully disabled
	Disabled,
}

impl fmt::Display for MonitorStatus {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		match self {
			Self::Active => write!(f, "active"),
			Self::Paused => write!(f, "paused"),
			Self::Disabled => write!(f, "disabled"),
		}
	}
}

impl FromStr for MonitorStatus {
	type Err = String;

	fn from_str(s: &str) -> Result<Self, Self::Err> {
		match s {
			"active" => Ok(Self::Active),
			"paused" => Ok(Self::Paused),
			"disabled" => Ok(Self::Disabled),
			_ => Err(format!("unknown monitor status: {}", s)),
		}
	}
}

/// Monitor health (current state based on recent check-ins).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum MonitorHealth {
	/// Recent check-in was OK
	Healthy,
	/// Recent check-in was Error
	Failing,
	/// Expected check-in didn't arrive
	Missed,
	/// Job exceeded max_runtime
	Timeout,
	/// No check-ins yet
	Unknown,
}

impl fmt::Display for MonitorHealth {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		match self {
			Self::Healthy => write!(f, "healthy"),
			Self::Failing => write!(f, "failing"),
			Self::Missed => write!(f, "missed"),
			Self::Timeout => write!(f, "timeout"),
			Self::Unknown => write!(f, "unknown"),
		}
	}
}

impl FromStr for MonitorHealth {
	type Err = String;

	fn from_str(s: &str) -> Result<Self, Self::Err> {
		match s {
			"healthy" => Ok(Self::Healthy),
			"failing" => Ok(Self::Failing),
			"missed" => Ok(Self::Missed),
			"timeout" => Ok(Self::Timeout),
			"unknown" => Ok(Self::Unknown),
			_ => Err(format!("unknown monitor health: {}", s)),
		}
	}
}

/// Schedule configuration for a monitor.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum MonitorSchedule {
	/// Cron expression (e.g., "0 0 * * *" for daily at midnight)
	Cron { expression: String },
	/// Fixed interval (e.g., every 30 minutes)
	Interval { minutes: u32 },
}

impl MonitorSchedule {
	/// Get the schedule type as a string.
	pub fn schedule_type(&self) -> &'static str {
		match self {
			Self::Cron { .. } => "cron",
			Self::Interval { .. } => "interval",
		}
	}

	/// Get the schedule value as a string.
	pub fn schedule_value(&self) -> String {
		match self {
			Self::Cron { expression } => expression.clone(),
			Self::Interval { minutes } => minutes.to_string(),
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use proptest::prelude::*;

	proptest! {
		#[test]
		fn monitor_id_roundtrip(uuid_bytes in any::<[u8; 16]>()) {
			let uuid = Uuid::from_bytes(uuid_bytes);
			let id = MonitorId(uuid);
			let s = id.to_string();
			let parsed: MonitorId = s.parse().unwrap();
			prop_assert_eq!(id, parsed);
		}

		#[test]
		fn valid_slug_starts_with_lowercase(s in "[a-z][a-z0-9_-]{2,49}") {
			prop_assert!(Monitor::validate_slug(&s));
		}

		#[test]
		fn slug_rejects_uppercase(s in "[A-Z][a-z0-9_-]{2,49}") {
			prop_assert!(!Monitor::validate_slug(&s));
		}

		#[test]
		fn slug_rejects_too_short(s in "[a-z][a-z0-9_-]{0,1}") {
			prop_assert!(!Monitor::validate_slug(&s));
		}

		#[test]
		fn monitor_status_roundtrip(status in prop_oneof![
			Just(MonitorStatus::Active),
			Just(MonitorStatus::Paused),
			Just(MonitorStatus::Disabled),
		]) {
			let s = status.to_string();
			let parsed: MonitorStatus = s.parse().unwrap();
			prop_assert_eq!(status, parsed);
		}

		#[test]
		fn monitor_health_roundtrip(health in prop_oneof![
			Just(MonitorHealth::Healthy),
			Just(MonitorHealth::Failing),
			Just(MonitorHealth::Missed),
			Just(MonitorHealth::Timeout),
			Just(MonitorHealth::Unknown),
		]) {
			let s = health.to_string();
			let parsed: MonitorHealth = s.parse().unwrap();
			prop_assert_eq!(health, parsed);
		}
	}

	#[test]
	fn ping_key_is_uuid() {
		let key = Monitor::generate_ping_key();
		assert!(Uuid::parse_str(&key).is_ok());
	}

	#[test]
	fn ping_url_format() {
		let monitor = Monitor {
			id: MonitorId::new(),
			org_id: OrgId::new(),
			slug: "test".to_string(),
			name: "Test".to_string(),
			description: None,
			status: MonitorStatus::Active,
			health: MonitorHealth::Unknown,
			schedule: MonitorSchedule::Interval { minutes: 30 },
			timezone: "UTC".to_string(),
			checkin_margin_minutes: 5,
			max_runtime_minutes: None,
			ping_key: "abc123".to_string(),
			environments: vec![],
			last_checkin_at: None,
			last_checkin_status: None,
			next_expected_at: None,
			consecutive_failures: 0,
			total_checkins: 0,
			total_failures: 0,
			created_at: Utc::now(),
			updated_at: Utc::now(),
		};

		assert_eq!(
			monitor.ping_url("https://loom.example.com"),
			"https://loom.example.com/ping/abc123"
		);
	}
}
