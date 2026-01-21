// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! Statistics types for cron monitoring.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::MonitorId;

/// Aggregated statistics for a monitor.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct MonitorStats {
	pub monitor_id: MonitorId,
	pub period: StatsPeriod,

	pub total_checkins: u64,
	pub successful_checkins: u64,
	pub failed_checkins: u64,
	pub missed_checkins: u64,
	pub timeout_checkins: u64,

	pub avg_duration_ms: Option<u64>,
	pub p50_duration_ms: Option<u64>,
	pub p95_duration_ms: Option<u64>,
	pub max_duration_ms: Option<u64>,

	/// (ok / total) * 100
	pub uptime_percentage: f64,

	pub updated_at: DateTime<Utc>,
}

/// Time period for statistics aggregation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum StatsPeriod {
	Day,
	Week,
	Month,
}

impl StatsPeriod {
	/// Get the number of days in this period.
	pub fn days(&self) -> u32 {
		match self {
			Self::Day => 1,
			Self::Week => 7,
			Self::Month => 30,
		}
	}
}

/// Daily rollup statistics (stored in database).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailyStats {
	pub id: String,
	pub monitor_id: MonitorId,
	/// Date in "YYYY-MM-DD" format
	pub date: String,

	pub total_checkins: u64,
	pub successful_checkins: u64,
	pub failed_checkins: u64,
	pub missed_checkins: u64,
	pub timeout_checkins: u64,

	pub avg_duration_ms: Option<u64>,
	pub min_duration_ms: Option<u64>,
	pub max_duration_ms: Option<u64>,

	pub updated_at: DateTime<Utc>,
}
