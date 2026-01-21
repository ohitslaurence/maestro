// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights
// reserved. SPDX-License-Identifier: Proprietary

//! Thread repository for database operations.
//!
//! This module provides core database operations for thread/conversation persistence.
//! For extended functionality (CSE cache, GitHub integration), see loom-server's
//! ThreadRepository which wraps this one.

use async_trait::async_trait;
use loom_common_thread::{Thread, ThreadId, ThreadSummary, ThreadVisibility};
use sqlx::{sqlite::SqlitePool, Row};

use crate::error::DbError;

/// A search result hit with relevance score.
#[derive(Debug, Clone)]
pub struct ThreadSearchHit {
	pub summary: ThreadSummary,
	pub score: f64,
}

/// Trait for thread database operations.
#[async_trait]
pub trait ThreadStore: Send + Sync {
	async fn upsert(&self, thread: &Thread, expected_version: Option<u64>)
		-> Result<Thread, DbError>;

	async fn get(&self, id: &ThreadId) -> Result<Option<Thread>, DbError>;

	async fn list(
		&self,
		workspace: Option<&str>,
		limit: u32,
		offset: u32,
	) -> Result<Vec<ThreadSummary>, DbError>;

	async fn delete(&self, id: &ThreadId) -> Result<bool, DbError>;

	async fn get_thread_owner_user_id(&self, thread_id: &str) -> Result<Option<String>, DbError>;

	async fn set_owner_user_id(&self, thread_id: &str, owner_user_id: &str) -> Result<bool, DbError>;

	async fn set_shared_with_support(&self, thread_id: &str, shared: bool) -> Result<bool, DbError>;

	async fn health_check(&self) -> Result<(), DbError>;

	async fn count(&self, workspace: Option<&str>) -> Result<u64, DbError>;

	async fn search(
		&self,
		query: &str,
		workspace: Option<&str>,
		limit: u32,
		offset: u32,
	) -> Result<Vec<ThreadSearchHit>, DbError>;

	/// List threads for a specific owner with optional workspace filter.
	async fn list_for_owner(
		&self,
		owner_user_id: &str,
		workspace: Option<&str>,
		limit: u32,
		offset: u32,
	) -> Result<Vec<ThreadSummary>, DbError>;

	/// Count threads for a specific owner with optional workspace filter.
	async fn count_for_owner(
		&self,
		owner_user_id: &str,
		workspace: Option<&str>,
	) -> Result<u64, DbError>;

	/// Search threads for a specific owner.
	async fn search_for_owner(
		&self,
		owner_user_id: &str,
		query: &str,
		workspace: Option<&str>,
		limit: u32,
		offset: u32,
	) -> Result<Vec<ThreadSearchHit>, DbError>;

	async fn upsert_github_installation(
		&self,
		installation: &crate::types::GithubInstallation,
	) -> Result<(), DbError>;

	async fn delete_github_installation(&self, installation_id: i64) -> Result<bool, DbError>;

	async fn update_github_installation_suspension(
		&self,
		installation_id: i64,
		suspended_at: Option<&str>,
	) -> Result<bool, DbError>;

	async fn add_github_installation_repos(
		&self,
		installation_id: i64,
		repos: &[crate::types::GithubRepo],
	) -> Result<(), DbError>;

	async fn remove_github_installation_repos(&self, repository_ids: &[i64]) -> Result<(), DbError>;

	async fn get_github_installation_for_repo(
		&self,
		owner: &str,
		name: &str,
	) -> Result<Option<crate::types::GithubInstallationInfo>, DbError>;

	async fn list_github_installations(
		&self,
	) -> Result<Vec<crate::types::GithubInstallation>, DbError>;
}

/// Get or create a repo entry in the thread_repos table, returning its id.
async fn get_or_create_repo_id(pool: &SqlitePool, slug: &str) -> Result<Option<i64>, DbError> {
	let now = chrono::Utc::now().to_rfc3339();

	sqlx::query("INSERT OR IGNORE INTO thread_repos (slug, created_at) VALUES(?, ?)")
		.bind(slug)
		.bind(&now)
		.execute(pool)
		.await?;

	let result: Option<(i64,)> = sqlx::query_as("SELECT id FROM thread_repos WHERE slug = ?")
		.bind(slug)
		.fetch_optional(pool)
		.await?;

	Ok(result.map(|(id,)| id))
}

/// Record commit SHAs associated with a thread in the thread_commits table.
async fn record_thread_commits(
	pool: &SqlitePool,
	thread: &Thread,
	repo_id: i64,
) -> Result<(), DbError> {
	for sha in &thread.git_commits {
		let is_initial = Some(sha) == thread.git_initial_commit_sha.as_ref();
		let is_final = Some(sha) == thread.git_current_commit_sha.as_ref();

		sqlx::query(
			r#"
            INSERT OR IGNORE INTO thread_commits (
                thread_id, repo_id, commit_sha, branch, is_dirty,
                observed_at, is_initial, is_final
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?)
            "#,
		)
		.bind(thread.id.as_str())
		.bind(repo_id)
		.bind(sha)
		.bind(&thread.git_branch)
		.bind(thread.git_end_dirty.unwrap_or(false) as i32)
		.bind(&thread.updated_at)
		.bind(is_initial as i32)
		.bind(is_final as i32)
		.execute(pool)
		.await?;
	}
	Ok(())
}

/// Repository for thread database operations.
#[derive(Clone)]
pub struct ThreadRepository {
	pool: SqlitePool,
}

impl ThreadRepository {
	/// Create a new repository from an existing pool.
	pub fn new(pool: SqlitePool) -> Self {
		Self { pool }
	}

	/// Get the underlying database pool.
	pub fn pool(&self) -> &SqlitePool {
		&self.pool
	}

	/// Upsert a thread with optional version checking.
	///
	/// If `expected_version` is Some, the update will fail with Conflict if
	/// the stored version doesn't match.
	pub async fn upsert(
		&self,
		thread: &Thread,
		expected_version: Option<u64>,
	) -> Result<Thread, DbError> {
		let existing = self.get(&thread.id).await?;

		if let Some(existing_thread) = existing {
			if let Some(expected) = expected_version {
				if existing_thread.version != expected {
					return Err(DbError::Conflict(format!(
						"expected version {}, found {}",
						expected, existing_thread.version
					)));
				}
			}

			self.update(thread).await?;
		} else {
			self.insert(thread).await?;
		}

		self
			.get(&thread.id)
			.await?
			.ok_or_else(|| DbError::Internal("Thread not found after upsert".to_string()))
	}

	/// Insert a new thread.
	async fn insert(&self, thread: &Thread) -> Result<(), DbError> {
		let full_json = serde_json::to_string(thread)?;
		let tags_json = serde_json::to_string(&thread.metadata.tags)?;
		let agent_state_json = serde_json::to_string(&thread.agent_state)?;
		let conversation_json = serde_json::to_string(&thread.conversation)?;
		let metadata_json = serde_json::to_string(&thread.metadata)?;

		let repo_id = if let Some(ref slug) = thread.git_remote_url {
			get_or_create_repo_id(&self.pool, slug).await?
		} else {
			None
		};

		sqlx::query(
			r#"
            INSERT INTO threads (
                id, version, created_at, updated_at, last_activity_at,
                workspace_root, cwd, loom_version, provider, model,
                git_branch, git_remote_url, repo_id,
                git_initial_branch, git_initial_commit_sha, git_current_commit_sha,
                git_start_dirty, git_end_dirty,
                title, tags, is_pinned, message_count,
                agent_state_kind, agent_state, conversation, metadata, full_json,
                visibility, is_shared_with_support
            ) VALUES (
                ?, ?, ?, ?, ?,
                ?, ?, ?, ?, ?,
                ?, ?, ?,
                ?, ?, ?,
                ?, ?,
                ?, ?, ?, ?,
                ?, ?, ?, ?, ?,
                ?, ?
            )
            "#,
		)
		.bind(thread.id.as_str())
		.bind(thread.version as i64)
		.bind(&thread.created_at)
		.bind(&thread.updated_at)
		.bind(&thread.last_activity_at)
		.bind(&thread.workspace_root)
		.bind(&thread.cwd)
		.bind(&thread.loom_version)
		.bind(&thread.provider)
		.bind(&thread.model)
		.bind(&thread.git_branch)
		.bind(&thread.git_remote_url)
		.bind(repo_id)
		.bind(&thread.git_initial_branch)
		.bind(&thread.git_initial_commit_sha)
		.bind(&thread.git_current_commit_sha)
		.bind(thread.git_start_dirty.map(|d| d as i32))
		.bind(thread.git_end_dirty.map(|d| d as i32))
		.bind(&thread.metadata.title)
		.bind(&tags_json)
		.bind(thread.metadata.is_pinned as i32)
		.bind(thread.conversation.messages.len() as i32)
		.bind(thread.agent_state.kind.as_str())
		.bind(&agent_state_json)
		.bind(&conversation_json)
		.bind(&metadata_json)
		.bind(&full_json)
		.bind(thread.visibility.as_str())
		.bind(thread.is_shared_with_support as i32)
		.execute(&self.pool)
		.await?;

		if let Some(repo_id) = repo_id {
			record_thread_commits(&self.pool, thread, repo_id).await?;
		}

		tracing::debug!(thread_id = %thread.id, version = thread.version, "thread inserted");

		Ok(())
	}

	/// Update an existing thread.
	async fn update(&self, thread: &Thread) -> Result<(), DbError> {
		let full_json = serde_json::to_string(thread)?;
		let tags_json = serde_json::to_string(&thread.metadata.tags)?;
		let agent_state_json = serde_json::to_string(&thread.agent_state)?;
		let conversation_json = serde_json::to_string(&thread.conversation)?;
		let metadata_json = serde_json::to_string(&thread.metadata)?;

		let repo_id = if let Some(ref slug) = thread.git_remote_url {
			get_or_create_repo_id(&self.pool, slug).await?
		} else {
			None
		};

		sqlx::query(
			r#"
            UPDATE threads SET
                version = ?,
                updated_at = ?,
                last_activity_at = ?,
                workspace_root = ?,
                cwd = ?,
                loom_version = ?,
                provider = ?,
                model = ?,
                git_branch = ?,
                git_remote_url = ?,
                repo_id = ?,
                git_initial_branch = ?,
                git_initial_commit_sha = ?,
                git_current_commit_sha = ?,
                git_start_dirty = ?,
                git_end_dirty = ?,
                title = ?,
                tags = ?,
                is_pinned = ?,
                message_count = ?,
                agent_state_kind = ?,
                agent_state = ?,
                conversation = ?,
                metadata = ?,
                full_json = ?,
                visibility = ?,
                is_shared_with_support = ?
            WHERE id = ?
            "#,
		)
		.bind(thread.version as i64)
		.bind(&thread.updated_at)
		.bind(&thread.last_activity_at)
		.bind(&thread.workspace_root)
		.bind(&thread.cwd)
		.bind(&thread.loom_version)
		.bind(&thread.provider)
		.bind(&thread.model)
		.bind(&thread.git_branch)
		.bind(&thread.git_remote_url)
		.bind(repo_id)
		.bind(&thread.git_initial_branch)
		.bind(&thread.git_initial_commit_sha)
		.bind(&thread.git_current_commit_sha)
		.bind(thread.git_start_dirty.map(|d| d as i32))
		.bind(thread.git_end_dirty.map(|d| d as i32))
		.bind(&thread.metadata.title)
		.bind(&tags_json)
		.bind(thread.metadata.is_pinned as i32)
		.bind(thread.conversation.messages.len() as i32)
		.bind(thread.agent_state.kind.as_str())
		.bind(&agent_state_json)
		.bind(&conversation_json)
		.bind(&metadata_json)
		.bind(&full_json)
		.bind(thread.visibility.as_str())
		.bind(thread.is_shared_with_support as i32)
		.bind(thread.id.as_str())
		.execute(&self.pool)
		.await?;

		if let Some(repo_id) = repo_id {
			record_thread_commits(&self.pool, thread, repo_id).await?;
		}

		tracing::debug!(thread_id = %thread.id, version = thread.version, "thread updated");

		Ok(())
	}

	/// Get a thread by ID.
	pub async fn get(&self, id: &ThreadId) -> Result<Option<Thread>, DbError> {
		let row = sqlx::query(
			r#"
            SELECT full_json
            FROM threads
            WHERE id = ? AND deleted_at IS NULL
            "#,
		)
		.bind(id.as_str())
		.fetch_optional(&self.pool)
		.await?;

		match row {
			Some(row) => {
				let full_json: String = row.get("full_json");
				let thread: Thread = serde_json::from_str(&full_json)?;
				Ok(Some(thread))
			}
			None => Ok(None),
		}
	}

	/// List threads with optional workspace filter.
	pub async fn list(
		&self,
		workspace: Option<&str>,
		limit: u32,
		offset: u32,
	) -> Result<Vec<ThreadSummary>, DbError> {
		let rows = match workspace {
			Some(ws) => {
				sqlx::query(
					r#"
                    SELECT id, title, workspace_root, last_activity_at,
                           provider, model, tags, version, message_count,
                           created_at, updated_at, is_pinned, visibility,
                           git_branch, git_remote_url,
                           git_initial_commit_sha, git_current_commit_sha
                    FROM threads
                    WHERE deleted_at IS NULL AND workspace_root = ?
                    ORDER BY last_activity_at DESC
                    LIMIT ? OFFSET ?
                    "#,
				)
				.bind(ws)
				.bind(limit as i32)
				.bind(offset as i32)
				.fetch_all(&self.pool)
				.await?
			}
			None => {
				sqlx::query(
					r#"
                    SELECT id, title, workspace_root, last_activity_at,
                           provider, model, tags, version, message_count,
                           created_at, updated_at, is_pinned, visibility,
                           git_branch, git_remote_url,
                           git_initial_commit_sha, git_current_commit_sha
                    FROM threads
                    WHERE deleted_at IS NULL
                    ORDER BY last_activity_at DESC
                    LIMIT ? OFFSET ?
                    "#,
				)
				.bind(limit as i32)
				.bind(offset as i32)
				.fetch_all(&self.pool)
				.await?
			}
		};

		let summaries = rows
			.into_iter()
			.map(|row| {
				let id: String = row.get("id");
				let title: Option<String> = row.get("title");
				let workspace_root: Option<String> = row.get("workspace_root");
				let last_activity_at: String = row.get("last_activity_at");
				let provider: Option<String> = row.get("provider");
				let model: Option<String> = row.get("model");
				let tags_json: String = row.get("tags");
				let version: i64 = row.get("version");
				let message_count: i32 = row.get("message_count");
				let created_at: String = row.get("created_at");
				let updated_at: String = row.get("updated_at");
				let is_pinned: i32 = row.get("is_pinned");
				let visibility_str: String = row.get("visibility");
				let git_branch: Option<String> = row.get("git_branch");
				let git_remote_url: Option<String> = row.get("git_remote_url");
				let git_initial_commit_sha: Option<String> = row.get("git_initial_commit_sha");
				let git_current_commit_sha: Option<String> = row.get("git_current_commit_sha");

				let tags: Vec<String> = serde_json::from_str(&tags_json).unwrap_or_default();
				let visibility = visibility_str.parse().unwrap_or(ThreadVisibility::Private);

				ThreadSummary {
					id: ThreadId::from_string(id),
					version: version as u64,
					created_at,
					updated_at,
					last_activity_at,
					title,
					workspace_root,
					git_branch,
					git_remote_url,
					git_initial_commit_sha,
					git_current_commit_sha,
					provider,
					model,
					tags,
					message_count: message_count as u32,
					is_pinned: is_pinned != 0,
					visibility,
				}
			})
			.collect();

		Ok(summaries)
	}

	/// Soft-delete a thread.
	pub async fn delete(&self, id: &ThreadId) -> Result<bool, DbError> {
		let now = chrono::Utc::now().to_rfc3339();

		let result = sqlx::query(
			r#"
            UPDATE threads
            SET deleted_at = ?
            WHERE id = ? AND deleted_at IS NULL
            "#,
		)
		.bind(&now)
		.bind(id.as_str())
		.execute(&self.pool)
		.await?;

		let deleted = result.rows_affected() > 0;

		if deleted {
			tracing::debug!(thread_id = %id, "thread soft-deleted");
		}

		Ok(deleted)
	}

	/// Get the owner_user_id for a thread.
	pub async fn get_thread_owner_user_id(&self, thread_id: &str) -> Result<Option<String>, DbError> {
		let row: Option<(Option<String>,)> = sqlx::query_as(
			r#"
			SELECT owner_user_id
			FROM threads
			WHERE id = ? AND deleted_at IS NULL
			"#,
		)
		.bind(thread_id)
		.fetch_optional(&self.pool)
		.await?;

		Ok(row.and_then(|(owner,)| owner))
	}

	/// Set the owner_user_id for a thread.
	pub async fn set_owner_user_id(
		&self,
		thread_id: &str,
		owner_user_id: &str,
	) -> Result<bool, DbError> {
		let result = sqlx::query(
			r#"
			UPDATE threads
			SET owner_user_id = ?
			WHERE id = ? AND deleted_at IS NULL
			"#,
		)
		.bind(owner_user_id)
		.bind(thread_id)
		.execute(&self.pool)
		.await?;

		Ok(result.rows_affected() > 0)
	}

	/// Update the is_shared_with_support flag for a thread.
	pub async fn set_shared_with_support(
		&self,
		thread_id: &str,
		shared: bool,
	) -> Result<bool, DbError> {
		let result = sqlx::query(
			r#"
			UPDATE threads
			SET is_shared_with_support = ?
			WHERE id = ? AND deleted_at IS NULL
			"#,
		)
		.bind(shared as i32)
		.bind(thread_id)
		.execute(&self.pool)
		.await?;

		Ok(result.rows_affected() > 0)
	}

	/// Lightweight database health check (used by /health endpoint).
	pub async fn health_check(&self) -> Result<(), DbError> {
		sqlx::query("SELECT 1")
			.execute(&self.pool)
			.await
			.map(|_| ())
			.map_err(Into::into)
	}

	/// Count total threads (excluding deleted).
	pub async fn count(&self, workspace: Option<&str>) -> Result<u64, DbError> {
		let count: (i64,) = match workspace {
			Some(ws) => {
				sqlx::query_as(
					r#"
                    SELECT COUNT(*) FROM threads
                    WHERE deleted_at IS NULL AND workspace_root = ?
                    "#,
				)
				.bind(ws)
				.fetch_one(&self.pool)
				.await?
			}
			None => {
				sqlx::query_as(
					r#"
                    SELECT COUNT(*) FROM threads
                    WHERE deleted_at IS NULL
                    "#,
				)
				.fetch_one(&self.pool)
				.await?
			}
		};

		Ok(count.0 as u64)
	}

	/// List threads for a specific owner with optional workspace filter.
	pub async fn list_for_owner(
		&self,
		owner_user_id: &str,
		workspace: Option<&str>,
		limit: u32,
		offset: u32,
	) -> Result<Vec<ThreadSummary>, DbError> {
		let rows = match workspace {
			Some(ws) => {
				sqlx::query(
					r#"
                    SELECT id, title, workspace_root, last_activity_at,
                           provider, model, tags, version, message_count,
                           created_at, updated_at, is_pinned, visibility,
                           git_branch, git_remote_url,
                           git_initial_commit_sha, git_current_commit_sha
                    FROM threads
                    WHERE deleted_at IS NULL AND owner_user_id = ? AND workspace_root = ?
                    ORDER BY last_activity_at DESC
                    LIMIT ? OFFSET ?
                    "#,
				)
				.bind(owner_user_id)
				.bind(ws)
				.bind(limit as i32)
				.bind(offset as i32)
				.fetch_all(&self.pool)
				.await?
			}
			None => {
				sqlx::query(
					r#"
                    SELECT id, title, workspace_root, last_activity_at,
                           provider, model, tags, version, message_count,
                           created_at, updated_at, is_pinned, visibility,
                           git_branch, git_remote_url,
                           git_initial_commit_sha, git_current_commit_sha
                    FROM threads
                    WHERE deleted_at IS NULL AND owner_user_id = ?
                    ORDER BY last_activity_at DESC
                    LIMIT ? OFFSET ?
                    "#,
				)
				.bind(owner_user_id)
				.bind(limit as i32)
				.bind(offset as i32)
				.fetch_all(&self.pool)
				.await?
			}
		};

		let summaries = rows
			.into_iter()
			.map(|row| {
				let id: String = row.get("id");
				let title: Option<String> = row.get("title");
				let workspace_root: Option<String> = row.get("workspace_root");
				let last_activity_at: String = row.get("last_activity_at");
				let provider: Option<String> = row.get("provider");
				let model: Option<String> = row.get("model");
				let tags_json: String = row.get("tags");
				let version: i64 = row.get("version");
				let message_count: i32 = row.get("message_count");
				let created_at: String = row.get("created_at");
				let updated_at: String = row.get("updated_at");
				let is_pinned: i32 = row.get("is_pinned");
				let visibility_str: String = row.get("visibility");
				let git_branch: Option<String> = row.get("git_branch");
				let git_remote_url: Option<String> = row.get("git_remote_url");
				let git_initial_commit_sha: Option<String> = row.get("git_initial_commit_sha");
				let git_current_commit_sha: Option<String> = row.get("git_current_commit_sha");

				let tags: Vec<String> = serde_json::from_str(&tags_json).unwrap_or_default();
				let visibility = visibility_str.parse().unwrap_or(ThreadVisibility::Private);

				ThreadSummary {
					id: ThreadId::from_string(id),
					version: version as u64,
					created_at,
					updated_at,
					last_activity_at,
					title,
					workspace_root,
					git_branch,
					git_remote_url,
					git_initial_commit_sha,
					git_current_commit_sha,
					provider,
					model,
					tags,
					message_count: message_count as u32,
					is_pinned: is_pinned != 0,
					visibility,
				}
			})
			.collect();

		Ok(summaries)
	}

	/// Count threads for a specific owner with optional workspace filter.
	pub async fn count_for_owner(
		&self,
		owner_user_id: &str,
		workspace: Option<&str>,
	) -> Result<u64, DbError> {
		let count: (i64,) = match workspace {
			Some(ws) => {
				sqlx::query_as(
					r#"
                    SELECT COUNT(*) FROM threads
                    WHERE deleted_at IS NULL AND owner_user_id = ? AND workspace_root = ?
                    "#,
				)
				.bind(owner_user_id)
				.bind(ws)
				.fetch_one(&self.pool)
				.await?
			}
			None => {
				sqlx::query_as(
					r#"
                    SELECT COUNT(*) FROM threads
                    WHERE deleted_at IS NULL AND owner_user_id = ?
                    "#,
				)
				.bind(owner_user_id)
				.fetch_one(&self.pool)
				.await?
			}
		};

		Ok(count.0 as u64)
	}

	/// Search threads for a specific owner.
	pub async fn search_for_owner(
		&self,
		owner_user_id: &str,
		query: &str,
		workspace: Option<&str>,
		limit: u32,
		offset: u32,
	) -> Result<Vec<ThreadSearchHit>, DbError> {
		let query = query.trim();

		let is_sha_like =
			query.len() >= 7 && query.len() <= 40 && query.chars().all(|c| c.is_ascii_hexdigit());

		if is_sha_like {
			self
				.search_by_commit_prefix_for_owner(owner_user_id, query, workspace, limit, offset)
				.await
		} else {
			self
				.search_fts_for_owner(owner_user_id, query, workspace, limit, offset)
				.await
		}
	}

	async fn search_by_commit_prefix_for_owner(
		&self,
		owner_user_id: &str,
		prefix: &str,
		workspace: Option<&str>,
		limit: u32,
		offset: u32,
	) -> Result<Vec<ThreadSearchHit>, DbError> {
		let like_pattern = format!("{prefix}%");

		let sql = if workspace.is_some() {
			r#"
            SELECT t.id, t.title, t.workspace_root, t.last_activity_at,
                   t.provider, t.model, t.tags, t.version, t.message_count,
                   t.created_at, t.updated_at, t.is_pinned, t.visibility,
                   t.git_branch, t.git_remote_url,
                   t.git_initial_commit_sha, t.git_current_commit_sha
            FROM threads t
            WHERE t.deleted_at IS NULL
              AND t.owner_user_id = ?
              AND t.workspace_root = ?
              AND (t.git_current_commit_sha LIKE ? OR t.git_initial_commit_sha LIKE ?)
            ORDER BY t.last_activity_at DESC
            LIMIT ? OFFSET ?
            "#
		} else {
			r#"
            SELECT t.id, t.title, t.workspace_root, t.last_activity_at,
                   t.provider, t.model, t.tags, t.version, t.message_count,
                   t.created_at, t.updated_at, t.is_pinned, t.visibility,
                   t.git_branch, t.git_remote_url,
                   t.git_initial_commit_sha, t.git_current_commit_sha
            FROM threads t
            WHERE t.deleted_at IS NULL
              AND t.owner_user_id = ?
              AND (t.git_current_commit_sha LIKE ? OR t.git_initial_commit_sha LIKE ?)
            ORDER BY t.last_activity_at DESC
            LIMIT ? OFFSET ?
            "#
		};

		let rows = if let Some(ws) = workspace {
			sqlx::query(sql)
				.bind(owner_user_id)
				.bind(ws)
				.bind(&like_pattern)
				.bind(&like_pattern)
				.bind(limit as i32)
				.bind(offset as i32)
				.fetch_all(&self.pool)
				.await?
		} else {
			sqlx::query(sql)
				.bind(owner_user_id)
				.bind(&like_pattern)
				.bind(&like_pattern)
				.bind(limit as i32)
				.bind(offset as i32)
				.fetch_all(&self.pool)
				.await?
		};

		let mut hits = Vec::new();
		for row in rows {
			let summary = self.row_to_summary(&row)?;
			hits.push(ThreadSearchHit {
				summary,
				score: 1.0,
			});
		}
		Ok(hits)
	}

	async fn search_fts_for_owner(
		&self,
		owner_user_id: &str,
		query: &str,
		workspace: Option<&str>,
		limit: u32,
		offset: u32,
	) -> Result<Vec<ThreadSearchHit>, DbError> {
		let fts_query = format!("\"{}\"", query.replace('"', " "));

		let sql = if workspace.is_some() {
			r#"
            SELECT t.id, t.title, t.workspace_root, t.last_activity_at,
                   t.provider, t.model, t.tags, t.version, t.message_count,
                   t.created_at, t.updated_at, t.is_pinned, t.visibility,
                   t.git_branch, t.git_remote_url,
                   t.git_initial_commit_sha, t.git_current_commit_sha,
                   bm25(thread_fts) as score
            FROM threads t
            JOIN thread_fts ON thread_fts.rowid = (
                SELECT rowid FROM threads WHERE id = t.id
            )
            WHERE thread_fts MATCH ?
              AND t.deleted_at IS NULL
              AND t.owner_user_id = ?
              AND t.workspace_root = ?
            ORDER BY score
            LIMIT ? OFFSET ?
            "#
		} else {
			r#"
            SELECT t.id, t.title, t.workspace_root, t.last_activity_at,
                   t.provider, t.model, t.tags, t.version, t.message_count,
                   t.created_at, t.updated_at, t.is_pinned, t.visibility,
                   t.git_branch, t.git_remote_url,
                   t.git_initial_commit_sha, t.git_current_commit_sha,
                   bm25(thread_fts) as score
            FROM threads t
            JOIN thread_fts ON thread_fts.rowid = (
                SELECT rowid FROM threads WHERE id = t.id
            )
            WHERE thread_fts MATCH ?
              AND t.deleted_at IS NULL
              AND t.owner_user_id = ?
            ORDER BY score
            LIMIT ? OFFSET ?
            "#
		};

		let rows = if let Some(ws) = workspace {
			sqlx::query(sql)
				.bind(&fts_query)
				.bind(owner_user_id)
				.bind(ws)
				.bind(limit as i32)
				.bind(offset as i32)
				.fetch_all(&self.pool)
				.await?
		} else {
			sqlx::query(sql)
				.bind(&fts_query)
				.bind(owner_user_id)
				.bind(limit as i32)
				.bind(offset as i32)
				.fetch_all(&self.pool)
				.await?
		};

		let mut hits = Vec::new();
		for row in rows {
			let summary = self.row_to_summary(&row)?;
			let score: f64 = row.try_get("score").unwrap_or(0.0);
			hits.push(ThreadSearchHit {
				summary,
				score: -score,
			});
		}
		Ok(hits)
	}

	/// Search threads by query string.
	///
	/// Detects SHA-like queries and searches commit prefixes first,
	/// otherwise falls back to FTS5 full-text search.
	pub async fn search(
		&self,
		query: &str,
		workspace: Option<&str>,
		limit: u32,
		offset: u32,
	) -> Result<Vec<ThreadSearchHit>, DbError> {
		let query = query.trim();

		let is_sha_like = query.len() >= 7
			&& query.len() <= 40
			&& !query.contains(char::is_whitespace)
			&& query.chars().all(|c| c.is_ascii_hexdigit());

		if is_sha_like {
			let hits = self
				.search_by_commit_prefix(query, workspace, limit, offset)
				.await?;
			if !hits.is_empty() {
				return Ok(hits);
			}
		}

		self.search_fts(query, workspace, limit, offset).await
	}

	async fn search_by_commit_prefix(
		&self,
		prefix: &str,
		workspace: Option<&str>,
		limit: u32,
		offset: u32,
	) -> Result<Vec<ThreadSearchHit>, DbError> {
		let like_pattern = format!("{prefix}%");

		let sql = if workspace.is_some() {
			r#"
            SELECT DISTINCT
                t.id, t.version, t.created_at, t.updated_at, t.last_activity_at,
                t.title, t.workspace_root, t.git_branch, t.git_remote_url,
                t.git_initial_commit_sha, t.git_current_commit_sha,
                t.provider, t.model, t.tags, t.message_count, t.is_pinned, t.visibility
            FROM thread_commits c
            JOIN threads t ON t.id = c.thread_id
            WHERE c.commit_sha LIKE ?1
              AND t.deleted_at IS NULL
              AND t.workspace_root = ?2
            ORDER BY t.last_activity_at DESC
            LIMIT ?3 OFFSET ?4
            "#
		} else {
			r#"
            SELECT DISTINCT
                t.id, t.version, t.created_at, t.updated_at, t.last_activity_at,
                t.title, t.workspace_root, t.git_branch, t.git_remote_url,
                t.git_initial_commit_sha, t.git_current_commit_sha,
                t.provider, t.model, t.tags, t.message_count, t.is_pinned, t.visibility
            FROM thread_commits c
            JOIN threads t ON t.id = c.thread_id
            WHERE c.commit_sha LIKE ?1
              AND t.deleted_at IS NULL
            ORDER BY t.last_activity_at DESC
            LIMIT ?2 OFFSET ?3
            "#
		};

		let rows = if let Some(ws) = workspace {
			sqlx::query(sql)
				.bind(&like_pattern)
				.bind(ws)
				.bind(limit as i32)
				.bind(offset as i32)
				.fetch_all(&self.pool)
				.await?
		} else {
			sqlx::query(sql)
				.bind(&like_pattern)
				.bind(limit as i32)
				.bind(offset as i32)
				.fetch_all(&self.pool)
				.await?
		};

		self.rows_to_search_hits(rows, 0.0)
	}

	async fn search_fts(
		&self,
		query: &str,
		workspace: Option<&str>,
		limit: u32,
		offset: u32,
	) -> Result<Vec<ThreadSearchHit>, DbError> {
		let fts_query = format!("\"{}\"", query.replace('"', " "));

		let sql = if workspace.is_some() {
			r#"
            SELECT
                t.id, t.version, t.created_at, t.updated_at, t.last_activity_at,
                t.title, t.workspace_root, t.git_branch, t.git_remote_url,
                t.git_initial_commit_sha, t.git_current_commit_sha,
                t.provider, t.model, t.tags, t.message_count, t.is_pinned, t.visibility,
                bm25(thread_fts) AS score
            FROM thread_fts
            JOIN threads t ON t.id = thread_fts.thread_id
            WHERE thread_fts MATCH ?1
              AND t.deleted_at IS NULL
              AND t.workspace_root = ?2
            ORDER BY score ASC, t.last_activity_at DESC
            LIMIT ?3 OFFSET ?4
            "#
		} else {
			r#"
            SELECT
                t.id, t.version, t.created_at, t.updated_at, t.last_activity_at,
                t.title, t.workspace_root, t.git_branch, t.git_remote_url,
                t.git_initial_commit_sha, t.git_current_commit_sha,
                t.provider, t.model, t.tags, t.message_count, t.is_pinned, t.visibility,
                bm25(thread_fts) AS score
            FROM thread_fts
            JOIN threads t ON t.id = thread_fts.thread_id
            WHERE thread_fts MATCH ?1
              AND t.deleted_at IS NULL
            ORDER BY score ASC, t.last_activity_at DESC
            LIMIT ?2 OFFSET ?3
            "#
		};

		let rows = if let Some(ws) = workspace {
			sqlx::query(sql)
				.bind(&fts_query)
				.bind(ws)
				.bind(limit as i32)
				.bind(offset as i32)
				.fetch_all(&self.pool)
				.await?
		} else {
			sqlx::query(sql)
				.bind(&fts_query)
				.bind(limit as i32)
				.bind(offset as i32)
				.fetch_all(&self.pool)
				.await?
		};

		let mut hits = Vec::new();
		for row in rows {
			let summary = self.row_to_summary(&row)?;
			let score: f64 = row.try_get("score").unwrap_or(0.0);
			hits.push(ThreadSearchHit { summary, score });
		}
		Ok(hits)
	}

	fn rows_to_search_hits(
		&self,
		rows: Vec<sqlx::sqlite::SqliteRow>,
		default_score: f64,
	) -> Result<Vec<ThreadSearchHit>, DbError> {
		let mut hits = Vec::new();
		for row in rows {
			let summary = self.row_to_summary(&row)?;
			hits.push(ThreadSearchHit {
				summary,
				score: default_score,
			});
		}
		Ok(hits)
	}

	fn row_to_summary(&self, row: &sqlx::sqlite::SqliteRow) -> Result<ThreadSummary, DbError> {
		let id: String = row.get("id");
		let title: Option<String> = row.get("title");
		let workspace_root: Option<String> = row.get("workspace_root");
		let last_activity_at: String = row.get("last_activity_at");
		let provider: Option<String> = row.get("provider");
		let model: Option<String> = row.get("model");
		let tags_json: String = row.get("tags");
		let version: i64 = row.get("version");
		let message_count: i32 = row.get("message_count");
		let created_at: String = row.get("created_at");
		let updated_at: String = row.get("updated_at");
		let is_pinned: i32 = row.get("is_pinned");
		let visibility_str: String = row.get("visibility");
		let git_branch: Option<String> = row.get("git_branch");
		let git_remote_url: Option<String> = row.get("git_remote_url");
		let git_initial_commit_sha: Option<String> = row.get("git_initial_commit_sha");
		let git_current_commit_sha: Option<String> = row.get("git_current_commit_sha");

		let tags: Vec<String> = serde_json::from_str(&tags_json).unwrap_or_default();
		let visibility = visibility_str.parse().unwrap_or(ThreadVisibility::Private);

		Ok(ThreadSummary {
			id: ThreadId::from_string(id),
			version: version as u64,
			created_at,
			updated_at,
			last_activity_at,
			title,
			workspace_root,
			git_branch,
			git_remote_url,
			git_initial_commit_sha,
			git_current_commit_sha,
			provider,
			model,
			tags,
			message_count: message_count as u32,
			is_pinned: is_pinned != 0,
			visibility,
		})
	}

	/// Normalizes a query string for cache key purposes.
	pub fn normalize_cache_query(query: &str) -> String {
		query
			.split_whitespace()
			.collect::<Vec<_>>()
			.join(" ")
			.to_lowercase()
	}

	// ========== GitHub App Methods ==========

	/// Upsert a GitHub installation from webhook data.
	pub async fn upsert_github_installation(
		&self,
		installation: &crate::types::GithubInstallation,
	) -> Result<(), DbError> {
		let now = chrono::Utc::now().to_rfc3339();

		sqlx::query(
			r#"
            INSERT INTO github_installations (
                installation_id, account_id, account_login, account_type,
                app_slug, repositories_selection, suspended_at, created_at, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
            ON CONFLICT(installation_id) DO UPDATE SET
                account_id = excluded.account_id,
                account_login = excluded.account_login,
                account_type = excluded.account_type,
                app_slug = excluded.app_slug,
                repositories_selection = excluded.repositories_selection,
                suspended_at = excluded.suspended_at,
                updated_at = ?9
            "#,
		)
		.bind(installation.installation_id)
		.bind(installation.account_id)
		.bind(&installation.account_login)
		.bind(&installation.account_type)
		.bind(&installation.app_slug)
		.bind(&installation.repositories_selection)
		.bind(&installation.suspended_at)
		.bind(&installation.created_at)
		.bind(&now)
		.execute(&self.pool)
		.await?;

		tracing::info!(
			installation_id = installation.installation_id,
			account_login = %installation.account_login,
			"github_installation: upserted"
		);

		Ok(())
	}

	/// Delete a GitHub installation (cascades to repos).
	pub async fn delete_github_installation(&self, installation_id: i64) -> Result<bool, DbError> {
		let result = sqlx::query("DELETE FROM github_installations WHERE installation_id = ?")
			.bind(installation_id)
			.execute(&self.pool)
			.await?;

		let deleted = result.rows_affected() > 0;

		if deleted {
			tracing::info!(
				installation_id = installation_id,
				"github_installation: deleted"
			);
		}

		Ok(deleted)
	}

	/// Suspend or unsuspend an installation.
	pub async fn update_github_installation_suspension(
		&self,
		installation_id: i64,
		suspended_at: Option<&str>,
	) -> Result<bool, DbError> {
		let now = chrono::Utc::now().to_rfc3339();

		let result = sqlx::query(
			r#"
            UPDATE github_installations
            SET suspended_at = ?1, updated_at = ?2
            WHERE installation_id = ?3
            "#,
		)
		.bind(suspended_at)
		.bind(&now)
		.bind(installation_id)
		.execute(&self.pool)
		.await?;

		let updated = result.rows_affected() > 0;

		if updated {
			tracing::info!(
				installation_id = installation_id,
				suspended = suspended_at.is_some(),
				"github_installation: suspension updated"
			);
		}

		Ok(updated)
	}

	/// Add repositories to an installation.
	pub async fn add_github_installation_repos(
		&self,
		installation_id: i64,
		repos: &[crate::types::GithubRepo],
	) -> Result<(), DbError> {
		let now = chrono::Utc::now().to_rfc3339();

		for repo in repos {
			sqlx::query(
				r#"
                INSERT INTO github_installation_repos (
                    repository_id, installation_id, owner, name, full_name,
                    private, default_branch, created_at, updated_at
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
                ON CONFLICT(repository_id) DO UPDATE SET
                    installation_id = excluded.installation_id,
                    owner = excluded.owner,
                    name = excluded.name,
                    full_name = excluded.full_name,
                    private = excluded.private,
                    default_branch = excluded.default_branch,
                    updated_at = ?9
                "#,
			)
			.bind(repo.repository_id)
			.bind(installation_id)
			.bind(&repo.owner)
			.bind(&repo.name)
			.bind(&repo.full_name)
			.bind(repo.private as i32)
			.bind(&repo.default_branch)
			.bind(&now)
			.bind(&now)
			.execute(&self.pool)
			.await?;
		}

		tracing::info!(
			installation_id = installation_id,
			repo_count = repos.len(),
			"github_installation_repos: added"
		);

		Ok(())
	}

	/// Remove repositories from installations by repository ID.
	pub async fn remove_github_installation_repos(
		&self,
		repository_ids: &[i64],
	) -> Result<(), DbError> {
		for repo_id in repository_ids {
			sqlx::query("DELETE FROM github_installation_repos WHERE repository_id = ?")
				.bind(repo_id)
				.execute(&self.pool)
				.await?;
		}

		tracing::info!(
			repo_count = repository_ids.len(),
			"github_installation_repos: removed"
		);

		Ok(())
	}

	/// Get installation ID for a repository by owner/name.
	pub async fn get_github_installation_for_repo(
		&self,
		owner: &str,
		name: &str,
	) -> Result<Option<crate::types::GithubInstallationInfo>, DbError> {
		let row: Option<(i64, String, String, String)> = sqlx::query_as(
			r#"
            SELECT
                gi.installation_id,
                gi.account_login,
                gi.account_type,
                gi.repositories_selection
            FROM github_installation_repos gir
            JOIN github_installations gi ON gi.installation_id = gir.installation_id
            WHERE gir.owner = ?1 AND gir.name = ?2
              AND gi.suspended_at IS NULL
            "#,
		)
		.bind(owner)
		.bind(name)
		.fetch_optional(&self.pool)
		.await?;

		match row {
			Some((installation_id, account_login, account_type, repositories_selection)) => {
				tracing::debug!(
					owner = %owner,
					name = %name,
					installation_id = installation_id,
					"github_installation_for_repo: found"
				);
				Ok(Some(crate::types::GithubInstallationInfo {
					installation_id,
					account_login,
					account_type,
					repositories_selection,
				}))
			}
			None => {
				tracing::debug!(
					owner = %owner,
					name = %name,
					"github_installation_for_repo: not found"
				);
				Ok(None)
			}
		}
	}

	/// List all installations.
	#[allow(clippy::type_complexity)]
	pub async fn list_github_installations(
		&self,
	) -> Result<Vec<crate::types::GithubInstallation>, DbError> {
		let rows: Vec<(
			i64,
			i64,
			String,
			String,
			Option<String>,
			String,
			Option<String>,
			String,
			String,
		)> = sqlx::query_as(
			r#"
                SELECT
                    installation_id, account_id, account_login, account_type,
                    app_slug, repositories_selection, suspended_at, created_at, updated_at
                FROM github_installations
                ORDER BY account_login
                "#,
		)
		.fetch_all(&self.pool)
		.await?;

		let installations = rows
			.into_iter()
			.map(
				|(
					installation_id,
					account_id,
					account_login,
					account_type,
					app_slug,
					repositories_selection,
					suspended_at,
					created_at,
					updated_at,
				)| crate::types::GithubInstallation {
					installation_id,
					account_id,
					account_login,
					account_type,
					app_slug,
					repositories_selection,
					suspended_at,
					created_at,
					updated_at,
				},
			)
			.collect();

		Ok(installations)
	}
}

#[async_trait]
impl ThreadStore for ThreadRepository {
	async fn upsert(
		&self,
		thread: &Thread,
		expected_version: Option<u64>,
	) -> Result<Thread, DbError> {
		ThreadRepository::upsert(self, thread, expected_version).await
	}

	async fn get(&self, id: &ThreadId) -> Result<Option<Thread>, DbError> {
		ThreadRepository::get(self, id).await
	}

	async fn list(
		&self,
		workspace: Option<&str>,
		limit: u32,
		offset: u32,
	) -> Result<Vec<ThreadSummary>, DbError> {
		ThreadRepository::list(self, workspace, limit, offset).await
	}

	async fn delete(&self, id: &ThreadId) -> Result<bool, DbError> {
		ThreadRepository::delete(self, id).await
	}

	async fn get_thread_owner_user_id(&self, thread_id: &str) -> Result<Option<String>, DbError> {
		ThreadRepository::get_thread_owner_user_id(self, thread_id).await
	}

	async fn set_owner_user_id(&self, thread_id: &str, owner_user_id: &str) -> Result<bool, DbError> {
		ThreadRepository::set_owner_user_id(self, thread_id, owner_user_id).await
	}

	async fn set_shared_with_support(&self, thread_id: &str, shared: bool) -> Result<bool, DbError> {
		ThreadRepository::set_shared_with_support(self, thread_id, shared).await
	}

	async fn health_check(&self) -> Result<(), DbError> {
		ThreadRepository::health_check(self).await
	}

	async fn count(&self, workspace: Option<&str>) -> Result<u64, DbError> {
		ThreadRepository::count(self, workspace).await
	}

	async fn search(
		&self,
		query: &str,
		workspace: Option<&str>,
		limit: u32,
		offset: u32,
	) -> Result<Vec<ThreadSearchHit>, DbError> {
		ThreadRepository::search(self, query, workspace, limit, offset).await
	}

	async fn list_for_owner(
		&self,
		owner_user_id: &str,
		workspace: Option<&str>,
		limit: u32,
		offset: u32,
	) -> Result<Vec<ThreadSummary>, DbError> {
		ThreadRepository::list_for_owner(self, owner_user_id, workspace, limit, offset).await
	}

	async fn count_for_owner(
		&self,
		owner_user_id: &str,
		workspace: Option<&str>,
	) -> Result<u64, DbError> {
		ThreadRepository::count_for_owner(self, owner_user_id, workspace).await
	}

	async fn search_for_owner(
		&self,
		owner_user_id: &str,
		query: &str,
		workspace: Option<&str>,
		limit: u32,
		offset: u32,
	) -> Result<Vec<ThreadSearchHit>, DbError> {
		ThreadRepository::search_for_owner(self, owner_user_id, query, workspace, limit, offset).await
	}

	async fn upsert_github_installation(
		&self,
		installation: &crate::types::GithubInstallation,
	) -> Result<(), DbError> {
		ThreadRepository::upsert_github_installation(self, installation).await
	}

	async fn delete_github_installation(&self, installation_id: i64) -> Result<bool, DbError> {
		ThreadRepository::delete_github_installation(self, installation_id).await
	}

	async fn update_github_installation_suspension(
		&self,
		installation_id: i64,
		suspended_at: Option<&str>,
	) -> Result<bool, DbError> {
		ThreadRepository::update_github_installation_suspension(self, installation_id, suspended_at)
			.await
	}

	async fn add_github_installation_repos(
		&self,
		installation_id: i64,
		repos: &[crate::types::GithubRepo],
	) -> Result<(), DbError> {
		ThreadRepository::add_github_installation_repos(self, installation_id, repos).await
	}

	async fn remove_github_installation_repos(&self, repository_ids: &[i64]) -> Result<(), DbError> {
		ThreadRepository::remove_github_installation_repos(self, repository_ids).await
	}

	async fn get_github_installation_for_repo(
		&self,
		owner: &str,
		name: &str,
	) -> Result<Option<crate::types::GithubInstallationInfo>, DbError> {
		ThreadRepository::get_github_installation_for_repo(self, owner, name).await
	}

	async fn list_github_installations(
		&self,
	) -> Result<Vec<crate::types::GithubInstallation>, DbError> {
		ThreadRepository::list_github_installations(self).await
	}
}

trait AgentStateKindExt {
	fn as_str(&self) -> &'static str;
}

impl AgentStateKindExt for loom_common_thread::AgentStateKind {
	fn as_str(&self) -> &'static str {
		match self {
			loom_common_thread::AgentStateKind::WaitingForUserInput => "waiting_for_user_input",
			loom_common_thread::AgentStateKind::CallingLlm => "calling_llm",
			loom_common_thread::AgentStateKind::ProcessingLlmResponse => "processing_llm_response",
			loom_common_thread::AgentStateKind::ExecutingTools => "executing_tools",
			loom_common_thread::AgentStateKind::PostToolsHook => "post_tools_hook",
			loom_common_thread::AgentStateKind::Error => "error",
			loom_common_thread::AgentStateKind::ShuttingDown => "shutting_down",
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use loom_common_thread::Thread;

	#[test]
	fn test_normalize_cache_query() {
		assert_eq!(
			ThreadRepository::normalize_cache_query("Hello World"),
			"hello world"
		);
		assert_eq!(
			ThreadRepository::normalize_cache_query("  multiple   spaces  "),
			"multiple spaces"
		);
		assert_eq!(
			ThreadRepository::normalize_cache_query("UPPERCASE"),
			"uppercase"
		);
		assert_eq!(
			ThreadRepository::normalize_cache_query("  trim  me  "),
			"trim me"
		);
		assert_eq!(
			ThreadRepository::normalize_cache_query("already normalized"),
			"already normalized"
		);
	}

	async fn create_thread_test_pool() -> SqlitePool {
		let pool = SqlitePool::connect(":memory:").await.unwrap();
		sqlx::query(
			r#"
			CREATE TABLE IF NOT EXISTS threads (
				id TEXT PRIMARY KEY NOT NULL,
				version INTEGER NOT NULL DEFAULT 1,
				created_at TEXT NOT NULL,
				updated_at TEXT NOT NULL,
				last_activity_at TEXT NOT NULL,
				deleted_at TEXT,
				workspace_root TEXT,
				cwd TEXT,
				loom_version TEXT,
				provider TEXT,
				model TEXT,
				git_branch TEXT,
				git_remote_url TEXT,
				repo_id INTEGER,
				git_initial_branch TEXT,
				git_initial_commit_sha TEXT,
				git_current_commit_sha TEXT,
				git_start_dirty INTEGER,
				git_end_dirty INTEGER,
				title TEXT,
				tags TEXT,
				is_pinned INTEGER NOT NULL DEFAULT 0,
				message_count INTEGER NOT NULL DEFAULT 0,
				agent_state_kind TEXT NOT NULL,
				agent_state JSON NOT NULL,
				conversation JSON NOT NULL,
				metadata JSON NOT NULL,
				full_json JSON NOT NULL,
				visibility TEXT DEFAULT 'private',
				is_shared_with_support INTEGER DEFAULT 0,
				owner_user_id TEXT
			)
			"#,
		)
		.execute(&pool)
		.await
		.unwrap();

		sqlx::query(
			r#"
			CREATE TABLE IF NOT EXISTS thread_repos (
				id INTEGER PRIMARY KEY AUTOINCREMENT,
				slug TEXT NOT NULL UNIQUE,
				created_at TEXT NOT NULL
			)
			"#,
		)
		.execute(&pool)
		.await
		.unwrap();

		sqlx::query(
			r#"
			CREATE TABLE IF NOT EXISTS thread_commits (
				thread_id TEXT NOT NULL,
				repo_id INTEGER NOT NULL,
				commit_sha TEXT NOT NULL,
				branch TEXT,
				is_dirty INTEGER,
				observed_at TEXT,
				is_initial INTEGER,
				is_final INTEGER,
				PRIMARY KEY (thread_id, commit_sha)
			)
			"#,
		)
		.execute(&pool)
		.await
		.unwrap();

		pool
	}

	#[tokio::test]
	async fn test_upsert_and_get_thread() {
		let pool = create_thread_test_pool().await;
		let repo = ThreadRepository::new(pool);

		let mut thread = Thread::new();
		thread.metadata.title = Some("Test Thread".to_string());
		thread.workspace_root = Some("/home/user/project".to_string());

		let result = repo.upsert(&thread, None).await.unwrap();
		assert_eq!(result.id, thread.id);
		assert_eq!(result.metadata.title, Some("Test Thread".to_string()));

		let fetched = repo.get(&thread.id).await.unwrap();
		assert!(fetched.is_some());
		let fetched = fetched.unwrap();
		assert_eq!(fetched.id, thread.id);
		assert_eq!(fetched.metadata.title, Some("Test Thread".to_string()));
		assert_eq!(
			fetched.workspace_root,
			Some("/home/user/project".to_string())
		);
	}

	#[tokio::test]
	async fn test_get_thread_not_found() {
		let pool = create_thread_test_pool().await;
		let repo = ThreadRepository::new(pool);

		let non_existent_id = ThreadId::new();
		let result = repo.get(&non_existent_id).await.unwrap();
		assert!(result.is_none());
	}

	#[tokio::test]
	async fn test_list_threads_for_user() {
		let pool = create_thread_test_pool().await;
		let repo = ThreadRepository::new(pool);

		let mut thread1 = Thread::new();
		thread1.metadata.title = Some("Thread 1".to_string());
		thread1.workspace_root = Some("/home/user/project".to_string());

		let mut thread2 = Thread::new();
		thread2.metadata.title = Some("Thread 2".to_string());
		thread2.workspace_root = Some("/home/user/project".to_string());

		let mut thread3 = Thread::new();
		thread3.metadata.title = Some("Thread 3".to_string());
		thread3.workspace_root = Some("/home/user/other".to_string());

		repo.upsert(&thread1, None).await.unwrap();
		repo.upsert(&thread2, None).await.unwrap();
		repo.upsert(&thread3, None).await.unwrap();

		let all_threads = repo.list(None, 100, 0).await.unwrap();
		assert_eq!(all_threads.len(), 3);

		let workspace_threads = repo.list(Some("/home/user/project"), 100, 0).await.unwrap();
		assert_eq!(workspace_threads.len(), 2);
	}

	#[tokio::test]
	async fn test_delete_thread() {
		let pool = create_thread_test_pool().await;
		let repo = ThreadRepository::new(pool);

		let thread = Thread::new();
		repo.upsert(&thread, None).await.unwrap();

		let fetched = repo.get(&thread.id).await.unwrap();
		assert!(fetched.is_some());

		let deleted = repo.delete(&thread.id).await.unwrap();
		assert!(deleted);

		let fetched_after_delete = repo.get(&thread.id).await.unwrap();
		assert!(fetched_after_delete.is_none());

		let deleted_again = repo.delete(&thread.id).await.unwrap();
		assert!(!deleted_again);
	}
}
