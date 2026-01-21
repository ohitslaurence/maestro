// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! Background job for cleaning up old app sessions.
//!
//! This job runs daily to delete individual app sessions older than the
//! retention period (default: 30 days). Session aggregates are kept forever
//! for historical release health metrics.

use async_trait::async_trait;
use chrono::{Duration, Utc};
use loom_server_jobs::{Job, JobContext, JobError, JobOutput};
use loom_server_sessions::{SessionsRepository, SqliteSessionsRepository};
use std::sync::Arc;
use tracing::{info, instrument};

/// Default retention period for individual app sessions (30 days).
const DEFAULT_RETENTION_DAYS: i64 = 30;

pub struct AppSessionCleanupJob {
	sessions_repo: Arc<SqliteSessionsRepository>,
	retention_days: i64,
}

impl AppSessionCleanupJob {
	pub fn new(sessions_repo: Arc<SqliteSessionsRepository>) -> Self {
		Self {
			sessions_repo,
			retention_days: DEFAULT_RETENTION_DAYS,
		}
	}

	/// Create with a custom retention period.
	pub fn with_retention_days(sessions_repo: Arc<SqliteSessionsRepository>, days: i64) -> Self {
		Self {
			sessions_repo,
			retention_days: days,
		}
	}
}

#[async_trait]
impl Job for AppSessionCleanupJob {
	fn id(&self) -> &str {
		"app-session-cleanup"
	}

	fn name(&self) -> &str {
		"App Session Cleanup"
	}

	fn description(&self) -> &str {
		"Delete individual app sessions older than retention period"
	}

	#[instrument(skip(self, ctx), fields(job_id = "app-session-cleanup", retention_days = %self.retention_days))]
	async fn run(&self, ctx: &JobContext) -> Result<JobOutput, JobError> {
		if ctx.cancellation_token.is_cancelled() {
			return Err(JobError::Cancelled);
		}

		let cutoff = Utc::now() - Duration::days(self.retention_days);

		info!(
			cutoff = %cutoff.to_rfc3339(),
			retention_days = self.retention_days,
			"Starting app session cleanup"
		);

		let deleted_count = self
			.sessions_repo
			.delete_old_sessions(cutoff)
			.await
			.map_err(|e| JobError::Failed {
				message: format!("Failed to delete old app sessions: {}", e),
				retryable: true,
			})?;

		info!(
			deleted_count = deleted_count,
			cutoff = %cutoff.to_rfc3339(),
			"App session cleanup completed"
		);

		Ok(JobOutput {
			message: format!(
				"Deleted {} app sessions older than {} days",
				deleted_count, self.retention_days
			),
			metadata: Some(serde_json::json!({
				"deleted_count": deleted_count,
				"cutoff": cutoff.to_rfc3339(),
				"retention_days": self.retention_days,
			})),
		})
	}
}
