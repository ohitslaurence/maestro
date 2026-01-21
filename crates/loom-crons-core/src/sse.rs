// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! SSE (Server-Sent Events) types for real-time cron monitoring updates.
//!
//! This module defines the event types used for streaming cron monitoring updates
//! to connected clients.
//!
//! # Events
//!
//! - `init` - Full state of all monitors on connect
//! - `checkin.started` - Job started (in_progress)
//! - `checkin.ok` - Job completed successfully
//! - `checkin.error` - Job failed
//! - `monitor.missed` - Expected check-in didn't arrive
//! - `monitor.timeout` - Job exceeded max runtime
//! - `monitor.healthy` - Monitor recovered from failure
//! - `heartbeat` - Keep-alive (every 30s)

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::{CheckInId, CheckInStatus, MonitorHealth, MonitorId, MonitorStatus};

/// SSE event types for cron streaming.
///
/// Each variant corresponds to a specific event type that can be sent
/// to connected clients via SSE.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "event", content = "data")]
pub enum CronStreamEvent {
	/// Initial state sent on connection.
	/// Contains all monitors with their current health.
	#[serde(rename = "init")]
	Init(CronInitData),

	/// Job started (in_progress check-in).
	#[serde(rename = "checkin.started")]
	CheckInStarted(CheckInStartedData),

	/// Job completed successfully.
	#[serde(rename = "checkin.ok")]
	CheckInOk(CheckInOkData),

	/// Job failed.
	#[serde(rename = "checkin.error")]
	CheckInError(CheckInErrorData),

	/// Expected check-in didn't arrive.
	#[serde(rename = "monitor.missed")]
	MonitorMissed(MonitorMissedData),

	/// Job exceeded max runtime.
	#[serde(rename = "monitor.timeout")]
	MonitorTimeout(MonitorTimeoutData),

	/// Monitor recovered from failure.
	#[serde(rename = "monitor.healthy")]
	MonitorHealthy(MonitorHealthyData),

	/// Heartbeat for connection keep-alive.
	#[serde(rename = "heartbeat")]
	Heartbeat(CronHeartbeatData),
}

impl CronStreamEvent {
	/// Returns the event type name as a string.
	pub fn event_type(&self) -> &'static str {
		match self {
			CronStreamEvent::Init(_) => "init",
			CronStreamEvent::CheckInStarted(_) => "checkin.started",
			CronStreamEvent::CheckInOk(_) => "checkin.ok",
			CronStreamEvent::CheckInError(_) => "checkin.error",
			CronStreamEvent::MonitorMissed(_) => "monitor.missed",
			CronStreamEvent::MonitorTimeout(_) => "monitor.timeout",
			CronStreamEvent::MonitorHealthy(_) => "monitor.healthy",
			CronStreamEvent::Heartbeat(_) => "heartbeat",
		}
	}

	/// Creates a new init event with the given monitors.
	pub fn init(monitors: Vec<MonitorState>) -> Self {
		CronStreamEvent::Init(CronInitData {
			monitors,
			timestamp: Utc::now(),
		})
	}

	/// Creates a new checkin.started event.
	pub fn checkin_started(
		monitor_id: MonitorId,
		monitor_slug: String,
		checkin_id: CheckInId,
	) -> Self {
		CronStreamEvent::CheckInStarted(CheckInStartedData {
			monitor_id,
			monitor_slug,
			checkin_id,
			timestamp: Utc::now(),
		})
	}

	/// Creates a new checkin.ok event.
	pub fn checkin_ok(
		monitor_id: MonitorId,
		monitor_slug: String,
		checkin_id: CheckInId,
		duration_ms: Option<u64>,
	) -> Self {
		CronStreamEvent::CheckInOk(CheckInOkData {
			monitor_id,
			monitor_slug,
			checkin_id,
			duration_ms,
			timestamp: Utc::now(),
		})
	}

	/// Creates a new checkin.error event.
	pub fn checkin_error(
		monitor_id: MonitorId,
		monitor_slug: String,
		checkin_id: CheckInId,
		exit_code: Option<i32>,
		consecutive_failures: u32,
	) -> Self {
		CronStreamEvent::CheckInError(CheckInErrorData {
			monitor_id,
			monitor_slug,
			checkin_id,
			exit_code,
			consecutive_failures,
			timestamp: Utc::now(),
		})
	}

	/// Creates a new monitor.missed event.
	pub fn monitor_missed(
		monitor_id: MonitorId,
		monitor_slug: String,
		monitor_name: String,
		expected_at: Option<DateTime<Utc>>,
		consecutive_failures: u32,
	) -> Self {
		CronStreamEvent::MonitorMissed(MonitorMissedData {
			monitor_id,
			monitor_slug,
			monitor_name,
			expected_at,
			consecutive_failures,
			timestamp: Utc::now(),
		})
	}

	/// Creates a new monitor.timeout event.
	pub fn monitor_timeout(
		monitor_id: MonitorId,
		monitor_slug: String,
		checkin_id: CheckInId,
		started_at: Option<DateTime<Utc>>,
		max_runtime_minutes: Option<u32>,
	) -> Self {
		CronStreamEvent::MonitorTimeout(MonitorTimeoutData {
			monitor_id,
			monitor_slug,
			checkin_id,
			started_at,
			max_runtime_minutes,
			timestamp: Utc::now(),
		})
	}

	/// Creates a new monitor.healthy event.
	pub fn monitor_healthy(monitor_id: MonitorId, monitor_slug: String) -> Self {
		CronStreamEvent::MonitorHealthy(MonitorHealthyData {
			monitor_id,
			monitor_slug,
			timestamp: Utc::now(),
		})
	}

	/// Creates a new heartbeat event.
	pub fn heartbeat() -> Self {
		CronStreamEvent::Heartbeat(CronHeartbeatData {
			timestamp: Utc::now(),
		})
	}
}

/// Initial state data sent on connection.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CronInitData {
	/// All monitors with their current state.
	pub monitors: Vec<MonitorState>,
	/// When the init data was generated.
	pub timestamp: DateTime<Utc>,
}

/// Compact representation of a monitor's current state.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MonitorState {
	/// Monitor ID.
	pub id: MonitorId,
	/// Monitor slug.
	pub slug: String,
	/// Monitor name.
	pub name: String,
	/// Current status.
	pub status: MonitorStatus,
	/// Current health.
	pub health: MonitorHealth,
	/// Last check-in status.
	pub last_checkin_status: Option<CheckInStatus>,
	/// When the last check-in occurred.
	pub last_checkin_at: Option<DateTime<Utc>>,
	/// When the next check-in is expected.
	pub next_expected_at: Option<DateTime<Utc>>,
	/// Consecutive failures count.
	pub consecutive_failures: u32,
}

/// Data for checkin.started event.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CheckInStartedData {
	/// The monitor ID.
	pub monitor_id: MonitorId,
	/// The monitor slug.
	pub monitor_slug: String,
	/// The check-in ID.
	pub checkin_id: CheckInId,
	/// When the check-in started.
	pub timestamp: DateTime<Utc>,
}

/// Data for checkin.ok event.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CheckInOkData {
	/// The monitor ID.
	pub monitor_id: MonitorId,
	/// The monitor slug.
	pub monitor_slug: String,
	/// The check-in ID.
	pub checkin_id: CheckInId,
	/// Duration in milliseconds.
	pub duration_ms: Option<u64>,
	/// When the check-in completed.
	pub timestamp: DateTime<Utc>,
}

/// Data for checkin.error event.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CheckInErrorData {
	/// The monitor ID.
	pub monitor_id: MonitorId,
	/// The monitor slug.
	pub monitor_slug: String,
	/// The check-in ID.
	pub checkin_id: CheckInId,
	/// Exit code if available.
	pub exit_code: Option<i32>,
	/// Consecutive failures count.
	pub consecutive_failures: u32,
	/// When the error occurred.
	pub timestamp: DateTime<Utc>,
}

/// Data for monitor.missed event.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MonitorMissedData {
	/// The monitor ID.
	pub monitor_id: MonitorId,
	/// The monitor slug.
	pub monitor_slug: String,
	/// The monitor name.
	pub monitor_name: String,
	/// When the check-in was expected.
	pub expected_at: Option<DateTime<Utc>>,
	/// Consecutive failures count.
	pub consecutive_failures: u32,
	/// When the missed event was detected.
	pub timestamp: DateTime<Utc>,
}

/// Data for monitor.timeout event.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MonitorTimeoutData {
	/// The monitor ID.
	pub monitor_id: MonitorId,
	/// The monitor slug.
	pub monitor_slug: String,
	/// The check-in ID that timed out.
	pub checkin_id: CheckInId,
	/// When the check-in started.
	pub started_at: Option<DateTime<Utc>>,
	/// Max runtime in minutes that was exceeded.
	pub max_runtime_minutes: Option<u32>,
	/// When the timeout was detected.
	pub timestamp: DateTime<Utc>,
}

/// Data for monitor.healthy event.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MonitorHealthyData {
	/// The monitor ID.
	pub monitor_id: MonitorId,
	/// The monitor slug.
	pub monitor_slug: String,
	/// When the monitor became healthy.
	pub timestamp: DateTime<Utc>,
}

/// Data for heartbeat event.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CronHeartbeatData {
	/// When the heartbeat was sent.
	pub timestamp: DateTime<Utc>,
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_event_type() {
		assert_eq!(CronStreamEvent::init(vec![]).event_type(), "init");
		assert_eq!(
			CronStreamEvent::checkin_started(MonitorId::new(), "test".to_string(), CheckInId::new())
				.event_type(),
			"checkin.started"
		);
		assert_eq!(
			CronStreamEvent::checkin_ok(MonitorId::new(), "test".to_string(), CheckInId::new(), None)
				.event_type(),
			"checkin.ok"
		);
		assert_eq!(
			CronStreamEvent::checkin_error(
				MonitorId::new(),
				"test".to_string(),
				CheckInId::new(),
				None,
				1
			)
			.event_type(),
			"checkin.error"
		);
		assert_eq!(
			CronStreamEvent::monitor_missed(
				MonitorId::new(),
				"test".to_string(),
				"Test".to_string(),
				None,
				1
			)
			.event_type(),
			"monitor.missed"
		);
		assert_eq!(
			CronStreamEvent::monitor_timeout(
				MonitorId::new(),
				"test".to_string(),
				CheckInId::new(),
				None,
				Some(30)
			)
			.event_type(),
			"monitor.timeout"
		);
		assert_eq!(
			CronStreamEvent::monitor_healthy(MonitorId::new(), "test".to_string()).event_type(),
			"monitor.healthy"
		);
		assert_eq!(CronStreamEvent::heartbeat().event_type(), "heartbeat");
	}

	#[test]
	fn test_checkin_ok_serialization() {
		let event = CronStreamEvent::checkin_ok(
			MonitorId::new(),
			"daily-backup".to_string(),
			CheckInId::new(),
			Some(5000),
		);

		let json = serde_json::to_string(&event).unwrap();
		assert!(json.contains(r#""event":"checkin.ok""#));
		assert!(json.contains(r#""monitor_slug":"daily-backup""#));
		assert!(json.contains(r#""duration_ms":5000"#));
	}

	#[test]
	fn test_monitor_missed_serialization() {
		let event = CronStreamEvent::monitor_missed(
			MonitorId::new(),
			"daily-cleanup".to_string(),
			"Daily Cleanup".to_string(),
			Some(Utc::now()),
			2,
		);

		let json = serde_json::to_string(&event).unwrap();
		assert!(json.contains(r#""event":"monitor.missed""#));
		assert!(json.contains(r#""monitor_slug":"daily-cleanup""#));
		assert!(json.contains(r#""consecutive_failures":2"#));
	}

	#[test]
	fn test_heartbeat_serialization() {
		let event = CronStreamEvent::heartbeat();
		let json = serde_json::to_string(&event).unwrap();
		assert!(json.contains(r#""event":"heartbeat""#));
		assert!(json.contains(r#""timestamp""#));
	}

	#[test]
	fn test_init_serialization() {
		let monitors = vec![MonitorState {
			id: MonitorId::new(),
			slug: "test-monitor".to_string(),
			name: "Test Monitor".to_string(),
			status: MonitorStatus::Active,
			health: MonitorHealth::Healthy,
			last_checkin_status: Some(CheckInStatus::Ok),
			last_checkin_at: Some(Utc::now()),
			next_expected_at: Some(Utc::now()),
			consecutive_failures: 0,
		}];

		let event = CronStreamEvent::init(monitors);
		let json = serde_json::to_string(&event).unwrap();
		assert!(json.contains(r#""event":"init""#));
		assert!(json.contains(r#""monitors""#));
		assert!(json.contains(r#""test-monitor""#));
	}

	#[test]
	fn test_deserialization_roundtrip() {
		let event = CronStreamEvent::checkin_error(
			MonitorId::new(),
			"failing-job".to_string(),
			CheckInId::new(),
			Some(1),
			3,
		);

		let json = serde_json::to_string(&event).unwrap();
		let parsed: CronStreamEvent = serde_json::from_str(&json).unwrap();

		if let CronStreamEvent::CheckInError(data) = parsed {
			assert_eq!(data.monitor_slug, "failing-job");
			assert_eq!(data.exit_code, Some(1));
			assert_eq!(data.consecutive_failures, 3);
		} else {
			panic!("Expected CheckInError event");
		}
	}
}
