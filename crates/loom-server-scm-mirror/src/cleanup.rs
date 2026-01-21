// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights
// reserved. SPDX-License-Identifier: Proprietary

use std::path::Path;
use std::time::Duration;

use chrono::Utc;
use tracing::{info, instrument, warn};
use uuid::Uuid;

pub use loom_server_db::ExternalMirrorStore;

use crate::error::Result;
use crate::pull::check_repo_exists;
use crate::types::ExternalMirror;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CleanupDecision {
	Deleted,
	Kept,
	RemoteGone,
	Error,
}

#[derive(Debug)]
pub struct CleanupResult {
	pub mirror_id: Uuid,
	pub decision: CleanupDecision,
	pub reason: String,
}

#[instrument(skip(store))]
pub async fn find_stale_mirrors(
	store: &impl ExternalMirrorStore,
	stale_after: Duration,
) -> Result<Vec<ExternalMirror>> {
	let threshold = Utc::now() - chrono::Duration::from_std(stale_after).unwrap_or_default();
	Ok(store.find_stale(threshold).await?)
}

#[instrument(skip(store), fields(mirror_id = %mirror.id, platform = ?mirror.platform))]
pub async fn cleanup_mirror_with_check(
	mirror: &ExternalMirror,
	repo_path: &Path,
	store: &impl ExternalMirrorStore,
	delete_if_stale: bool,
) -> CleanupResult {
	let remote_exists = match check_repo_exists(
		mirror.platform,
		&mirror.external_owner,
		&mirror.external_repo,
	)
	.await
	{
		Ok(exists) => exists,
		Err(e) => {
			warn!(
				mirror_id = %mirror.id,
				error = %e,
				"Failed to check if remote exists, keeping mirror"
			);
			return CleanupResult {
				mirror_id: mirror.id,
				decision: CleanupDecision::Error,
				reason: format!("Failed to check remote: {}", e),
			};
		}
	};

	if !remote_exists {
		info!(
			mirror_id = %mirror.id,
			platform = ?mirror.platform,
			owner = %mirror.external_owner,
			repo = %mirror.external_repo,
			"Remote repository no longer exists (404), deleting mirror"
		);

		if let Err(e) = delete_mirror(mirror, repo_path, store).await {
			return CleanupResult {
				mirror_id: mirror.id,
				decision: CleanupDecision::Error,
				reason: format!("Failed to delete mirror: {}", e),
			};
		}

		return CleanupResult {
			mirror_id: mirror.id,
			decision: CleanupDecision::RemoteGone,
			reason: "Remote repository deleted (404)".to_string(),
		};
	}

	if delete_if_stale {
		info!(
			mirror_id = %mirror.id,
			platform = ?mirror.platform,
			owner = %mirror.external_owner,
			repo = %mirror.external_repo,
			"Deleting stale mirror (remote still exists but configured to delete)"
		);

		if let Err(e) = delete_mirror(mirror, repo_path, store).await {
			return CleanupResult {
				mirror_id: mirror.id,
				decision: CleanupDecision::Error,
				reason: format!("Failed to delete mirror: {}", e),
			};
		}

		return CleanupResult {
			mirror_id: mirror.id,
			decision: CleanupDecision::Deleted,
			reason: "Stale mirror deleted (configured to delete even if remote exists)".to_string(),
		};
	}

	info!(
		mirror_id = %mirror.id,
		platform = ?mirror.platform,
		owner = %mirror.external_owner,
		repo = %mirror.external_repo,
		"Keeping stale mirror (remote still exists)"
	);

	CleanupResult {
		mirror_id: mirror.id,
		decision: CleanupDecision::Kept,
		reason: "Remote still exists, keeping mirror".to_string(),
	}
}

#[instrument(skip(store), fields(mirror_id = %mirror.id, platform = ?mirror.platform))]
pub async fn delete_mirror(
	mirror: &ExternalMirror,
	repo_path: &Path,
	store: &impl ExternalMirrorStore,
) -> Result<()> {
	if repo_path.exists() {
		info!(path = ?repo_path, "Removing mirror repository from disk");
		if let Err(e) = std::fs::remove_dir_all(repo_path) {
			warn!(path = ?repo_path, error = %e, "Failed to remove repository directory");
		}
	}

	store.delete(mirror.id).await?;

	info!(
		platform = ?mirror.platform,
		owner = %mirror.external_owner,
		repo = %mirror.external_repo,
		"External mirror deleted"
	);

	Ok(())
}

pub async fn touch_mirror(store: &impl ExternalMirrorStore, id: Uuid) -> Result<()> {
	Ok(store.update_last_accessed(id, Utc::now()).await?)
}

pub async fn run_cleanup_job(
	store: &impl ExternalMirrorStore,
	stale_after: Duration,
	mirrors_base_path: &Path,
	delete_if_stale: bool,
) -> Vec<CleanupResult> {
	let stale_mirrors = match find_stale_mirrors(store, stale_after).await {
		Ok(mirrors) => mirrors,
		Err(e) => {
			warn!(error = %e, "Failed to find stale mirrors");
			return vec![];
		}
	};

	info!(
		count = stale_mirrors.len(),
		"Found stale mirrors for cleanup"
	);

	let mut results = Vec::new();
	for mirror in &stale_mirrors {
		let repo_path = mirrors_base_path
			.join(mirror.platform.as_str())
			.join(&mirror.external_owner)
			.join(&mirror.external_repo);

		let result = cleanup_mirror_with_check(mirror, &repo_path, store, delete_if_stale).await;

		info!(
			mirror_id = %result.mirror_id,
			decision = ?result.decision,
			reason = %result.reason,
			"Cleanup decision made"
		);

		results.push(result);
	}

	results
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::types::Platform;
	use async_trait::async_trait;
	use chrono::DateTime;
	use std::sync::{Arc, Mutex};

	struct FakeExternalMirrorStore {
		mirrors: Arc<Mutex<Vec<ExternalMirror>>>,
		deleted_ids: Arc<Mutex<Vec<Uuid>>>,
		accessed_updates: Arc<Mutex<Vec<(Uuid, DateTime<Utc>)>>>,
		synced_updates: Arc<Mutex<Vec<(Uuid, DateTime<Utc>)>>>,
	}

	impl FakeExternalMirrorStore {
		fn new() -> Self {
			Self {
				mirrors: Arc::new(Mutex::new(Vec::new())),
				deleted_ids: Arc::new(Mutex::new(Vec::new())),
				accessed_updates: Arc::new(Mutex::new(Vec::new())),
				synced_updates: Arc::new(Mutex::new(Vec::new())),
			}
		}

		fn add_mirror(&self, mirror: ExternalMirror) {
			self.mirrors.lock().unwrap().push(mirror);
		}

		fn was_deleted(&self, id: Uuid) -> bool {
			self.deleted_ids.lock().unwrap().contains(&id)
		}

		fn get_accessed_updates(&self) -> Vec<(Uuid, DateTime<Utc>)> {
			self.accessed_updates.lock().unwrap().clone()
		}
	}

	#[async_trait]
	impl ExternalMirrorStore for FakeExternalMirrorStore {
		async fn create(
			&self,
			_mirror: &loom_server_db::CreateExternalMirror,
		) -> loom_server_db::Result<ExternalMirror> {
			unimplemented!()
		}

		async fn get_by_id(&self, id: Uuid) -> loom_server_db::Result<Option<ExternalMirror>> {
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
			repo_id: Uuid,
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
			stale_threshold: DateTime<Utc>,
		) -> loom_server_db::Result<Vec<ExternalMirror>> {
			Ok(
				self
					.mirrors
					.lock()
					.unwrap()
					.iter()
					.filter(|m| {
						m.last_accessed_at
							.map(|t| t < stale_threshold)
							.unwrap_or(true)
					})
					.cloned()
					.collect(),
			)
		}

		async fn list_needing_sync(
			&self,
			sync_threshold: DateTime<Utc>,
			limit: usize,
		) -> loom_server_db::Result<Vec<ExternalMirror>> {
			Ok(
				self
					.mirrors
					.lock()
					.unwrap()
					.iter()
					.filter(|m| m.last_synced_at.map(|t| t < sync_threshold).unwrap_or(true))
					.take(limit)
					.cloned()
					.collect(),
			)
		}

		async fn delete(&self, id: Uuid) -> loom_server_db::Result<()> {
			self.deleted_ids.lock().unwrap().push(id);
			self.mirrors.lock().unwrap().retain(|m| m.id != id);
			Ok(())
		}

		async fn update_last_accessed(
			&self,
			id: Uuid,
			at: DateTime<Utc>,
		) -> loom_server_db::Result<()> {
			self.accessed_updates.lock().unwrap().push((id, at));
			Ok(())
		}

		async fn update_last_synced(&self, id: Uuid, at: DateTime<Utc>) -> loom_server_db::Result<()> {
			self.synced_updates.lock().unwrap().push((id, at));
			Ok(())
		}
	}

	fn make_mirror(id: Uuid, last_accessed: Option<DateTime<Utc>>) -> ExternalMirror {
		ExternalMirror {
			id,
			platform: Platform::GitHub,
			external_owner: "test-owner".to_string(),
			external_repo: "test-repo".to_string(),
			repo_id: Uuid::new_v4(),
			last_synced_at: None,
			last_accessed_at: last_accessed,
			created_at: Utc::now(),
		}
	}

	#[test]
	fn test_cleanup_decision_variants() {
		assert_eq!(CleanupDecision::Deleted, CleanupDecision::Deleted);
		assert_eq!(CleanupDecision::Kept, CleanupDecision::Kept);
		assert_eq!(CleanupDecision::RemoteGone, CleanupDecision::RemoteGone);
		assert_eq!(CleanupDecision::Error, CleanupDecision::Error);
		assert_ne!(CleanupDecision::Deleted, CleanupDecision::Kept);
	}

	#[test]
	fn test_cleanup_result_structure() {
		let result = CleanupResult {
			mirror_id: uuid::Uuid::new_v4(),
			decision: CleanupDecision::Kept,
			reason: "Remote still exists".to_string(),
		};

		assert_eq!(result.decision, CleanupDecision::Kept);
		assert!(result.reason.contains("exists"));
	}

	#[tokio::test]
	async fn test_find_stale_mirrors_threshold() {
		let store = FakeExternalMirrorStore::new();

		let fresh_id = Uuid::new_v4();
		let stale_id = Uuid::new_v4();
		let never_accessed_id = Uuid::new_v4();

		let fresh_mirror = make_mirror(fresh_id, Some(Utc::now()));
		let stale_mirror = make_mirror(stale_id, Some(Utc::now() - chrono::Duration::days(10)));
		let never_accessed_mirror = make_mirror(never_accessed_id, None);

		store.add_mirror(fresh_mirror);
		store.add_mirror(stale_mirror);
		store.add_mirror(never_accessed_mirror);

		let stale_after = Duration::from_secs(7 * 24 * 60 * 60);
		let stale_mirrors = find_stale_mirrors(&store, stale_after).await.unwrap();

		assert_eq!(stale_mirrors.len(), 2);
		let stale_ids: Vec<Uuid> = stale_mirrors.iter().map(|m| m.id).collect();
		assert!(stale_ids.contains(&stale_id));
		assert!(stale_ids.contains(&never_accessed_id));
		assert!(!stale_ids.contains(&fresh_id));
	}

	#[tokio::test]
	async fn test_touch_mirror_updates_accessed_time() {
		let store = FakeExternalMirrorStore::new();
		let mirror_id = Uuid::new_v4();

		let before = Utc::now();
		touch_mirror(&store, mirror_id).await.unwrap();
		let after = Utc::now();

		let updates = store.get_accessed_updates();
		assert_eq!(updates.len(), 1);
		assert_eq!(updates[0].0, mirror_id);
		assert!(updates[0].1 >= before && updates[0].1 <= after);
	}

	#[tokio::test]
	async fn test_delete_mirror_removes_directory_and_db_record() {
		let store = FakeExternalMirrorStore::new();
		let mirror_id = Uuid::new_v4();
		let mirror = make_mirror(mirror_id, Some(Utc::now()));
		store.add_mirror(mirror.clone());

		let temp_dir = tempfile::tempdir().unwrap();
		let repo_path = temp_dir.path().join("test-repo");
		std::fs::create_dir_all(&repo_path).unwrap();
		assert!(repo_path.exists());

		delete_mirror(&mirror, &repo_path, &store).await.unwrap();

		assert!(!repo_path.exists());
		assert!(store.was_deleted(mirror_id));
		assert!(store.get_by_id(mirror_id).await.unwrap().is_none());
	}

	#[tokio::test]
	async fn test_delete_mirror_handles_nonexistent_directory() {
		let store = FakeExternalMirrorStore::new();
		let mirror_id = Uuid::new_v4();
		let mirror = make_mirror(mirror_id, Some(Utc::now()));
		store.add_mirror(mirror.clone());

		let nonexistent_path = std::path::Path::new("/tmp/nonexistent-loom-test-path");

		delete_mirror(&mirror, nonexistent_path, &store)
			.await
			.unwrap();

		assert!(store.was_deleted(mirror_id));
	}

	#[tokio::test]
	async fn test_find_stale_mirrors_empty_store() {
		let store = FakeExternalMirrorStore::new();
		let stale_after = Duration::from_secs(7 * 24 * 60 * 60);

		let stale_mirrors = find_stale_mirrors(&store, stale_after).await.unwrap();

		assert!(stale_mirrors.is_empty());
	}

	#[tokio::test]
	async fn test_find_stale_mirrors_all_fresh() {
		let store = FakeExternalMirrorStore::new();

		for _ in 0..3 {
			store.add_mirror(make_mirror(Uuid::new_v4(), Some(Utc::now())));
		}

		let stale_after = Duration::from_secs(7 * 24 * 60 * 60);
		let stale_mirrors = find_stale_mirrors(&store, stale_after).await.unwrap();

		assert!(stale_mirrors.is_empty());
	}
}
