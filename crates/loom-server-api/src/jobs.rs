// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use loom_server_jobs::HealthState;
use serde::{Deserialize, Serialize};

#[cfg(feature = "openapi")]
use utoipa::{IntoParams, ToSchema};

#[derive(Debug, Serialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct JobInfo {
	pub id: String,
	pub name: String,
	pub description: String,
	pub job_type: String,
	pub interval_secs: Option<i64>,
	pub enabled: bool,
	pub status: JobHealthState,
	pub last_run: Option<LastRunInfo>,
	pub consecutive_failures: u32,
}

#[derive(Debug, Serialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct LastRunInfo {
	pub run_id: String,
	pub status: String,
	pub started_at: String,
	pub completed_at: Option<String>,
	pub duration_ms: Option<i64>,
	pub error: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum JobHealthState {
	Healthy,
	Degraded,
	Unhealthy,
}

#[derive(Debug, Serialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct ListJobsResponse {
	pub jobs: Vec<JobInfo>,
}

#[derive(Debug, Serialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct TriggerJobResponse {
	pub run_id: String,
	pub message: String,
}

#[derive(Debug, Serialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct JobSuccessResponse {
	pub message: String,
}

#[derive(Debug, Deserialize)]
#[cfg_attr(feature = "openapi", derive(IntoParams))]
pub struct HistoryQuery {
	#[serde(default = "default_limit")]
	pub limit: u32,
	#[serde(default)]
	pub offset: u32,
}

fn default_limit() -> u32 {
	50
}

#[derive(Debug, Serialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct JobHistoryResponse {
	pub runs: Vec<JobRunInfo>,
	pub total: u32,
	pub limit: u32,
	pub offset: u32,
}

#[derive(Debug, Serialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct JobRunInfo {
	pub id: String,
	pub status: String,
	pub started_at: String,
	pub completed_at: Option<String>,
	pub duration_ms: Option<i64>,
	pub error_message: Option<String>,
	pub retry_count: u32,
	pub triggered_by: String,
	pub metadata: Option<serde_json::Value>,
}

impl From<HealthState> for JobHealthState {
	fn from(state: HealthState) -> Self {
		match state {
			HealthState::Healthy => JobHealthState::Healthy,
			HealthState::Degraded => JobHealthState::Degraded,
			HealthState::Unhealthy => JobHealthState::Unhealthy,
		}
	}
}
