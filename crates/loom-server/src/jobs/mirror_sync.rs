// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! Background job for scheduled mirror synchronization.
//!
//! This job periodically syncs external mirrors (GitHub/GitLab repos) to keep
//! local copies up to date with their upstream sources.

use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use chrono::Utc;
use loom_server_jobs::{Job, JobContext, JobError, JobOutput};
use loom_server_scm::RepoStore;
use loom_server_scm_mirror::{ExternalMirrorStore, PullResult};
use tracing::instrument;

/// Job that syncs external mirrors with their upstream repositories.
///
/// This job finds all external mirrors that haven't been synced within
/// a configurable interval and pulls updates from their upstream sources.
pub struct MirrorSyncJob<S: ExternalMirrorStore, R: RepoStore> {
	store: Arc<S>,
	repo_store: Arc<R>,
	repos_dir: PathBuf,
	sync_interval_hours: u32,
	max_mirrors_per_run: usize,
}

impl<S: ExternalMirrorStore + 'static, R: RepoStore + 'static> MirrorSyncJob<S, R> {
	pub fn new(
		store: Arc<S>,
		repo_store: Arc<R>,
		repos_dir: PathBuf,
		sync_interval_hours: u32,
		max_mirrors_per_run: usize,
	) -> Self {
		Self {
			store,
			repo_store,
			repos_dir,
			sync_interval_hours,
			max_mirrors_per_run,
		}
	}

	fn repo_path_for_id(&self, repo_id: uuid::Uuid) -> PathBuf {
		let id_str = repo_id.to_string();
		let prefix = &id_str[..2];
		self.repos_dir.join(prefix).join(&id_str).join("git")
	}
}

#[async_trait]
impl<S: ExternalMirrorStore + 'static, R: RepoStore + 'static> Job for MirrorSyncJob<S, R> {
	fn id(&self) -> &str {
		"mirror-sync"
	}

	fn name(&self) -> &str {
		"Mirror Sync"
	}

	fn description(&self) -> &str {
		"Synchronize external mirrors with upstream repositories"
	}

	#[instrument(skip(self, ctx), fields(job_id = "mirror-sync", sync_interval_hours = self.sync_interval_hours))]
	async fn run(&self, ctx: &JobContext) -> Result<JobOutput, JobError> {
		if ctx.cancellation_token.is_cancelled() {
			return Err(JobError::Cancelled);
		}

		let sync_threshold = Utc::now() - chrono::Duration::hours(self.sync_interval_hours as i64);

		// Get mirrors that need syncing (not synced within the interval)
		let mirrors_needing_sync = self
			.store
			.list_needing_sync(sync_threshold, self.max_mirrors_per_run)
			.await
			.map_err(|e| JobError::Failed {
				message: format!("Failed to list mirrors needing sync: {}", e),
				retryable: true,
			})?;

		if mirrors_needing_sync.is_empty() {
			return Ok(JobOutput {
				message: "No mirrors need syncing".to_string(),
				metadata: Some(serde_json::json!({
					"sync_interval_hours": self.sync_interval_hours,
					"mirrors_synced": 0,
				})),
			});
		}

		let mut synced = 0;
		let mut updated = 0;
		let mut no_changes = 0;
		let mut recloned = 0;
		let mut errors = Vec::new();

		for mirror in &mirrors_needing_sync {
			if ctx.cancellation_token.is_cancelled() {
				return Err(JobError::Cancelled);
			}

			// Verify the repo still exists
			let repo = match self.repo_store.get_by_id(mirror.repo_id).await {
				Ok(Some(repo)) => repo,
				Ok(None) => {
					tracing::warn!(
						mirror_id = %mirror.id,
						repo_id = %mirror.repo_id,
						"Mirror's repository no longer exists, skipping"
					);
					continue;
				}
				Err(e) => {
					tracing::warn!(
						mirror_id = %mirror.id,
						error = %e,
						"Failed to get mirror's repository"
					);
					errors.push(serde_json::json!({
						"mirror_id": mirror.id.to_string(),
						"error": format!("Failed to get repository: {}", e),
					}));
					continue;
				}
			};

			let repo_path = self.repo_path_for_id(repo.id);

			// Pull updates from upstream
			match loom_server_scm_mirror::pull_mirror_with_recovery(
				mirror.platform,
				&mirror.external_owner,
				&mirror.external_repo,
				&repo_path,
			)
			.await
			{
				Ok(result) => {
					synced += 1;

					match result {
						PullResult::Updated => {
							updated += 1;
							tracing::info!(
								mirror_id = %mirror.id,
								platform = ?mirror.platform,
								owner = %mirror.external_owner,
								repo = %mirror.external_repo,
								"Mirror updated successfully"
							);
						}
						PullResult::NoChanges => {
							no_changes += 1;
							tracing::debug!(
								mirror_id = %mirror.id,
								"Mirror already up to date"
							);
						}
						PullResult::Recloned => {
							recloned += 1;
							tracing::info!(
								mirror_id = %mirror.id,
								"Mirror recloned due to divergence"
							);
						}
						PullResult::Error(ref msg) => {
							errors.push(serde_json::json!({
								"mirror_id": mirror.id.to_string(),
								"error": msg,
							}));
							tracing::warn!(
								mirror_id = %mirror.id,
								error = %msg,
								"Mirror sync returned error"
							);
						}
					}

					// Update last_synced_at timestamp
					if let Err(e) = self.store.update_last_synced(mirror.id, Utc::now()).await {
						tracing::warn!(
							mirror_id = %mirror.id,
							error = %e,
							"Failed to update last_synced_at"
						);
					}
				}
				Err(e) => {
					tracing::warn!(
						mirror_id = %mirror.id,
						platform = ?mirror.platform,
						owner = %mirror.external_owner,
						repo = %mirror.external_repo,
						error = %e,
						"Failed to sync mirror"
					);
					errors.push(serde_json::json!({
						"mirror_id": mirror.id.to_string(),
						"error": e.to_string(),
					}));
				}
			}
		}

		tracing::info!(
			sync_interval_hours = self.sync_interval_hours,
			total_found = mirrors_needing_sync.len(),
			synced,
			updated,
			no_changes,
			recloned,
			errors = errors.len(),
			"Mirror sync completed"
		);

		let message = if errors.is_empty() {
			format!(
				"Synced {} mirrors ({} updated, {} unchanged, {} recloned)",
				synced, updated, no_changes, recloned
			)
		} else {
			format!(
				"Synced {} mirrors with {} errors ({} updated, {} unchanged, {} recloned)",
				synced,
				errors.len(),
				updated,
				no_changes,
				recloned
			)
		};

		Ok(JobOutput {
			message,
			metadata: Some(serde_json::json!({
				"sync_interval_hours": self.sync_interval_hours,
				"mirrors_found": mirrors_needing_sync.len(),
				"mirrors_synced": synced,
				"mirrors_updated": updated,
				"mirrors_no_changes": no_changes,
				"mirrors_recloned": recloned,
				"errors": errors,
			})),
		})
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use chrono::DateTime;
	use loom_server_db::{ExternalMirror, Platform};
	use loom_server_jobs::TriggerSource;
	use loom_server_scm::{Repository, Visibility};
	use std::sync::Mutex;

	struct MockExternalMirrorStore {
		mirrors: Mutex<Vec<ExternalMirror>>,
		synced_updates: Mutex<Vec<(uuid::Uuid, DateTime<Utc>)>>,
	}

	impl MockExternalMirrorStore {
		fn new() -> Self {
			Self {
				mirrors: Mutex::new(Vec::new()),
				synced_updates: Mutex::new(Vec::new()),
			}
		}

		fn add_mirror(&self, mirror: ExternalMirror) {
			self.mirrors.lock().unwrap().push(mirror);
		}

		fn get_synced_updates(&self) -> Vec<(uuid::Uuid, DateTime<Utc>)> {
			self.synced_updates.lock().unwrap().clone()
		}
	}

	#[async_trait]
	impl ExternalMirrorStore for MockExternalMirrorStore {
		async fn create(
			&self,
			_mirror: &loom_server_db::CreateExternalMirror,
		) -> loom_server_db::Result<ExternalMirror> {
			unimplemented!()
		}

		async fn get_by_id(&self, id: uuid::Uuid) -> loom_server_db::Result<Option<ExternalMirror>> {
			Ok(
				self
					.mirrors
					.lock()
					.unwrap()
					.iter()
					.find(|m| m.id == id)
					.cloned(),
			)
		}

		async fn get_by_repo_id(
			&self,
			repo_id: uuid::Uuid,
		) -> loom_server_db::Result<Option<ExternalMirror>> {
			Ok(
				self
					.mirrors
					.lock()
					.unwrap()
					.iter()
					.find(|m| m.repo_id == repo_id)
					.cloned(),
			)
		}

		async fn get_by_external(
			&self,
			_platform: Platform,
			_owner: &str,
			_repo: &str,
		) -> loom_server_db::Result<Option<ExternalMirror>> {
			unimplemented!()
		}

		async fn find_stale(
			&self,
			_stale_threshold: DateTime<Utc>,
		) -> loom_server_db::Result<Vec<ExternalMirror>> {
			unimplemented!()
		}

		async fn list_needing_sync(
			&self,
			sync_threshold: DateTime<Utc>,
			limit: usize,
		) -> loom_server_db::Result<Vec<ExternalMirror>> {
			let mirrors = self.mirrors.lock().unwrap();
			Ok(
				mirrors
					.iter()
					.filter(|m| m.last_synced_at.map(|t| t < sync_threshold).unwrap_or(true))
					.take(limit)
					.cloned()
					.collect(),
			)
		}

		async fn delete(&self, _id: uuid::Uuid) -> loom_server_db::Result<()> {
			unimplemented!()
		}

		async fn update_last_accessed(
			&self,
			_id: uuid::Uuid,
			_at: DateTime<Utc>,
		) -> loom_server_db::Result<()> {
			Ok(())
		}

		async fn update_last_synced(
			&self,
			id: uuid::Uuid,
			at: DateTime<Utc>,
		) -> loom_server_db::Result<()> {
			self.synced_updates.lock().unwrap().push((id, at));
			Ok(())
		}
	}

	struct MockRepoStore {
		repos: Mutex<Vec<Repository>>,
	}

	impl MockRepoStore {
		fn new() -> Self {
			Self {
				repos: Mutex::new(Vec::new()),
			}
		}

		#[allow(dead_code)]
		fn add_repo(&self, repo: Repository) {
			self.repos.lock().unwrap().push(repo);
		}
	}

	#[async_trait]
	impl RepoStore for MockRepoStore {
		async fn create(&self, _repo: &Repository) -> loom_server_scm::Result<Repository> {
			unimplemented!()
		}

		async fn get_by_id(&self, id: uuid::Uuid) -> loom_server_scm::Result<Option<Repository>> {
			Ok(
				self
					.repos
					.lock()
					.unwrap()
					.iter()
					.find(|r| r.id == id)
					.cloned(),
			)
		}

		async fn get_by_owner_and_name(
			&self,
			_owner_type: loom_server_scm::OwnerType,
			_owner_id: uuid::Uuid,
			_name: &str,
		) -> loom_server_scm::Result<Option<Repository>> {
			unimplemented!()
		}

		async fn list_by_owner(
			&self,
			_owner_type: loom_server_scm::OwnerType,
			_owner_id: uuid::Uuid,
		) -> loom_server_scm::Result<Vec<Repository>> {
			unimplemented!()
		}

		async fn update(&self, _repo: &Repository) -> loom_server_scm::Result<Repository> {
			unimplemented!()
		}

		async fn soft_delete(&self, _id: uuid::Uuid) -> loom_server_scm::Result<()> {
			unimplemented!()
		}

		async fn hard_delete(&self, _id: uuid::Uuid) -> loom_server_scm::Result<()> {
			unimplemented!()
		}
	}

	fn make_test_mirror(repo_id: uuid::Uuid, last_synced: Option<DateTime<Utc>>) -> ExternalMirror {
		ExternalMirror {
			id: uuid::Uuid::new_v4(),
			platform: Platform::GitHub,
			external_owner: "test-owner".to_string(),
			external_repo: "test-repo".to_string(),
			repo_id,
			last_synced_at: last_synced,
			last_accessed_at: Some(Utc::now()),
			created_at: Utc::now(),
		}
	}

	#[allow(dead_code)]
	fn make_test_repo(id: uuid::Uuid) -> Repository {
		Repository {
			id,
			owner_type: loom_server_scm::OwnerType::User,
			owner_id: uuid::Uuid::new_v4(),
			name: "test-repo".to_string(),
			visibility: Visibility::Private,
			default_branch: "main".to_string(),
			deleted_at: None,
			created_at: Utc::now(),
			updated_at: Utc::now(),
		}
	}

	fn make_test_context() -> JobContext {
		JobContext {
			run_id: uuid::Uuid::new_v4().to_string(),
			triggered_by: TriggerSource::Schedule,
			cancellation_token: loom_server_jobs::CancellationToken::new(),
		}
	}

	#[test]
	fn test_job_id() {
		let mirror_store = Arc::new(MockExternalMirrorStore::new());
		let repo_store = Arc::new(MockRepoStore::new());
		let job = MirrorSyncJob::new(
			mirror_store,
			repo_store,
			PathBuf::from("/tmp/repos"),
			6,
			100,
		);

		assert_eq!(job.id(), "mirror-sync");
		assert_eq!(job.name(), "Mirror Sync");
		assert_eq!(
			job.description(),
			"Synchronize external mirrors with upstream repositories"
		);
	}

	#[test]
	fn test_repo_path_for_id() {
		let mirror_store = Arc::new(MockExternalMirrorStore::new());
		let repo_store = Arc::new(MockRepoStore::new());
		let job = MirrorSyncJob::new(
			mirror_store,
			repo_store,
			PathBuf::from("/data/repos"),
			6,
			100,
		);

		let id = uuid::Uuid::parse_str("12345678-1234-1234-1234-123456789abc").unwrap();
		let path = job.repo_path_for_id(id);

		// Should be /data/repos/12/12345678-1234-1234-1234-123456789abc/git
		assert!(path.to_str().unwrap().contains("/12/"));
		assert!(path.to_str().unwrap().ends_with("/git"));
	}

	#[tokio::test]
	async fn test_no_mirrors_needing_sync() {
		let mirror_store = Arc::new(MockExternalMirrorStore::new());
		let repo_store = Arc::new(MockRepoStore::new());
		let job = MirrorSyncJob::new(
			mirror_store,
			repo_store,
			PathBuf::from("/tmp/repos"),
			6,
			100,
		);

		let ctx = make_test_context();
		let result = job.run(&ctx).await.unwrap();
		assert_eq!(result.message, "No mirrors need syncing");

		let metadata = result.metadata.unwrap();
		assert_eq!(metadata["mirrors_synced"], 0);
	}

	#[tokio::test]
	async fn test_list_needing_sync_respects_threshold() {
		let mirror_store = MockExternalMirrorStore::new();

		let repo_id1 = uuid::Uuid::new_v4();
		let repo_id2 = uuid::Uuid::new_v4();
		let repo_id3 = uuid::Uuid::new_v4();

		// Never synced - should be included
		mirror_store.add_mirror(make_test_mirror(repo_id1, None));

		// Synced recently - should NOT be included
		mirror_store.add_mirror(make_test_mirror(repo_id2, Some(Utc::now())));

		// Synced long ago - should be included
		mirror_store.add_mirror(make_test_mirror(
			repo_id3,
			Some(Utc::now() - chrono::Duration::hours(12)),
		));

		let sync_threshold = Utc::now() - chrono::Duration::hours(6);
		let needing_sync = mirror_store
			.list_needing_sync(sync_threshold, 100)
			.await
			.unwrap();

		assert_eq!(needing_sync.len(), 2);
		let repo_ids: Vec<_> = needing_sync.iter().map(|m| m.repo_id).collect();
		assert!(repo_ids.contains(&repo_id1));
		assert!(!repo_ids.contains(&repo_id2));
		assert!(repo_ids.contains(&repo_id3));
	}

	#[tokio::test]
	async fn test_list_needing_sync_respects_limit() {
		let mirror_store = MockExternalMirrorStore::new();

		// Add 5 mirrors that all need syncing
		for _ in 0..5 {
			mirror_store.add_mirror(make_test_mirror(uuid::Uuid::new_v4(), None));
		}

		let sync_threshold = Utc::now() - chrono::Duration::hours(6);
		let needing_sync = mirror_store
			.list_needing_sync(sync_threshold, 2)
			.await
			.unwrap();

		assert_eq!(needing_sync.len(), 2);
	}

	#[tokio::test]
	async fn test_job_cancelled() {
		let mirror_store = Arc::new(MockExternalMirrorStore::new());
		let repo_store = Arc::new(MockRepoStore::new());
		let job = MirrorSyncJob::new(
			mirror_store,
			repo_store,
			PathBuf::from("/tmp/repos"),
			6,
			100,
		);

		let ctx = make_test_context();
		ctx.cancellation_token.cancel();

		let result = job.run(&ctx).await;
		assert!(matches!(result, Err(JobError::Cancelled)));
	}

	#[tokio::test]
	async fn test_mirror_without_repo_skipped() {
		let mirror_store = Arc::new(MockExternalMirrorStore::new());
		let repo_store = Arc::new(MockRepoStore::new());

		// Add mirror without corresponding repo
		let repo_id = uuid::Uuid::new_v4();
		mirror_store.add_mirror(make_test_mirror(repo_id, None));
		// Don't add repo to repo_store

		let job = MirrorSyncJob::new(
			mirror_store.clone(),
			repo_store,
			PathBuf::from("/tmp/repos"),
			6,
			100,
		);

		let ctx = make_test_context();
		let result = job.run(&ctx).await.unwrap();

		// Should complete without syncing anything (repo not found)
		let metadata = result.metadata.unwrap();
		assert_eq!(metadata["mirrors_synced"], 0);
		// No synced updates should have occurred
		assert!(mirror_store.get_synced_updates().is_empty());
	}
}
