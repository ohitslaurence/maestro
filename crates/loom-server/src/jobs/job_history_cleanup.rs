// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights
// reserved. SPDX-License-Identifier: Proprietary

use async_trait::async_trait;
use loom_server_jobs::{Job, JobContext, JobError, JobOutput, JobRepository};
use std::sync::Arc;

pub struct JobHistoryCleanupJob {
	repository: Arc<JobRepository>,
	retention_days: u32,
}

impl JobHistoryCleanupJob {
	pub fn new(repository: Arc<JobRepository>, retention_days: u32) -> Self {
		Self {
			repository,
			retention_days,
		}
	}
}

#[async_trait]
impl Job for JobHistoryCleanupJob {
	fn id(&self) -> &str {
		"job-history-cleanup"
	}

	fn name(&self) -> &str {
		"Job History Cleanup"
	}

	fn description(&self) -> &str {
		"Removes old job run history entries"
	}

	async fn run(&self, ctx: &JobContext) -> Result<JobOutput, JobError> {
		if ctx.cancellation_token.is_cancelled() {
			return Err(JobError::Cancelled);
		}

		match self.repository.cleanup_old_runs(self.retention_days).await {
			Ok(count) => {
				tracing::info!(
					deleted = count,
					retention_days = self.retention_days,
					"Job history cleanup completed"
				);
				Ok(JobOutput {
					message: format!("Cleaned up {count} old job run records"),
					metadata: Some(serde_json::json!({
						"deleted_count": count,
						"retention_days": self.retention_days
					})),
				})
			}
			Err(e) => Err(JobError::Failed {
				message: format!("Job history cleanup failed: {e}"),
				retryable: true,
			}),
		}
	}
}
