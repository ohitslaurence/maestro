// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use std::sync::Arc;

use async_trait::async_trait;
use loom_server_jobs::{Job, JobContext, JobError, JobOutput};
use loom_server_weaver::Provisioner;
use tracing::instrument;

pub struct WeaverCleanupJob {
	provisioner: Arc<Provisioner>,
}

impl WeaverCleanupJob {
	pub fn new(provisioner: Arc<Provisioner>) -> Self {
		Self { provisioner }
	}
}

#[async_trait]
impl Job for WeaverCleanupJob {
	fn id(&self) -> &str {
		"weaver-cleanup"
	}

	fn name(&self) -> &str {
		"Weaver Cleanup"
	}

	fn description(&self) -> &str {
		"Clean up expired weaver pods from Kubernetes"
	}

	#[instrument(skip(self, ctx), fields(job_id = "weaver-cleanup"))]
	async fn run(&self, ctx: &JobContext) -> Result<JobOutput, JobError> {
		if ctx.cancellation_token.is_cancelled() {
			return Err(JobError::Cancelled);
		}

		match self.provisioner.cleanup_expired_weavers().await {
			Ok(result) => {
				tracing::info!(deleted_count = result.count, "Weaver cleanup completed");
				Ok(JobOutput {
					message: format!("Deleted {} expired weavers", result.count),
					metadata: Some(serde_json::json!({ "deleted_count": result.count })),
				})
			}
			Err(e) => {
				tracing::error!(error = %e, "Weaver cleanup failed");
				Err(JobError::Failed {
					message: e.to_string(),
					retryable: true,
				})
			}
		}
	}
}
