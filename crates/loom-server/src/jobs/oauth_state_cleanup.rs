// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use std::sync::Arc;

use async_trait::async_trait;
use loom_server_jobs::{Job, JobContext, JobError, JobOutput};
use tracing::instrument;

use crate::oauth_state::OAuthStateStore;

pub struct OAuthStateCleanupJob {
	store: Arc<OAuthStateStore>,
}

impl OAuthStateCleanupJob {
	pub fn new(store: Arc<OAuthStateStore>) -> Self {
		Self { store }
	}
}

#[async_trait]
impl Job for OAuthStateCleanupJob {
	fn id(&self) -> &str {
		"oauth-state-cleanup"
	}

	fn name(&self) -> &str {
		"OAuth State Cleanup"
	}

	fn description(&self) -> &str {
		"Remove expired OAuth state entries"
	}

	#[instrument(skip(self, ctx), fields(job_id = "oauth-state-cleanup"))]
	async fn run(&self, ctx: &JobContext) -> Result<JobOutput, JobError> {
		if ctx.cancellation_token.is_cancelled() {
			return Err(JobError::Cancelled);
		}

		let removed = self.store.cleanup_expired().await;

		tracing::debug!(removed_count = removed, "OAuth state cleanup completed");

		Ok(JobOutput {
			message: format!("Removed {} expired OAuth states", removed),
			metadata: Some(serde_json::json!({ "removed_count": removed })),
		})
	}
}
