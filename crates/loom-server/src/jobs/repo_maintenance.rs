// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use std::path::PathBuf;
use std::time::Duration;

use async_trait::async_trait;
use loom_server_db::scm::ScmRepository;
use loom_server_jobs::{Job, JobContext, JobError, JobOutput};
use loom_server_scm::MaintenanceTask;
use tracing::instrument;
use uuid::Uuid;

pub struct RepoMaintenanceJob {
	repos_dir: PathBuf,
	repo_id: Uuid,
	task: MaintenanceTask,
}

impl RepoMaintenanceJob {
	pub fn new(repos_dir: PathBuf, repo_id: Uuid, task: MaintenanceTask) -> Self {
		Self {
			repos_dir,
			repo_id,
			task,
		}
	}

	fn repo_path(&self) -> PathBuf {
		let id_str = self.repo_id.to_string();
		let prefix = &id_str[..2];
		self.repos_dir.join(prefix).join(&id_str).join("git")
	}
}

#[async_trait]
impl Job for RepoMaintenanceJob {
	fn id(&self) -> &str {
		"repo-maintenance"
	}

	fn name(&self) -> &str {
		"Repository Maintenance"
	}

	fn description(&self) -> &str {
		"Run git maintenance tasks on a single repository"
	}

	#[instrument(skip(self, ctx), fields(job_id = "repo-maintenance", repo_id = %self.repo_id, task = ?self.task))]
	async fn run(&self, ctx: &JobContext) -> Result<JobOutput, JobError> {
		if ctx.cancellation_token.is_cancelled() {
			return Err(JobError::Cancelled);
		}

		let repo_path = self.repo_path();
		let task = self.task;

		let result =
			tokio::task::spawn_blocking(move || loom_server_scm::run_maintenance(&repo_path, task))
				.await
				.map_err(|e| JobError::Failed {
					message: format!("Task join error: {}", e),
					retryable: true,
				})?
				.map_err(|e| JobError::Failed {
					message: e.to_string(),
					retryable: true,
				})?;

		if result.success {
			tracing::info!(
				repo_id = %self.repo_id,
				task = ?task,
				"Maintenance completed successfully"
			);

			Ok(JobOutput {
				message: format!(
					"Maintenance task '{}' completed for repo {}",
					task.as_str(),
					self.repo_id
				),
				metadata: Some(serde_json::json!({
					"repo_id": self.repo_id.to_string(),
					"task": task.as_str(),
					"fsck_issues": result.fsck_issues,
				})),
			})
		} else {
			Err(JobError::Failed {
				message: result.error.unwrap_or_else(|| "Unknown error".to_string()),
				retryable: false,
			})
		}
	}
}

pub struct GlobalMaintenanceJob {
	scm_repo: ScmRepository,
	repos_dir: PathBuf,
	task: MaintenanceTask,
	stagger_ms: u64,
}

impl GlobalMaintenanceJob {
	pub fn new(
		scm_repo: ScmRepository,
		repos_dir: PathBuf,
		task: MaintenanceTask,
		stagger_ms: u64,
	) -> Self {
		Self {
			scm_repo,
			repos_dir,
			task,
			stagger_ms,
		}
	}
}

#[async_trait]
impl Job for GlobalMaintenanceJob {
	fn id(&self) -> &str {
		"global-maintenance"
	}

	fn name(&self) -> &str {
		"Global Repository Maintenance"
	}

	fn description(&self) -> &str {
		"Run git maintenance tasks on all repositories"
	}

	#[instrument(skip(self, ctx), fields(job_id = "global-maintenance", task = ?self.task))]
	async fn run(&self, ctx: &JobContext) -> Result<JobOutput, JobError> {
		if ctx.cancellation_token.is_cancelled() {
			return Err(JobError::Cancelled);
		}

		let repo_ids = self
			.scm_repo
			.list_all_repo_ids()
			.await
			.map_err(|e| JobError::Failed {
				message: e.to_string(),
				retryable: true,
			})?;

		let repo_paths: Vec<(Uuid, PathBuf)> = repo_ids
			.into_iter()
			.map(|id| {
				let id_str = id.to_string();
				let prefix = &id_str[..2];
				let path = self.repos_dir.join(prefix).join(&id_str).join("git");
				(id, path)
			})
			.filter(|(_, path)| path.exists())
			.collect();

		if repo_paths.is_empty() {
			return Ok(JobOutput {
				message: "No repositories to maintain".to_string(),
				metadata: Some(serde_json::json!({
					"repos_processed": 0,
					"task": self.task.as_str(),
				})),
			});
		}

		let stagger_delay = Duration::from_millis(self.stagger_ms);
		let results =
			loom_server_scm::run_global_sweep(&self.repos_dir, self.task, stagger_delay, repo_paths)
				.await;

		let total = results.len();
		let successful = results.iter().filter(|r| r.result.is_ok()).count();
		let failed = total - successful;

		let errors: Vec<_> = results
			.iter()
			.filter_map(|r| {
				r.result.as_ref().err().map(|e| {
					serde_json::json!({
						"repo_id": r.repo_id.to_string(),
						"error": e.to_string(),
					})
				})
			})
			.collect();

		tracing::info!(
			task = ?self.task,
			total,
			successful,
			failed,
			"Global maintenance completed"
		);

		if failed > 0 {
			Err(JobError::Failed {
				message: format!(
					"Global maintenance completed with {} failures out of {} repos",
					failed, total
				),
				retryable: false,
			})
		} else {
			Ok(JobOutput {
				message: format!(
					"Global maintenance '{}' completed for {} repositories",
					self.task.as_str(),
					total
				),
				metadata: Some(serde_json::json!({
					"task": self.task.as_str(),
					"repos_processed": total,
					"successful": successful,
					"failed": failed,
					"errors": errors,
				})),
			})
		}
	}
}
