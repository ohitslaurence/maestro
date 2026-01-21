// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! Background job for detecting missed cron monitor runs.
//!
//! This job runs periodically (typically every minute) to find monitors
//! that have missed their expected check-in time and creates synthetic
//! "missed" check-in records.

use async_trait::async_trait;
use chrono::Utc;
use loom_crons_core::{CheckIn, CheckInId, CheckInSource, CheckInStatus, MonitorHealth};
use loom_server_crons::{calculate_next_expected, CronsRepository, SqliteCronsRepository};
use loom_server_jobs::{Job, JobContext, JobError, JobOutput};
use std::sync::Arc;
use tracing::{info, instrument, warn};

pub struct CronMissedRunDetectorJob {
	crons_repo: Arc<SqliteCronsRepository>,
}

impl CronMissedRunDetectorJob {
	pub fn new(crons_repo: Arc<SqliteCronsRepository>) -> Self {
		Self { crons_repo }
	}
}

#[async_trait]
impl Job for CronMissedRunDetectorJob {
	fn id(&self) -> &str {
		"cron-missed-run-detector"
	}

	fn name(&self) -> &str {
		"Cron Missed Run Detector"
	}

	fn description(&self) -> &str {
		"Detect cron monitors that have missed their expected check-in and create synthetic missed check-ins"
	}

	#[instrument(skip(self, ctx), fields(job_id = "cron-missed-run-detector"))]
	async fn run(&self, ctx: &JobContext) -> Result<JobOutput, JobError> {
		if ctx.cancellation_token.is_cancelled() {
			return Err(JobError::Cancelled);
		}

		let now = Utc::now();
		let mut missed_count = 0;

		// Find all overdue monitors
		let overdue_monitors = self
			.crons_repo
			.list_overdue_monitors(now)
			.await
			.map_err(|e| JobError::Failed {
				message: format!("Failed to query overdue monitors: {}", e),
				retryable: true,
			})?;

		for monitor in overdue_monitors {
			if ctx.cancellation_token.is_cancelled() {
				return Err(JobError::Cancelled);
			}

			// Create a synthetic "missed" check-in
			let checkin = CheckIn {
				id: CheckInId::new(),
				monitor_id: monitor.id,
				status: CheckInStatus::Missed,
				started_at: None,
				finished_at: now,
				duration_ms: None,
				environment: None,
				release: None,
				exit_code: None,
				output: Some("System-generated: Expected check-in did not arrive".to_string()),
				crash_event_id: None,
				source: CheckInSource::System,
				created_at: now,
			};

			if let Err(e) = self.crons_repo.create_checkin(&checkin).await {
				warn!(
					monitor_id = %monitor.id,
					monitor_slug = %monitor.slug,
					error = %e,
					"Failed to create missed check-in"
				);
				continue;
			}

			// Update monitor health to Missed
			if let Err(e) = self
				.crons_repo
				.update_monitor_health(monitor.id, MonitorHealth::Missed)
				.await
			{
				warn!(
					monitor_id = %monitor.id,
					error = %e,
					"Failed to update monitor health to missed"
				);
			}

			// Calculate next expected time and update last check-in
			let next_expected_at =
				calculate_next_expected(&monitor.schedule, &monitor.timezone, now).ok();

			if let Err(e) = self
				.crons_repo
				.update_monitor_last_checkin(monitor.id, CheckInStatus::Missed, next_expected_at)
				.await
			{
				warn!(
					monitor_id = %monitor.id,
					error = %e,
					"Failed to update monitor last check-in"
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
				monitor_id = %monitor.id,
				monitor_slug = %monitor.slug,
				monitor_name = %monitor.name,
				expected_at = ?monitor.next_expected_at,
				consecutive_failures = monitor.consecutive_failures + 1,
				"Monitor missed expected check-in"
			);

			missed_count += 1;
		}

		if missed_count > 0 {
			info!(missed_count, "Cron missed run detection completed");
		}

		Ok(JobOutput {
			message: format!("Detected {} missed cron runs", missed_count),
			metadata: Some(serde_json::json!({
				"missed_count": missed_count,
				"checked_at": now.to_rfc3339(),
			})),
		})
	}
}
