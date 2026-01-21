// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights
// reserved. SPDX-License-Identifier: Proprietary

pub use loom_server_db::{ExternalMirrorStore, MirrorRepository, PushMirrorStore};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::SqlitePool;
use uuid::Uuid;

use crate::error::{MirrorError, Result};
use crate::types::{
	CreateExternalMirror, CreatePushMirror, ExternalMirror, MirrorBranchRule, Platform, PushMirror,
};

pub struct SqlitePushMirrorStore {
	repo: MirrorRepository,
}

impl SqlitePushMirrorStore {
	pub fn new(pool: SqlitePool) -> Self {
		Self {
			repo: MirrorRepository::new(pool),
		}
	}
}

#[async_trait]
impl PushMirrorStore for SqlitePushMirrorStore {
	async fn create(&self, mirror: &CreatePushMirror) -> loom_server_db::Result<PushMirror> {
		self.repo.create_push_mirror(mirror).await
	}

	async fn get_by_id(&self, id: Uuid) -> loom_server_db::Result<Option<PushMirror>> {
		self.repo.get_push_mirror_by_id(id).await
	}

	async fn list_by_repo(&self, repo_id: Uuid) -> loom_server_db::Result<Vec<PushMirror>> {
		self.repo.list_push_mirrors_by_repo(repo_id).await
	}

	async fn delete(&self, id: Uuid) -> loom_server_db::Result<()> {
		self.repo.delete_push_mirror(id).await
	}

	async fn update_push_result(
		&self,
		id: Uuid,
		pushed_at: DateTime<Utc>,
		error: Option<String>,
	) -> loom_server_db::Result<()> {
		self.repo.update_push_result(id, pushed_at, error).await
	}

	async fn list_branch_rules(
		&self,
		mirror_id: Uuid,
	) -> loom_server_db::Result<Vec<MirrorBranchRule>> {
		self.repo.list_branch_rules(mirror_id).await
	}
}

pub struct SqliteExternalMirrorStore {
	repo: MirrorRepository,
}

impl SqliteExternalMirrorStore {
	pub fn new(pool: SqlitePool) -> Self {
		Self {
			repo: MirrorRepository::new(pool),
		}
	}

	pub async fn create(&self, mirror: &CreateExternalMirror) -> Result<ExternalMirror> {
		self
			.repo
			.create_external_mirror(mirror)
			.await
			.map_err(|e| MirrorError::Database(sqlx::Error::Protocol(e.to_string())))
	}

	pub async fn get_by_external(
		&self,
		platform: Platform,
		owner: &str,
		repo: &str,
	) -> Result<Option<ExternalMirror>> {
		self
			.repo
			.get_external_mirror_by_external(platform, owner, repo)
			.await
			.map_err(|e| MirrorError::Database(sqlx::Error::Protocol(e.to_string())))
	}
}

#[async_trait]
impl ExternalMirrorStore for SqliteExternalMirrorStore {
	async fn get_by_id(&self, id: Uuid) -> loom_server_db::Result<Option<ExternalMirror>> {
		self.repo.get_external_mirror_by_id(id).await
	}

	async fn get_by_repo_id(&self, repo_id: Uuid) -> loom_server_db::Result<Option<ExternalMirror>> {
		self.repo.get_external_mirror_by_repo_id(repo_id).await
	}

	async fn find_stale(
		&self,
		stale_threshold: DateTime<Utc>,
	) -> loom_server_db::Result<Vec<ExternalMirror>> {
		self.repo.find_stale_external_mirrors(stale_threshold).await
	}

	async fn list_needing_sync(
		&self,
		sync_threshold: DateTime<Utc>,
		limit: usize,
	) -> loom_server_db::Result<Vec<ExternalMirror>> {
		self
			.repo
			.list_external_mirrors_needing_sync(sync_threshold, limit)
			.await
	}

	async fn delete(&self, id: Uuid) -> loom_server_db::Result<()> {
		self.repo.delete_external_mirror(id).await
	}

	async fn update_last_accessed(&self, id: Uuid, at: DateTime<Utc>) -> loom_server_db::Result<()> {
		self.repo.update_external_mirror_last_accessed(id, at).await
	}

	async fn update_last_synced(&self, id: Uuid, at: DateTime<Utc>) -> loom_server_db::Result<()> {
		self.repo.update_external_mirror_last_synced(id, at).await
	}

	async fn create(&self, mirror: &CreateExternalMirror) -> loom_server_db::Result<ExternalMirror> {
		self.repo.create_external_mirror(mirror).await
	}

	async fn get_by_external(
		&self,
		platform: Platform,
		owner: &str,
		name: &str,
	) -> loom_server_db::Result<Option<ExternalMirror>> {
		self
			.repo
			.get_external_mirror_by_external(platform, owner, name)
			.await
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	async fn create_test_pool() -> SqlitePool {
		let pool = SqlitePool::connect(":memory:").await.unwrap();
		crate::schema::run_test_migrations(&pool).await.unwrap();
		pool
	}

	#[tokio::test]
	async fn test_create_and_get_mirror() {
		let pool = create_test_pool().await;
		let store = SqlitePushMirrorStore::new(pool);

		let create = CreatePushMirror {
			repo_id: Uuid::new_v4(),
			remote_url: "https://github.com/test/repo.git".to_string(),
			credential_key: "mirror:test:github".to_string(),
			enabled: true,
		};

		let created = store.create(&create).await.unwrap();
		assert_eq!(created.remote_url, create.remote_url);
		assert_eq!(created.enabled, true);

		let fetched = store.get_by_id(created.id).await.unwrap().unwrap();
		assert_eq!(fetched.id, created.id);
		assert_eq!(fetched.remote_url, created.remote_url);
	}

	#[tokio::test]
	async fn test_list_by_repo() {
		let pool = create_test_pool().await;
		let store = SqlitePushMirrorStore::new(pool);

		let repo_id = Uuid::new_v4();

		store
			.create(&CreatePushMirror {
				repo_id,
				remote_url: "https://github.com/test/repo1.git".to_string(),
				credential_key: "key1".to_string(),
				enabled: true,
			})
			.await
			.unwrap();

		store
			.create(&CreatePushMirror {
				repo_id,
				remote_url: "https://gitlab.com/test/repo2.git".to_string(),
				credential_key: "key2".to_string(),
				enabled: false,
			})
			.await
			.unwrap();

		let mirrors = store.list_by_repo(repo_id).await.unwrap();
		assert_eq!(mirrors.len(), 2);
	}

	#[tokio::test]
	async fn test_delete_mirror() {
		let pool = create_test_pool().await;
		let store = SqlitePushMirrorStore::new(pool);

		let created = store
			.create(&CreatePushMirror {
				repo_id: Uuid::new_v4(),
				remote_url: "https://github.com/test/repo.git".to_string(),
				credential_key: "key".to_string(),
				enabled: true,
			})
			.await
			.unwrap();

		store.delete(created.id).await.unwrap();

		let fetched = store.get_by_id(created.id).await.unwrap();
		assert!(fetched.is_none());
	}

	#[tokio::test]
	async fn test_delete_nonexistent_returns_not_found() {
		let pool = create_test_pool().await;
		let store = SqlitePushMirrorStore::new(pool);

		let result = store.delete(Uuid::new_v4()).await;
		assert!(result.is_err());
	}

	#[tokio::test]
	async fn test_list_by_repo_ordered_by_created_at_desc() {
		let pool = create_test_pool().await;
		let store = SqlitePushMirrorStore::new(pool);

		let repo_id = Uuid::new_v4();

		let mirror1 = store
			.create(&CreatePushMirror {
				repo_id,
				remote_url: "https://github.com/test/first.git".to_string(),
				credential_key: "key1".to_string(),
				enabled: true,
			})
			.await
			.unwrap();

		tokio::time::sleep(std::time::Duration::from_millis(10)).await;

		let mirror2 = store
			.create(&CreatePushMirror {
				repo_id,
				remote_url: "https://github.com/test/second.git".to_string(),
				credential_key: "key2".to_string(),
				enabled: true,
			})
			.await
			.unwrap();

		tokio::time::sleep(std::time::Duration::from_millis(10)).await;

		let mirror3 = store
			.create(&CreatePushMirror {
				repo_id,
				remote_url: "https://github.com/test/third.git".to_string(),
				credential_key: "key3".to_string(),
				enabled: true,
			})
			.await
			.unwrap();

		let mirrors = store.list_by_repo(repo_id).await.unwrap();
		assert_eq!(mirrors.len(), 3);
		assert_eq!(mirrors[0].id, mirror3.id);
		assert_eq!(mirrors[1].id, mirror2.id);
		assert_eq!(mirrors[2].id, mirror1.id);
	}

	#[tokio::test]
	async fn test_update_push_result_updates_fields() {
		let pool = create_test_pool().await;
		let store = SqlitePushMirrorStore::new(pool);

		let created = store
			.create(&CreatePushMirror {
				repo_id: Uuid::new_v4(),
				remote_url: "https://github.com/test/repo.git".to_string(),
				credential_key: "key".to_string(),
				enabled: true,
			})
			.await
			.unwrap();

		assert!(created.last_pushed_at.is_none());
		assert!(created.last_error.is_none());

		let pushed_at = Utc::now();
		let error_msg = Some("connection refused".to_string());
		store
			.update_push_result(created.id, pushed_at, error_msg.clone())
			.await
			.unwrap();

		let fetched = store.get_by_id(created.id).await.unwrap().unwrap();
		assert!(fetched.last_pushed_at.is_some());
		assert_eq!(fetched.last_error, error_msg);

		let pushed_at2 = Utc::now();
		store
			.update_push_result(created.id, pushed_at2, None)
			.await
			.unwrap();

		let fetched2 = store.get_by_id(created.id).await.unwrap().unwrap();
		assert!(fetched2.last_pushed_at.is_some());
		assert!(fetched2.last_error.is_none());
	}

	#[tokio::test]
	async fn test_update_push_result_missing_returns_not_found() {
		let pool = create_test_pool().await;
		let store = SqlitePushMirrorStore::new(pool);

		let result = store
			.update_push_result(Uuid::new_v4(), Utc::now(), None)
			.await;
		assert!(result.is_err());
	}

	#[tokio::test]
	async fn test_list_branch_rules_returns_rules() {
		let pool = create_test_pool().await;
		let store = SqlitePushMirrorStore::new(pool.clone());

		let created = store
			.create(&CreatePushMirror {
				repo_id: Uuid::new_v4(),
				remote_url: "https://github.com/test/repo.git".to_string(),
				credential_key: "key".to_string(),
				enabled: true,
			})
			.await
			.unwrap();

		let mirror_id_str = created.id.to_string();
		sqlx::query("INSERT INTO mirror_branch_rules (mirror_id, pattern, enabled) VALUES (?, ?, ?)")
			.bind(&mirror_id_str)
			.bind("main")
			.bind(1)
			.execute(&pool)
			.await
			.unwrap();

		sqlx::query("INSERT INTO mirror_branch_rules (mirror_id, pattern, enabled) VALUES (?, ?, ?)")
			.bind(&mirror_id_str)
			.bind("release/*")
			.bind(0)
			.execute(&pool)
			.await
			.unwrap();

		let rules = store.list_branch_rules(created.id).await.unwrap();
		assert_eq!(rules.len(), 2);

		let main_rule = rules.iter().find(|r| r.pattern == "main").unwrap();
		assert!(main_rule.enabled);
		assert_eq!(main_rule.mirror_id, created.id);

		let release_rule = rules.iter().find(|r| r.pattern == "release/*").unwrap();
		assert!(!release_rule.enabled);
	}
}

#[cfg(test)]
mod external_mirror_tests {
	use super::*;

	async fn create_test_pool() -> SqlitePool {
		let pool = SqlitePool::connect(":memory:").await.unwrap();
		crate::schema::run_test_migrations(&pool).await.unwrap();
		pool
	}

	#[tokio::test]
	async fn test_create_and_get_external_mirror() {
		let pool = create_test_pool().await;
		let store = SqliteExternalMirrorStore::new(pool);

		let create = CreateExternalMirror {
			platform: Platform::GitHub,
			external_owner: "torvalds".to_string(),
			external_repo: "linux".to_string(),
			repo_id: Uuid::new_v4(),
		};

		let created = store.create(&create).await.unwrap();
		assert_eq!(created.platform, Platform::GitHub);
		assert_eq!(created.external_owner, "torvalds");
		assert_eq!(created.external_repo, "linux");
		assert!(created.last_accessed_at.is_some());

		let fetched = store.get_by_id(created.id).await.unwrap().unwrap();
		assert_eq!(fetched.id, created.id);
		assert_eq!(fetched.external_owner, "torvalds");
	}

	#[tokio::test]
	async fn test_get_by_repo_id() {
		let pool = create_test_pool().await;
		let store = SqliteExternalMirrorStore::new(pool);

		let repo_id = Uuid::new_v4();
		let create = CreateExternalMirror {
			platform: Platform::GitLab,
			external_owner: "gitlab-org".to_string(),
			external_repo: "gitlab".to_string(),
			repo_id,
		};

		store.create(&create).await.unwrap();

		let fetched = store.get_by_repo_id(repo_id).await.unwrap().unwrap();
		assert_eq!(fetched.repo_id, repo_id);
		assert_eq!(fetched.platform, Platform::GitLab);
	}

	#[tokio::test]
	async fn test_get_by_external() {
		let pool = create_test_pool().await;
		let store = SqliteExternalMirrorStore::new(pool);

		let create = CreateExternalMirror {
			platform: Platform::GitHub,
			external_owner: "rust-lang".to_string(),
			external_repo: "rust".to_string(),
			repo_id: Uuid::new_v4(),
		};

		store.create(&create).await.unwrap();

		let fetched = store
			.get_by_external(Platform::GitHub, "rust-lang", "rust")
			.await
			.unwrap()
			.unwrap();
		assert_eq!(fetched.external_owner, "rust-lang");
		assert_eq!(fetched.external_repo, "rust");
	}

	#[tokio::test]
	async fn test_update_last_accessed() {
		let pool = create_test_pool().await;
		let store = SqliteExternalMirrorStore::new(pool);

		let create = CreateExternalMirror {
			platform: Platform::GitHub,
			external_owner: "owner".to_string(),
			external_repo: "repo".to_string(),
			repo_id: Uuid::new_v4(),
		};

		let created = store.create(&create).await.unwrap();
		let original_accessed = created.last_accessed_at;

		tokio::time::sleep(std::time::Duration::from_millis(10)).await;

		let new_time = Utc::now();
		store
			.update_last_accessed(created.id, new_time)
			.await
			.unwrap();

		let fetched = store.get_by_id(created.id).await.unwrap().unwrap();
		assert!(fetched.last_accessed_at.unwrap() > original_accessed.unwrap());
	}

	#[tokio::test]
	async fn test_find_stale_mirrors() {
		let pool = create_test_pool().await;
		let store = SqliteExternalMirrorStore::new(pool);

		let create1 = CreateExternalMirror {
			platform: Platform::GitHub,
			external_owner: "owner1".to_string(),
			external_repo: "repo1".to_string(),
			repo_id: Uuid::new_v4(),
		};
		store.create(&create1).await.unwrap();

		let threshold = Utc::now() + chrono::Duration::hours(1);
		let stale = store.find_stale(threshold).await.unwrap();
		assert_eq!(stale.len(), 1);

		let threshold_past = Utc::now() - chrono::Duration::hours(1);
		let stale = store.find_stale(threshold_past).await.unwrap();
		assert_eq!(stale.len(), 0);
	}

	#[tokio::test]
	async fn test_delete_external_mirror() {
		let pool = create_test_pool().await;
		let store = SqliteExternalMirrorStore::new(pool);

		let create = CreateExternalMirror {
			platform: Platform::GitHub,
			external_owner: "owner".to_string(),
			external_repo: "repo".to_string(),
			repo_id: Uuid::new_v4(),
		};

		let created = store.create(&create).await.unwrap();
		store.delete(created.id).await.unwrap();

		let fetched = store.get_by_id(created.id).await.unwrap();
		assert!(fetched.is_none());
	}

	#[tokio::test]
	async fn test_update_last_synced() {
		let pool = create_test_pool().await;
		let store = SqliteExternalMirrorStore::new(pool);

		let create = CreateExternalMirror {
			platform: Platform::GitHub,
			external_owner: "owner".to_string(),
			external_repo: "repo".to_string(),
			repo_id: Uuid::new_v4(),
		};

		let created = store.create(&create).await.unwrap();
		assert!(created.last_synced_at.is_none());

		let sync_time = Utc::now();
		store
			.update_last_synced(created.id, sync_time)
			.await
			.unwrap();

		let fetched = store.get_by_id(created.id).await.unwrap().unwrap();
		assert!(fetched.last_synced_at.is_some());
	}

	#[tokio::test]
	async fn test_update_last_accessed_not_found() {
		let pool = create_test_pool().await;
		let store = SqliteExternalMirrorStore::new(pool);

		let result = store.update_last_accessed(Uuid::new_v4(), Utc::now()).await;
		assert!(result.is_err());
	}

	#[tokio::test]
	async fn test_update_last_synced_not_found() {
		let pool = create_test_pool().await;
		let store = SqliteExternalMirrorStore::new(pool);

		let result = store.update_last_synced(Uuid::new_v4(), Utc::now()).await;
		assert!(result.is_err());
	}

	#[tokio::test]
	async fn test_list_needing_sync() {
		let pool = create_test_pool().await;
		let store = SqliteExternalMirrorStore::new(pool);

		// Create mirror that has never been synced
		let create1 = CreateExternalMirror {
			platform: Platform::GitHub,
			external_owner: "never-synced".to_string(),
			external_repo: "repo1".to_string(),
			repo_id: Uuid::new_v4(),
		};
		let mirror1 = store.create(&create1).await.unwrap();
		assert!(mirror1.last_synced_at.is_none());

		// Create mirror and sync it recently
		let create2 = CreateExternalMirror {
			platform: Platform::GitHub,
			external_owner: "recently-synced".to_string(),
			external_repo: "repo2".to_string(),
			repo_id: Uuid::new_v4(),
		};
		let mirror2 = store.create(&create2).await.unwrap();
		store
			.update_last_synced(mirror2.id, Utc::now())
			.await
			.unwrap();

		// Query with threshold in the future - should find the never-synced one
		let threshold = Utc::now() - chrono::Duration::hours(6);
		let needing_sync = store.list_needing_sync(threshold, 100).await.unwrap();

		// Only the never-synced mirror should be returned
		assert_eq!(needing_sync.len(), 1);
		assert_eq!(needing_sync[0].id, mirror1.id);
	}

	#[tokio::test]
	async fn test_list_needing_sync_respects_limit() {
		let pool = create_test_pool().await;
		let store = SqliteExternalMirrorStore::new(pool);

		// Create 5 mirrors that have never been synced
		for i in 0..5 {
			let create = CreateExternalMirror {
				platform: Platform::GitHub,
				external_owner: format!("owner{}", i),
				external_repo: format!("repo{}", i),
				repo_id: Uuid::new_v4(),
			};
			store.create(&create).await.unwrap();
		}

		// Query with limit of 2
		let threshold = Utc::now() - chrono::Duration::hours(6);
		let needing_sync = store.list_needing_sync(threshold, 2).await.unwrap();
		assert_eq!(needing_sync.len(), 2);

		// Query with limit of 10 (more than available)
		let needing_sync = store.list_needing_sync(threshold, 10).await.unwrap();
		assert_eq!(needing_sync.len(), 5);
	}

	#[tokio::test]
	async fn test_list_needing_sync_old_sync_included() {
		let pool = create_test_pool().await;
		let store = SqliteExternalMirrorStore::new(pool);

		// Create mirror and sync it a long time ago
		let create = CreateExternalMirror {
			platform: Platform::GitHub,
			external_owner: "old-sync".to_string(),
			external_repo: "repo".to_string(),
			repo_id: Uuid::new_v4(),
		};
		let mirror = store.create(&create).await.unwrap();

		// Manually set last_synced_at to 24 hours ago
		let old_sync_time = Utc::now() - chrono::Duration::hours(24);
		store
			.update_last_synced(mirror.id, old_sync_time)
			.await
			.unwrap();

		// Query with 6 hour threshold - should find it
		let threshold = Utc::now() - chrono::Duration::hours(6);
		let needing_sync = store.list_needing_sync(threshold, 100).await.unwrap();
		assert_eq!(needing_sync.len(), 1);
		assert_eq!(needing_sync[0].id, mirror.id);
	}
}
