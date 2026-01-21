// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! Background job for detecting timed-out cron check-ins.
//!
//! This job runs periodically (typically every minute) to find in-progress
//! check-ins that have exceeded their monitor's max_runtime_minutes and
//! marks them as timed out.

use async_trait::async_trait;
use chrono::Utc;
use loom_crons_core::{CheckInStatus, MonitorHealth};
use loom_server_crons::{CronsRepository, SqliteCronsRepository};
use loom_server_jobs::{Job, JobContext, JobError, JobOutput};
use std::sync::Arc;
use tracing::{info, instrument, warn};

pub struct CronTimeoutDetectorJob {
	crons_repo: Arc<SqliteCronsRepository>,
}

impl CronTimeoutDetectorJob {
	pub fn new(crons_repo: Arc<SqliteCronsRepository>) -> Self {
		Self { crons_repo }
	}
}

#[async_trait]
impl Job for CronTimeoutDetectorJob {
	fn id(&self) -> &str {
		"cron-timeout-detector"
	}

	fn name(&self) -> &str {
		"Cron Timeout Detector"
	}

	fn description(&self) -> &str {
		"Detect in-progress cron check-ins that have exceeded max_runtime and mark them as timed out"
	}

	#[instrument(skip(self, ctx), fields(job_id = "cron-timeout-detector"))]
	async fn run(&self, ctx: &JobContext) -> Result<JobOutput, JobError> {
		if ctx.cancellation_token.is_cancelled() {
			return Err(JobError::Cancelled);
		}

		let now = Utc::now();
		let mut timeout_count = 0;

		// Find all timed-out check-ins
		let timed_out = self
			.crons_repo
			.list_timed_out_checkins(now)
			.await
			.map_err(|e| JobError::Failed {
				message: format!("Failed to query timed-out check-ins: {}", e),
				retryable: true,
			})?;

		for (mut checkin, monitor) in timed_out {
			if ctx.cancellation_token.is_cancelled() {
				return Err(JobError::Cancelled);
			}

			// Update check-in status to Timeout
			checkin.status = CheckInStatus::Timeout;
			checkin.finished_at = now;

			// Calculate duration from started_at
			if let Some(started_at) = checkin.started_at {
				checkin.duration_ms = Some((now - started_at).num_milliseconds() as u64);
			}

			checkin.output = Some(format!(
				"System-generated: Job exceeded max runtime of {} minutes",
				monitor.max_runtime_minutes.unwrap_or(0)
			));

			if let Err(e) = self.crons_repo.update_checkin(&checkin).await {
				warn!(
					checkin_id = %checkin.id,
					monitor_id = %monitor.id,
					error = %e,
					"Failed to update check-in to timeout"
				);
				continue;
			}

			// Update monitor health to Timeout
			if let Err(e) = self
				.crons_repo
				.update_monitor_health(monitor.id, MonitorHealth::Timeout)
				.await
			{
				warn!(
					monitor_id = %monitor.id,
					error = %e,
					"Failed to update monitor health to timeout"
				);
			}

			// Increment failure stats
			if let Err(e) = self
				.crons_repo
				.increment_monitor_stats(monitor.id, true)
				.await
			{
				warn!(
					monitor_id = %monitor.id,
					error = %e,
					"Failed to increment monitor stats"
				);
			}

			info!(
				checkin_id = %checkin.id,
				monitor_id = %monitor.id,
				monitor_slug = %monitor.slug,
				monitor_name = %monitor.name,
				started_at = ?checkin.started_at,
				max_runtime_minutes = ?monitor.max_runtime_minutes,
				"Check-in exceeded max runtime and timed out"
			);

			timeout_count += 1;
		}

		if timeout_count > 0 {
			info!(timeout_count, "Cron timeout detection completed");
		}

		Ok(JobOutput {
			message: format!("Detected {} timed out check-ins", timeout_count),
			metadata: Some(serde_json::json!({
				"timeout_count": timeout_count,
				"checked_at": now.to_rfc3339(),
			})),
		})
	}
}
