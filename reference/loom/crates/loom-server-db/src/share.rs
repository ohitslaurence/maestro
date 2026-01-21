// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights
// reserved. SPDX-License-Identifier: Proprietary

//! Share link and support access repository for database operations.
//!
//! This module provides database access for:
//! - Share links (read-only external thread access)
//! - Support access (temporary debug access for support staff)

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use loom_server_auth::{ShareLink, SupportAccess, UserId};
use sqlx::{sqlite::SqlitePool, Row};
use uuid::Uuid;

use crate::error::DbError;

#[async_trait]
pub trait ShareStore: Send + Sync {
	async fn create_share_link(&self, share_link: &ShareLink) -> Result<(), DbError>;
	async fn get_share_link_by_thread(&self, thread_id: &str) -> Result<Option<ShareLink>, DbError>;
	async fn get_share_link_by_hash(&self, token_hash: &str) -> Result<Option<ShareLink>, DbError>;
	async fn revoke_share_link(&self, thread_id: &str) -> Result<i64, DbError>;
	async fn create_support_access(&self, support_access: &SupportAccess) -> Result<(), DbError>;
	async fn get_pending_support_access(
		&self,
		thread_id: &str,
	) -> Result<Option<SupportAccess>, DbError>;
	async fn get_active_support_access(
		&self,
		thread_id: &str,
		user_id: &UserId,
	) -> Result<Option<SupportAccess>, DbError>;
	async fn get_any_active_support_access(
		&self,
		thread_id: &str,
	) -> Result<Option<SupportAccess>, DbError>;
	async fn approve_support_access(
		&self,
		id: &Uuid,
		approved_by: &UserId,
		expires_at: DateTime<Utc>,
	) -> Result<bool, DbError>;
	async fn revoke_support_access(&self, id: &Uuid) -> Result<bool, DbError>;
}

#[async_trait]
impl ShareStore for ShareRepository {
	async fn create_share_link(&self, share_link: &ShareLink) -> Result<(), DbError> {
		self.create_share_link(share_link).await
	}

	async fn get_share_link_by_thread(&self, thread_id: &str) -> Result<Option<ShareLink>, DbError> {
		self.get_share_link_by_thread(thread_id).await
	}

	async fn get_share_link_by_hash(&self, token_hash: &str) -> Result<Option<ShareLink>, DbError> {
		self.get_share_link_by_hash(token_hash).await
	}

	async fn revoke_share_link(&self, thread_id: &str) -> Result<i64, DbError> {
		self.revoke_share_link(thread_id).await
	}

	async fn create_support_access(&self, support_access: &SupportAccess) -> Result<(), DbError> {
		self.create_support_access(support_access).await
	}

	async fn get_pending_support_access(
		&self,
		thread_id: &str,
	) -> Result<Option<SupportAccess>, DbError> {
		self.get_pending_support_access(thread_id).await
	}

	async fn get_active_support_access(
		&self,
		thread_id: &str,
		user_id: &UserId,
	) -> Result<Option<SupportAccess>, DbError> {
		self.get_active_support_access(thread_id, user_id).await
	}

	async fn get_any_active_support_access(
		&self,
		thread_id: &str,
	) -> Result<Option<SupportAccess>, DbError> {
		self.get_any_active_support_access(thread_id).await
	}

	async fn approve_support_access(
		&self,
		id: &Uuid,
		approved_by: &UserId,
		expires_at: DateTime<Utc>,
	) -> Result<bool, DbError> {
		self
			.approve_support_access(id, approved_by, expires_at)
			.await
	}

	async fn revoke_support_access(&self, id: &Uuid) -> Result<bool, DbError> {
		self.revoke_support_access(id).await
	}
}

/// Repository for share link and support access database operations.
#[derive(Clone)]
pub struct ShareRepository {
	pool: SqlitePool,
}

impl ShareRepository {
	/// Create a new share repository with the given pool.
	pub fn new(pool: SqlitePool) -> Self {
		Self { pool }
	}

	/// Create a new share link.
	///
	/// # Arguments
	/// * `share_link` - The share link to insert
	#[tracing::instrument(skip(self, share_link), fields(share_link_id = %share_link.id, thread_id = %share_link.thread_id))]
	pub async fn create_share_link(&self, share_link: &ShareLink) -> Result<(), DbError> {
		sqlx::query(
			r#"
			INSERT INTO share_links (
				id, thread_id, token_hash, created_by, created_at, expires_at, revoked_at
			) VALUES (?, ?, ?, ?, ?, ?, ?)
			"#,
		)
		.bind(share_link.id.to_string())
		.bind(&share_link.thread_id)
		.bind(&share_link.token_hash)
		.bind(share_link.created_by.to_string())
		.bind(share_link.created_at.to_rfc3339())
		.bind(share_link.expires_at.map(|dt| dt.to_rfc3339()))
		.bind(share_link.revoked_at.map(|dt| dt.to_rfc3339()))
		.execute(&self.pool)
		.await?;

		tracing::debug!(share_link_id = %share_link.id, thread_id = %share_link.thread_id, "share link created");
		Ok(())
	}

	/// Get an active share link for a thread.
	///
	/// Returns the most recently created non-revoked link.
	#[tracing::instrument(skip(self), fields(thread_id = %thread_id))]
	pub async fn get_share_link_by_thread(
		&self,
		thread_id: &str,
	) -> Result<Option<ShareLink>, DbError> {
		let row = sqlx::query(
			r#"
			SELECT id, thread_id, token_hash, created_by, created_at, expires_at, revoked_at
			FROM share_links
			WHERE thread_id = ? AND revoked_at IS NULL
			ORDER BY created_at DESC
			LIMIT 1
			"#,
		)
		.bind(thread_id)
		.fetch_optional(&self.pool)
		.await?;

		match row {
			Some(row) => {
				let link = parse_share_link_row(&row)?;
				tracing::debug!(share_link_id = %link.id, thread_id = %thread_id, "share link found for thread");
				Ok(Some(link))
			}
			None => Ok(None),
		}
	}

	/// Get a share link by its token hash.
	///
	/// Used for validating share tokens during access.
	#[tracing::instrument(skip(self, token_hash))]
	pub async fn get_share_link_by_hash(
		&self,
		token_hash: &str,
	) -> Result<Option<ShareLink>, DbError> {
		let row = sqlx::query(
			r#"
			SELECT id, thread_id, token_hash, created_by, created_at, expires_at, revoked_at
			FROM share_links
			WHERE token_hash = ?
			"#,
		)
		.bind(token_hash)
		.fetch_optional(&self.pool)
		.await?;

		match row {
			Some(row) => {
				let link = parse_share_link_row(&row)?;
				tracing::debug!(share_link_id = %link.id, thread_id = %link.thread_id, "share link found by hash");
				Ok(Some(link))
			}
			None => Ok(None),
		}
	}

	/// Revoke all active share links for a thread.
	///
	/// # Returns
	/// Number of links revoked.
	#[tracing::instrument(skip(self), fields(thread_id = %thread_id))]
	pub async fn revoke_share_link(&self, thread_id: &str) -> Result<i64, DbError> {
		let now = Utc::now();

		let result = sqlx::query(
			r#"
			UPDATE share_links
			SET revoked_at = ?
			WHERE thread_id = ? AND revoked_at IS NULL
			"#,
		)
		.bind(now.to_rfc3339())
		.bind(thread_id)
		.execute(&self.pool)
		.await?;

		let count = result.rows_affected() as i64;
		if count > 0 {
			tracing::info!(thread_id = %thread_id, count, "share links revoked");
		}
		Ok(count)
	}

	/// Create a new support access request.
	#[tracing::instrument(skip(self, support_access), fields(support_access_id = %support_access.id, thread_id = %support_access.thread_id))]
	pub async fn create_support_access(&self, support_access: &SupportAccess) -> Result<(), DbError> {
		sqlx::query(
			r#"
			INSERT INTO support_access (
				id, thread_id, requested_by, approved_by, requested_at,
				approved_at, expires_at, revoked_at
			) VALUES (?, ?, ?, ?, ?, ?, ?, ?)
			"#,
		)
		.bind(support_access.id.to_string())
		.bind(&support_access.thread_id)
		.bind(support_access.requested_by.to_string())
		.bind(support_access.approved_by.as_ref().map(|u| u.to_string()))
		.bind(support_access.requested_at.to_rfc3339())
		.bind(support_access.approved_at.map(|dt| dt.to_rfc3339()))
		.bind(support_access.expires_at.map(|dt| dt.to_rfc3339()))
		.bind(support_access.revoked_at.map(|dt| dt.to_rfc3339()))
		.execute(&self.pool)
		.await?;

		tracing::debug!(
			support_access_id = %support_access.id,
			thread_id = %support_access.thread_id,
			"support access request created"
		);
		Ok(())
	}

	/// Get a pending support access request for a thread.
	#[tracing::instrument(skip(self), fields(thread_id = %thread_id))]
	pub async fn get_pending_support_access(
		&self,
		thread_id: &str,
	) -> Result<Option<SupportAccess>, DbError> {
		let row = sqlx::query(
			r#"
			SELECT id, thread_id, requested_by, approved_by, requested_at,
			       approved_at, expires_at, revoked_at
			FROM support_access
			WHERE thread_id = ? AND approved_at IS NULL AND revoked_at IS NULL
			ORDER BY requested_at DESC
			LIMIT 1
			"#,
		)
		.bind(thread_id)
		.fetch_optional(&self.pool)
		.await?;

		match row {
			Some(row) => {
				let access = parse_support_access_row(&row)?;
				tracing::debug!(support_access_id = %access.id, thread_id = %thread_id, "pending support access found");
				Ok(Some(access))
			}
			None => Ok(None),
		}
	}

	/// Check if a user has active support access to a thread.
	#[tracing::instrument(skip(self), fields(thread_id = %thread_id, user_id = %user_id))]
	pub async fn get_active_support_access(
		&self,
		thread_id: &str,
		user_id: &UserId,
	) -> Result<Option<SupportAccess>, DbError> {
		let now = Utc::now();

		let row = sqlx::query(
			r#"
			SELECT id, thread_id, requested_by, approved_by, requested_at,
			       approved_at, expires_at, revoked_at
			FROM support_access
			WHERE thread_id = ?
			  AND requested_by = ?
			  AND approved_at IS NOT NULL
			  AND revoked_at IS NULL
			  AND (expires_at IS NULL OR expires_at > ?)
			ORDER BY approved_at DESC
			LIMIT 1
			"#,
		)
		.bind(thread_id)
		.bind(user_id.to_string())
		.bind(now.to_rfc3339())
		.fetch_optional(&self.pool)
		.await?;

		match row {
			Some(row) => {
				let access = parse_support_access_row(&row)?;
				tracing::debug!(
					support_access_id = %access.id,
					thread_id = %thread_id,
					user_id = %user_id,
					"active support access found"
				);
				Ok(Some(access))
			}
			None => Ok(None),
		}
	}

	/// Get any active support access for a thread (for owner to revoke).
	#[tracing::instrument(skip(self), fields(thread_id = %thread_id))]
	pub async fn get_any_active_support_access(
		&self,
		thread_id: &str,
	) -> Result<Option<SupportAccess>, DbError> {
		let now = Utc::now();

		let row = sqlx::query(
			r#"
			SELECT id, thread_id, requested_by, approved_by, requested_at,
			       approved_at, expires_at, revoked_at
			FROM support_access
			WHERE thread_id = ?
			  AND approved_at IS NOT NULL
			  AND revoked_at IS NULL
			  AND (expires_at IS NULL OR expires_at > ?)
			ORDER BY approved_at DESC
			LIMIT 1
			"#,
		)
		.bind(thread_id)
		.bind(now.to_rfc3339())
		.fetch_optional(&self.pool)
		.await?;

		match row {
			Some(row) => {
				let access = parse_support_access_row(&row)?;
				tracing::debug!(
					support_access_id = %access.id,
					thread_id = %thread_id,
					"active support access found for thread"
				);
				Ok(Some(access))
			}
			None => Ok(None),
		}
	}

	/// Approve a support access request.
	#[tracing::instrument(skip(self), fields(support_access_id = %id, approved_by = %approved_by))]
	pub async fn approve_support_access(
		&self,
		id: &Uuid,
		approved_by: &UserId,
		expires_at: DateTime<Utc>,
	) -> Result<bool, DbError> {
		let now = Utc::now();

		let result = sqlx::query(
			r#"
			UPDATE support_access
			SET approved_by = ?, approved_at = ?, expires_at = ?
			WHERE id = ? AND approved_at IS NULL AND revoked_at IS NULL
			"#,
		)
		.bind(approved_by.to_string())
		.bind(now.to_rfc3339())
		.bind(expires_at.to_rfc3339())
		.bind(id.to_string())
		.execute(&self.pool)
		.await?;

		let approved = result.rows_affected() > 0;
		if approved {
			tracing::info!(support_access_id = %id, approved_by = %approved_by, "support access approved");
		}
		Ok(approved)
	}

	/// Revoke a support access grant.
	#[tracing::instrument(skip(self), fields(support_access_id = %id))]
	pub async fn revoke_support_access(&self, id: &Uuid) -> Result<bool, DbError> {
		let now = Utc::now();

		let result = sqlx::query(
			r#"
			UPDATE support_access
			SET revoked_at = ?
			WHERE id = ? AND revoked_at IS NULL
			"#,
		)
		.bind(now.to_rfc3339())
		.bind(id.to_string())
		.execute(&self.pool)
		.await?;

		let revoked = result.rows_affected() > 0;
		if revoked {
			tracing::info!(support_access_id = %id, "support access revoked");
		}
		Ok(revoked)
	}
}

fn parse_share_link_row(row: &sqlx::sqlite::SqliteRow) -> Result<ShareLink, DbError> {
	let id_str: String = row.get("id");
	let thread_id: String = row.get("thread_id");
	let token_hash: String = row.get("token_hash");
	let created_by_str: String = row.get("created_by");
	let created_at_str: String = row.get("created_at");
	let expires_at_str: Option<String> = row.get("expires_at");
	let revoked_at_str: Option<String> = row.get("revoked_at");

	let id = Uuid::parse_str(&id_str)
		.map_err(|e| DbError::Internal(format!("Invalid share_link id UUID: {e}")))?;
	let created_by = Uuid::parse_str(&created_by_str)
		.map_err(|e| DbError::Internal(format!("Invalid created_by UUID: {e}")))?;

	let created_at = DateTime::parse_from_rfc3339(&created_at_str)
		.map_err(|e| DbError::Internal(format!("Invalid created_at: {e}")))?
		.with_timezone(&Utc);

	let expires_at = expires_at_str
		.map(|s| {
			DateTime::parse_from_rfc3339(&s)
				.map(|dt| dt.with_timezone(&Utc))
				.map_err(|e| DbError::Internal(format!("Invalid expires_at: {e}")))
		})
		.transpose()?;

	let revoked_at = revoked_at_str
		.map(|s| {
			DateTime::parse_from_rfc3339(&s)
				.map(|dt| dt.with_timezone(&Utc))
				.map_err(|e| DbError::Internal(format!("Invalid revoked_at: {e}")))
		})
		.transpose()?;

	Ok(ShareLink {
		id,
		thread_id,
		token_hash,
		created_by: UserId::new(created_by),
		created_at,
		expires_at,
		revoked_at,
	})
}

fn parse_support_access_row(row: &sqlx::sqlite::SqliteRow) -> Result<SupportAccess, DbError> {
	let id_str: String = row.get("id");
	let thread_id: String = row.get("thread_id");
	let requested_by_str: String = row.get("requested_by");
	let approved_by_str: Option<String> = row.get("approved_by");
	let requested_at_str: String = row.get("requested_at");
	let approved_at_str: Option<String> = row.get("approved_at");
	let expires_at_str: Option<String> = row.get("expires_at");
	let revoked_at_str: Option<String> = row.get("revoked_at");

	let id = Uuid::parse_str(&id_str)
		.map_err(|e| DbError::Internal(format!("Invalid support_access id UUID: {e}")))?;
	let requested_by = Uuid::parse_str(&requested_by_str)
		.map_err(|e| DbError::Internal(format!("Invalid requested_by UUID: {e}")))?;

	let approved_by = approved_by_str
		.map(|s| {
			Uuid::parse_str(&s)
				.map(UserId::new)
				.map_err(|e| DbError::Internal(format!("Invalid approved_by UUID: {e}")))
		})
		.transpose()?;

	let requested_at = DateTime::parse_from_rfc3339(&requested_at_str)
		.map_err(|e| DbError::Internal(format!("Invalid requested_at: {e}")))?
		.with_timezone(&Utc);

	let approved_at = approved_at_str
		.map(|s| {
			DateTime::parse_from_rfc3339(&s)
				.map(|dt| dt.with_timezone(&Utc))
				.map_err(|e| DbError::Internal(format!("Invalid approved_at: {e}")))
		})
		.transpose()?;

	let expires_at = expires_at_str
		.map(|s| {
			DateTime::parse_from_rfc3339(&s)
				.map(|dt| dt.with_timezone(&Utc))
				.map_err(|e| DbError::Internal(format!("Invalid expires_at: {e}")))
		})
		.transpose()?;

	let revoked_at = revoked_at_str
		.map(|s| {
			DateTime::parse_from_rfc3339(&s)
				.map(|dt| dt.with_timezone(&Utc))
				.map_err(|e| DbError::Internal(format!("Invalid revoked_at: {e}")))
		})
		.transpose()?;

	Ok(SupportAccess {
		id,
		thread_id,
		requested_by: UserId::new(requested_by),
		approved_by,
		requested_at,
		approved_at,
		expires_at,
		revoked_at,
	})
}

#[cfg(test)]
mod tests {
	use super::*;
	use proptest::prelude::*;
	use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
	use std::collections::HashSet;
	use std::str::FromStr;

	async fn create_share_test_pool() -> SqlitePool {
		let options = SqliteConnectOptions::from_str(":memory:")
			.unwrap()
			.create_if_missing(true);

		let pool = SqlitePoolOptions::new()
			.max_connections(1)
			.connect_with(options)
			.await
			.expect("Failed to create test pool");

		sqlx::query(
			r#"
			CREATE TABLE IF NOT EXISTS share_links (
				id TEXT PRIMARY KEY,
				thread_id TEXT NOT NULL,
				token_hash TEXT NOT NULL UNIQUE,
				created_by TEXT NOT NULL,
				created_at TEXT NOT NULL,
				expires_at TEXT,
				revoked_at TEXT
			)
			"#,
		)
		.execute(&pool)
		.await
		.unwrap();

		sqlx::query(
			r#"
			CREATE TABLE IF NOT EXISTS support_access (
				id TEXT PRIMARY KEY,
				thread_id TEXT NOT NULL,
				requested_by TEXT NOT NULL,
				approved_by TEXT,
				requested_at TEXT NOT NULL,
				approved_at TEXT,
				expires_at TEXT,
				revoked_at TEXT
			)
			"#,
		)
		.execute(&pool)
		.await
		.unwrap();

		pool
	}

	async fn make_repo() -> ShareRepository {
		let pool = create_share_test_pool().await;
		ShareRepository::new(pool)
	}

	fn make_share_link(thread_id: &str, token_hash: &str) -> ShareLink {
		ShareLink {
			id: Uuid::new_v4(),
			thread_id: thread_id.to_string(),
			token_hash: token_hash.to_string(),
			created_by: UserId::generate(),
			created_at: Utc::now(),
			expires_at: None,
			revoked_at: None,
		}
	}

	#[tokio::test]
	async fn test_create_and_get_share() {
		let repo = make_repo().await;
		let share_link = make_share_link("T-abc12345", "token_hash_123");

		repo.create_share_link(&share_link).await.unwrap();

		let result = repo.get_share_link_by_thread("T-abc12345").await.unwrap();
		assert!(result.is_some());
		let result = result.unwrap();
		assert_eq!(result.id, share_link.id);
		assert_eq!(result.thread_id, "T-abc12345");
		assert_eq!(result.token_hash, "token_hash_123");
		assert_eq!(result.created_by, share_link.created_by);
		assert!(result.revoked_at.is_none());
	}

	#[tokio::test]
	async fn test_get_share_not_found() {
		let repo = make_repo().await;
		let result = repo
			.get_share_link_by_thread("T-nonexistent")
			.await
			.unwrap();
		assert!(result.is_none());
	}

	#[tokio::test]
	async fn test_get_by_token() {
		let repo = make_repo().await;
		let token_hash = "unique_token_hash_456";
		let share_link = make_share_link("T-def67890", token_hash);

		repo.create_share_link(&share_link).await.unwrap();

		let result = repo.get_share_link_by_hash(token_hash).await.unwrap();
		assert!(result.is_some());
		let result = result.unwrap();
		assert_eq!(result.id, share_link.id);
		assert_eq!(result.token_hash, token_hash);
	}

	#[tokio::test]
	async fn test_delete_share() {
		let repo = make_repo().await;
		let share_link = make_share_link("T-revoke123", "revoke_hash_789");

		repo.create_share_link(&share_link).await.unwrap();

		let result = repo.get_share_link_by_thread("T-revoke123").await.unwrap();
		assert!(result.is_some());

		let revoked_count = repo.revoke_share_link("T-revoke123").await.unwrap();
		assert_eq!(revoked_count, 1);

		let result = repo.get_share_link_by_thread("T-revoke123").await.unwrap();
		assert!(result.is_none());
	}

	proptest! {
		#[test]
		fn uuid_generation_is_unique(count in 1..1000usize) {
			let mut ids = HashSet::new();
			for _ in 0..count {
				let id = Uuid::new_v4();
				prop_assert!(ids.insert(id.to_string()), "Generated duplicate UUID");
			}
		}

		#[test]
		fn thread_id_format_is_preserved(thread_id in "T-[a-z0-9]{8}") {
			let parsed = thread_id.clone();
			prop_assert_eq!(parsed, thread_id);
		}
	}
}
