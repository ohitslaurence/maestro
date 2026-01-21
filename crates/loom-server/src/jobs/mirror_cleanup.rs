// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use loom_server_jobs::{Job, JobContext, JobError, JobOutput};
use loom_server_scm_mirror::ExternalMirrorStore;
use tracing::instrument;

pub struct MirrorCleanupJob<S: ExternalMirrorStore> {
	store: Arc<S>,
	repos_dir: PathBuf,
	stale_after_days: u32,
}

impl<S: ExternalMirrorStore + 'static> MirrorCleanupJob<S> {
	pub fn new(store: Arc<S>, repos_dir: PathBuf, stale_after_days: u32) -> Self {
		Self {
			store,
			repos_dir,
			stale_after_days,
		}
	}

	fn repo_path_for_id(&self, repo_id: uuid::Uuid) -> PathBuf {
		let id_str = repo_id.to_string();
		let prefix = &id_str[..2];
		self.repos_dir.join(prefix).join(&id_str).join("git")
	}
}

#[async_trait]
impl<S: ExternalMirrorStore + 'static> Job for MirrorCleanupJob<S> {
	fn id(&self) -> &str {
		"mirror-cleanup"
	}

	fn name(&self) -> &str {
		"Mirror Cleanup"
	}

	fn description(&self) -> &str {
		"Remove stale external mirror repositories"
	}

	#[instrument(skip(self, ctx), fields(job_id = "mirror-cleanup", stale_after_days = self.stale_after_days))]
	async fn run(&self, ctx: &JobContext) -> Result<JobOutput, JobError> {
		if ctx.cancellation_token.is_cancelled() {
			return Err(JobError::Cancelled);
		}

		let stale_duration = Duration::from_secs(self.stale_after_days as u64 * 24 * 60 * 60);

		let stale_mirrors =
			loom_server_scm_mirror::find_stale_mirrors(self.store.as_ref(), stale_duration)
				.await
				.map_err(|e| JobError::Failed {
					message: e.to_string(),
					retryable: true,
				})?;

		if stale_mirrors.is_empty() {
			return Ok(JobOutput {
				message: "No stale mirrors to clean up".to_string(),
				metadata: Some(serde_json::json!({
					"stale_after_days": self.stale_after_days,
					"mirrors_deleted": 0,
				})),
			});
		}

		let mut deleted = 0;
		let mut errors = Vec::new();

		for mirror in &stale_mirrors {
			if ctx.cancellation_token.is_cancelled() {
				return Err(JobError::Cancelled);
			}

			let repo_path = self.repo_path_for_id(mirror.repo_id);

			match loom_server_scm_mirror::delete_mirror(mirror, &repo_path, self.store.as_ref()).await {
				Ok(()) => {
					deleted += 1;
					tracing::info!(
						mirror_id = %mirror.id,
						platform = ?mirror.platform,
						owner = %mirror.external_owner,
						repo = %mirror.external_repo,
						"Deleted stale mirror"
					);
				}
				Err(e) => {
					tracing::warn!(
						mirror_id = %mirror.id,
						error = %e,
						"Failed to delete stale mirror"
					);
					errors.push(serde_json::json!({
						"mirror_id": mirror.id.to_string(),
						"error": e.to_string(),
					}));
				}
			}
		}

		tracing::info!(
			stale_after_days = self.stale_after_days,
			total = stale_mirrors.len(),
			deleted,
			errors = errors.len(),
			"Mirror cleanup completed"
		);

		if errors.is_empty() {
			Ok(JobOutput {
				message: format!("Deleted {} stale mirrors", deleted),
				metadata: Some(serde_json::json!({
					"stale_after_days": self.stale_after_days,
					"mirrors_found": stale_mirrors.len(),
					"mirrors_deleted": deleted,
				})),
			})
		} else {
			Ok(JobOutput {
				message: format!(
					"Deleted {} stale mirrors with {} errors",
					deleted,
					errors.len()
				),
				metadata: Some(serde_json::json!({
					"stale_after_days": self.stale_after_days,
					"mirrors_found": stale_mirrors.len(),
					"mirrors_deleted": deleted,
					"errors": errors,
				})),
			})
		}
	}
}
