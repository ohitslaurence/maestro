// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use async_trait::async_trait;
use loom_server_db::SessionRepository;
use loom_server_jobs::{Job, JobContext, JobError, JobOutput};
use tracing::instrument;

pub struct SessionCleanupJob {
	session_repo: SessionRepository,
}

impl SessionCleanupJob {
	pub fn new(session_repo: SessionRepository) -> Self {
		Self { session_repo }
	}
}

#[async_trait]
impl Job for SessionCleanupJob {
	fn id(&self) -> &str {
		"session-cleanup"
	}

	fn name(&self) -> &str {
		"Session Cleanup"
	}

	fn description(&self) -> &str {
		"Delete expired user sessions from database"
	}

	#[instrument(skip(self, ctx), fields(job_id = "session-cleanup"))]
	async fn run(&self, ctx: &JobContext) -> Result<JobOutput, JobError> {
		if ctx.cancellation_token.is_cancelled() {
			return Err(JobError::Cancelled);
		}

		let sessions_deleted = self
			.session_repo
			.cleanup_expired_sessions()
			.await
			.map_err(|e| JobError::Failed {
				message: e.to_string(),
				retryable: true,
			})?;

		let tokens_deleted = self
			.session_repo
			.cleanup_expired_access_tokens()
			.await
			.map_err(|e| JobError::Failed {
				message: e.to_string(),
				retryable: true,
			})?;

		let device_codes_deleted = self
			.session_repo
			.cleanup_expired_device_codes()
			.await
			.map_err(|e| JobError::Failed {
				message: e.to_string(),
				retryable: true,
			})?;

		let magic_links_deleted = self
			.session_repo
			.cleanup_expired_magic_links()
			.await
			.map_err(|e| JobError::Failed {
				message: e.to_string(),
				retryable: true,
			})?;

		let total = sessions_deleted + tokens_deleted + device_codes_deleted + magic_links_deleted;

		tracing::info!(
			sessions_deleted,
			tokens_deleted,
			device_codes_deleted,
			magic_links_deleted,
			total,
			"Session cleanup completed"
		);

		Ok(JobOutput {
			message: format!("Deleted {} expired auth records", total),
			metadata: Some(serde_json::json!({
				"sessions_deleted": sessions_deleted,
				"tokens_deleted": tokens_deleted,
				"device_codes_deleted": device_codes_deleted,
				"magic_links_deleted": magic_links_deleted,
			})),
		})
	}
}
