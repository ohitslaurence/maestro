// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use crate::types::JobStatus;
use chrono::{DateTime, Utc};
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct JobHealthStatus {
	pub job_id: String,
	pub name: String,
	pub status: HealthState,
	pub last_run: Option<LastRunInfo>,
	pub consecutive_failures: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct LastRunInfo {
	pub run_id: String,
	pub status: JobStatus,
	pub started_at: DateTime<Utc>,
	pub duration_ms: Option<i64>,
	pub error: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum HealthState {
	Healthy,
	Degraded,
	Unhealthy,
}

#[derive(Debug, Clone, Serialize)]
pub struct JobsHealthStatus {
	pub status: HealthState,
	pub jobs: Vec<JobHealthStatus>,
}
