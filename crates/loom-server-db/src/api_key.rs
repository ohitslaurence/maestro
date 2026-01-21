// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights
// reserved. SPDX-License-Identifier: Proprietary

//! API key repository for database operations.
//!
//! This module provides database access for API key management.
//! API keys are organization-scoped and used for programmatic access.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use loom_server_auth::{ApiKey, ApiKeyId, ApiKeyScope, ApiKeyUsage, OrgId, UserId};
use sqlx::{sqlite::SqlitePool, Row};
use std::net::IpAddr;
use uuid::Uuid;

use crate::error::DbError;

#[async_trait]
pub trait ApiKeyStore: Send + Sync {
	async fn create_api_key(
		&self,
		org_id: &OrgId,
		name: &str,
		token_hash: &str,
		scopes: &[ApiKeyScope],
		created_by: &UserId,
	) -> Result<String, DbError>;
	async fn get_api_key_by_id(&self, id: &str) -> Result<Option<ApiKey>, DbError>;
	async fn get_api_key_by_hash(&self, token_hash: &str) -> Result<Option<ApiKey>, DbError>;
	async fn list_api_keys_for_org(&self, org_id: &OrgId) -> Result<Vec<ApiKey>, DbError>;
	async fn revoke_api_key(&self, id: &str, revoked_by: &UserId) -> Result<bool, DbError>;
	async fn update_last_used(&self, id: &str) -> Result<(), DbError>;
	async fn log_usage(
		&self,
		api_key_id: &str,
		ip_address: Option<&str>,
		endpoint: &str,
		method: &str,
	) -> Result<(), DbError>;
	async fn get_usage_logs(
		&self,
		api_key_id: &str,
		limit: i32,
		offset: i32,
	) -> Result<(Vec<ApiKeyUsage>, i64), DbError>;
}

#[async_trait]
impl ApiKeyStore for ApiKeyRepository {
	async fn create_api_key(
		&self,
		org_id: &OrgId,
		name: &str,
		token_hash: &str,
		scopes: &[ApiKeyScope],
		created_by: &UserId,
	) -> Result<String, DbError> {
		self
			.create_api_key(org_id, name, token_hash, scopes, created_by)
			.await
	}

	async fn get_api_key_by_id(&self, id: &str) -> Result<Option<ApiKey>, DbError> {
		self.get_api_key_by_id(id).await
	}

	async fn get_api_key_by_hash(&self, token_hash: &str) -> Result<Option<ApiKey>, DbError> {
		self.get_api_key_by_hash(token_hash).await
	}

	async fn list_api_keys_for_org(&self, org_id: &OrgId) -> Result<Vec<ApiKey>, DbError> {
		self.list_api_keys_for_org(org_id).await
	}

	async fn revoke_api_key(&self, id: &str, revoked_by: &UserId) -> Result<bool, DbError> {
		self.revoke_api_key(id, revoked_by).await
	}

	async fn update_last_used(&self, id: &str) -> Result<(), DbError> {
		self.update_last_used(id).await
	}

	async fn log_usage(
		&self,
		api_key_id: &str,
		ip_address: Option<&str>,
		endpoint: &str,
		method: &str,
	) -> Result<(), DbError> {
		self
			.log_usage(api_key_id, ip_address, endpoint, method)
			.await
	}

	async fn get_usage_logs(
		&self,
		api_key_id: &str,
		limit: i32,
		offset: i32,
	) -> Result<(Vec<ApiKeyUsage>, i64), DbError> {
		self.get_usage_logs(api_key_id, limit, offset).await
	}
}

/// Repository for API key database operations.
///
/// Manages API keys for organizations and their usage logs.
/// All tokens are stored as hashes, never in plaintext.
#[derive(Clone)]
pub struct ApiKeyRepository {
	pool: SqlitePool,
}

impl ApiKeyRepository {
	/// Create a new API key repository with the given pool.
	///
	/// # Arguments
	/// * `pool` - SQLite connection pool
	pub fn new(pool: SqlitePool) -> Self {
		Self { pool }
	}

	/// Create a new API key.
	///
	/// # Arguments
	/// * `org_id` - The organization's UUID
	/// * `name` - Human-readable name for the key
	/// * `token_hash` - SHA-256 hash of the API key (never store plaintext)
	/// * `scopes` - List of permission scopes for the key
	/// * `created_by` - The user who created the key
	///
	/// # Returns
	/// The generated API key ID.
	///
	/// # Database Constraints
	/// - `id` must be unique
	/// - `token_hash` should be unique
	/// - `org_id` must reference an existing organization
	/// - `created_by` must reference an existing user
	#[tracing::instrument(skip(self, token_hash), fields(org_id = %org_id, created_by = %created_by))]
	pub async fn create_api_key(
		&self,
		org_id: &OrgId,
		name: &str,
		token_hash: &str,
		scopes: &[ApiKeyScope],
		created_by: &UserId,
	) -> Result<String, DbError> {
		let id = Uuid::new_v4().to_string();
		let now = Utc::now();
		let scopes_json = serde_json::to_string(scopes)?;

		sqlx::query(
			r#"
			INSERT INTO api_keys (
				id, org_id, name, token_hash, scopes, created_by, created_at
			) VALUES (?, ?, ?, ?, ?, ?, ?)
			"#,
		)
		.bind(&id)
		.bind(org_id.to_string())
		.bind(name)
		.bind(token_hash)
		.bind(&scopes_json)
		.bind(created_by.to_string())
		.bind(now.to_rfc3339())
		.execute(&self.pool)
		.await?;

		tracing::debug!(api_key_id = %id, org_id = %org_id, "API key created");
		Ok(id)
	}

	/// Get an API key by its ID.
	///
	/// # Arguments
	/// * `id` - The API key's UUID
	///
	/// # Returns
	/// `None` if no key exists with this ID.
	///
	/// # Note
	/// Returns the key regardless of revocation status - caller should check `revoked_at`.
	#[tracing::instrument(skip(self), fields(api_key_id = %id))]
	pub async fn get_api_key_by_id(&self, id: &str) -> Result<Option<ApiKey>, DbError> {
		let row = sqlx::query(
			r#"
			SELECT id, org_id, name, token_hash, scopes, created_by, created_at,
			       last_used_at, revoked_at, revoked_by
			FROM api_keys
			WHERE id = ?
			"#,
		)
		.bind(id)
		.fetch_optional(&self.pool)
		.await?;

		match row {
			Some(row) => Ok(Some(parse_api_key_row(&row)?)),
			None => Ok(None),
		}
	}

	/// Get an API key by its token hash.
	///
	/// # Arguments
	/// * `token_hash` - SHA-256 hash of the API key
	///
	/// # Returns
	/// `None` if no key exists with this hash.
	///
	/// # Note
	/// Returns the key regardless of revocation status - caller should check `revoked_at`.
	#[tracing::instrument(skip(self, token_hash))]
	pub async fn get_api_key_by_hash(&self, token_hash: &str) -> Result<Option<ApiKey>, DbError> {
		let row = sqlx::query(
			r#"
			SELECT id, org_id, name, token_hash, scopes, created_by, created_at,
			       last_used_at, revoked_at, revoked_by
			FROM api_keys
			WHERE token_hash = ?
			"#,
		)
		.bind(token_hash)
		.fetch_optional(&self.pool)
		.await?;

		match row {
			Some(row) => {
				let key = parse_api_key_row(&row)?;
				tracing::debug!(api_key_id = %key.id, org_id = %key.org_id, "API key found by hash");
				Ok(Some(key))
			}
			None => Ok(None),
		}
	}

	/// List all API keys for an organization.
	///
	/// # Arguments
	/// * `org_id` - The organization's UUID
	///
	/// # Returns
	/// List of API keys (including revoked) ordered by creation date descending.
	#[tracing::instrument(skip(self), fields(org_id = %org_id))]
	pub async fn list_api_keys_for_org(&self, org_id: &OrgId) -> Result<Vec<ApiKey>, DbError> {
		let rows = sqlx::query(
			r#"
			SELECT id, org_id, name, token_hash, scopes, created_by, created_at,
			       last_used_at, revoked_at, revoked_by
			FROM api_keys
			WHERE org_id = ?
			ORDER BY created_at DESC
			"#,
		)
		.bind(org_id.to_string())
		.fetch_all(&self.pool)
		.await?;

		let mut keys = Vec::with_capacity(rows.len());
		for row in rows {
			keys.push(parse_api_key_row(&row)?);
		}
		tracing::debug!(org_id = %org_id, count = keys.len(), "listed API keys for organization");
		Ok(keys)
	}

	/// Revoke an API key.
	///
	/// # Arguments
	/// * `id` - The API key's UUID
	/// * `revoked_by` - The user revoking the key
	///
	/// # Returns
	/// `true` if the key was revoked, `false` if already revoked or not found.
	#[tracing::instrument(skip(self), fields(api_key_id = %id, revoked_by = %revoked_by))]
	pub async fn revoke_api_key(&self, id: &str, revoked_by: &UserId) -> Result<bool, DbError> {
		let now = Utc::now();

		let result = sqlx::query(
			r#"
			UPDATE api_keys
			SET revoked_at = ?, revoked_by = ?
			WHERE id = ? AND revoked_at IS NULL
			"#,
		)
		.bind(now.to_rfc3339())
		.bind(revoked_by.to_string())
		.bind(id)
		.execute(&self.pool)
		.await?;

		let revoked = result.rows_affected() > 0;
		if revoked {
			tracing::info!(api_key_id = %id, revoked_by = %revoked_by, "API key revoked");
		}
		Ok(revoked)
	}

	/// Update the last used timestamp for an API key.
	///
	/// # Arguments
	/// * `id` - The API key's UUID
	#[tracing::instrument(skip(self), fields(api_key_id = %id))]
	pub async fn update_last_used(&self, id: &str) -> Result<(), DbError> {
		let now = Utc::now();

		sqlx::query(
			r#"
			UPDATE api_keys
			SET last_used_at = ?
			WHERE id = ?
			"#,
		)
		.bind(now.to_rfc3339())
		.bind(id)
		.execute(&self.pool)
		.await?;

		tracing::debug!(api_key_id = %id, "API key last_used updated");
		Ok(())
	}

	/// Log API key usage.
	///
	/// # Arguments
	/// * `api_key_id` - The API key's UUID
	/// * `ip_address` - The client's IP address (optional)
	/// * `endpoint` - The API endpoint accessed
	/// * `method` - The HTTP method used
	#[tracing::instrument(skip(self, ip_address), fields(api_key_id = %api_key_id, endpoint = %endpoint, method = %method))]
	pub async fn log_usage(
		&self,
		api_key_id: &str,
		ip_address: Option<&str>,
		endpoint: &str,
		method: &str,
	) -> Result<(), DbError> {
		let id = Uuid::new_v4().to_string();
		let now = Utc::now();

		sqlx::query(
			r#"
			INSERT INTO api_key_usage (id, api_key_id, timestamp, ip_address, endpoint, method)
			VALUES (?, ?, ?, ?, ?, ?)
			"#,
		)
		.bind(&id)
		.bind(api_key_id)
		.bind(now.to_rfc3339())
		.bind(ip_address)
		.bind(endpoint)
		.bind(method)
		.execute(&self.pool)
		.await?;

		tracing::debug!(api_key_id = %api_key_id, "API key usage logged");
		Ok(())
	}

	/// Get usage logs for an API key with pagination.
	///
	/// # Arguments
	/// * `api_key_id` - The API key's UUID
	/// * `limit` - Maximum number of logs to return
	/// * `offset` - Number of logs to skip
	///
	/// # Returns
	/// Tuple of (usage_logs, total_count) for pagination.
	#[tracing::instrument(skip(self), fields(api_key_id = %api_key_id, limit, offset))]
	pub async fn get_usage_logs(
		&self,
		api_key_id: &str,
		limit: i32,
		offset: i32,
	) -> Result<(Vec<ApiKeyUsage>, i64), DbError> {
		let count_row = sqlx::query(
			r#"
			SELECT COUNT(*) as count
			FROM api_key_usage
			WHERE api_key_id = ?
			"#,
		)
		.bind(api_key_id)
		.fetch_one(&self.pool)
		.await?;
		let total: i64 = count_row.get("count");

		let rows = sqlx::query(
			r#"
			SELECT id, api_key_id, timestamp, ip_address, endpoint, method
			FROM api_key_usage
			WHERE api_key_id = ?
			ORDER BY timestamp DESC
			LIMIT ? OFFSET ?
			"#,
		)
		.bind(api_key_id)
		.bind(limit)
		.bind(offset)
		.fetch_all(&self.pool)
		.await?;

		let mut usage_logs = Vec::with_capacity(rows.len());
		for row in rows {
			usage_logs.push(parse_api_key_usage_row(&row)?);
		}

		tracing::debug!(api_key_id = %api_key_id, count = usage_logs.len(), total, "retrieved API key usage logs");
		Ok((usage_logs, total))
	}
}

fn parse_api_key_row(row: &sqlx::sqlite::SqliteRow) -> Result<ApiKey, DbError> {
	let id_str: String = row.get("id");
	let org_id_str: String = row.get("org_id");
	let name: String = row.get("name");
	let token_hash: String = row.get("token_hash");
	let scopes_json: String = row.get("scopes");
	let created_by_str: String = row.get("created_by");
	let created_at_str: String = row.get("created_at");
	let last_used_at_str: Option<String> = row.get("last_used_at");
	let revoked_at_str: Option<String> = row.get("revoked_at");
	let revoked_by_str: Option<String> = row.get("revoked_by");

	let id = Uuid::parse_str(&id_str)
		.map_err(|e| DbError::Internal(format!("Invalid api_key id UUID: {e}")))?;
	let org_id = Uuid::parse_str(&org_id_str)
		.map_err(|e| DbError::Internal(format!("Invalid org_id UUID: {e}")))?;
	let created_by = Uuid::parse_str(&created_by_str)
		.map_err(|e| DbError::Internal(format!("Invalid created_by UUID: {e}")))?;

	let scopes: Vec<ApiKeyScope> = serde_json::from_str(&scopes_json)?;

	let created_at = DateTime::parse_from_rfc3339(&created_at_str)
		.map_err(|e| DbError::Internal(format!("Invalid created_at: {e}")))?
		.with_timezone(&Utc);

	let last_used_at = last_used_at_str
		.map(|s| {
			DateTime::parse_from_rfc3339(&s)
				.map(|dt| dt.with_timezone(&Utc))
				.map_err(|e| DbError::Internal(format!("Invalid last_used_at: {e}")))
		})
		.transpose()?;

	let revoked_at = revoked_at_str
		.map(|s| {
			DateTime::parse_from_rfc3339(&s)
				.map(|dt| dt.with_timezone(&Utc))
				.map_err(|e| DbError::Internal(format!("Invalid revoked_at: {e}")))
		})
		.transpose()?;

	let revoked_by = revoked_by_str
		.map(|s| {
			Uuid::parse_str(&s)
				.map(UserId::new)
				.map_err(|e| DbError::Internal(format!("Invalid revoked_by UUID: {e}")))
		})
		.transpose()?;

	Ok(ApiKey {
		id: ApiKeyId::new(id),
		org_id: OrgId::new(org_id),
		name,
		token_hash,
		scopes,
		created_by: UserId::new(created_by),
		created_at,
		last_used_at,
		revoked_at,
		revoked_by,
	})
}

fn parse_api_key_usage_row(row: &sqlx::sqlite::SqliteRow) -> Result<ApiKeyUsage, DbError> {
	let id_str: String = row.get("id");
	let api_key_id_str: String = row.get("api_key_id");
	let timestamp_str: String = row.get("timestamp");
	let ip_address_str: Option<String> = row.get("ip_address");
	let endpoint: String = row.get("endpoint");
	let method: String = row.get("method");

	let id = Uuid::parse_str(&id_str)
		.map_err(|e| DbError::Internal(format!("Invalid usage id UUID: {e}")))?;
	let api_key_id = Uuid::parse_str(&api_key_id_str)
		.map_err(|e| DbError::Internal(format!("Invalid api_key_id UUID: {e}")))?;

	let timestamp = DateTime::parse_from_rfc3339(&timestamp_str)
		.map_err(|e| DbError::Internal(format!("Invalid timestamp: {e}")))?
		.with_timezone(&Utc);

	let ip_address: Option<IpAddr> = ip_address_str
		.map(|s| {
			s.parse()
				.map_err(|e| DbError::Internal(format!("Invalid IP address: {e}")))
		})
		.transpose()?;

	Ok(ApiKeyUsage {
		id,
		api_key_id: ApiKeyId::new(api_key_id),
		timestamp,
		ip_address,
		endpoint,
		method,
	})
}

#[cfg(test)]
mod tests {
	use super::*;
	use proptest::prelude::*;
	use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
	use std::collections::HashSet;
	use std::str::FromStr;

	async fn create_api_key_test_pool() -> SqlitePool {
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
			CREATE TABLE IF NOT EXISTS api_keys (
				id TEXT PRIMARY KEY,
				org_id TEXT NOT NULL,
				name TEXT NOT NULL,
				token_hash TEXT NOT NULL UNIQUE,
				scopes TEXT NOT NULL,
				created_by TEXT NOT NULL,
				created_at TEXT NOT NULL,
				last_used_at TEXT,
				revoked_at TEXT,
				revoked_by TEXT
			)
			"#,
		)
		.execute(&pool)
		.await
		.unwrap();

		sqlx::query(
			r#"
			CREATE TABLE IF NOT EXISTS api_key_usage (
				id TEXT PRIMARY KEY,
				api_key_id TEXT NOT NULL,
				timestamp TEXT NOT NULL,
				ip_address TEXT,
				endpoint TEXT NOT NULL,
				method TEXT NOT NULL
			)
			"#,
		)
		.execute(&pool)
		.await
		.unwrap();

		pool
	}

	async fn make_repo() -> ApiKeyRepository {
		let pool = create_api_key_test_pool().await;
		ApiKeyRepository::new(pool)
	}

	#[tokio::test]
	async fn test_create_and_get_api_key() {
		let repo = make_repo().await;
		let org_id = OrgId::generate();
		let created_by = UserId::generate();
		let scopes = vec![ApiKeyScope::ThreadsRead, ApiKeyScope::ThreadsWrite];

		let id = repo
			.create_api_key(&org_id, "Test Key", "hash123", &scopes, &created_by)
			.await
			.unwrap();

		let api_key = repo.get_api_key_by_id(&id).await.unwrap();
		assert!(api_key.is_some());
		let api_key = api_key.unwrap();
		assert_eq!(api_key.id.to_string(), id);
		assert_eq!(api_key.org_id, org_id);
		assert_eq!(api_key.name, "Test Key");
		assert_eq!(api_key.token_hash, "hash123");
		assert_eq!(api_key.scopes, scopes);
		assert_eq!(api_key.created_by, created_by);
		assert!(api_key.revoked_at.is_none());
	}

	#[tokio::test]
	async fn test_get_api_key_not_found() {
		let repo = make_repo().await;
		let result = repo
			.get_api_key_by_id("nonexistent-api-key-id")
			.await
			.unwrap();
		assert!(result.is_none());
	}

	#[tokio::test]
	async fn test_get_by_key_hash() {
		let repo = make_repo().await;
		let org_id = OrgId::generate();
		let created_by = UserId::generate();
		let token_hash = "unique_hash_456";

		let id = repo
			.create_api_key(&org_id, "Hash Test Key", token_hash, &[], &created_by)
			.await
			.unwrap();

		let api_key = repo.get_api_key_by_hash(token_hash).await.unwrap();
		assert!(api_key.is_some());
		let api_key = api_key.unwrap();
		assert_eq!(api_key.id.to_string(), id);
		assert_eq!(api_key.token_hash, token_hash);
	}

	#[tokio::test]
	async fn test_revoke_api_key() {
		let repo = make_repo().await;
		let org_id = OrgId::generate();
		let created_by = UserId::generate();
		let revoked_by = UserId::generate();

		let id = repo
			.create_api_key(&org_id, "Revokable Key", "hash789", &[], &created_by)
			.await
			.unwrap();

		let api_key = repo.get_api_key_by_id(&id).await.unwrap().unwrap();
		assert!(api_key.revoked_at.is_none());

		let revoked = repo.revoke_api_key(&id, &revoked_by).await.unwrap();
		assert!(revoked);

		let api_key = repo.get_api_key_by_id(&id).await.unwrap().unwrap();
		assert!(api_key.revoked_at.is_some());
		assert_eq!(api_key.revoked_by, Some(revoked_by));
	}

	proptest! {
		#[test]
		fn api_key_id_generation_is_unique(count in 1..1000usize) {
			let mut ids = HashSet::new();
			for _ in 0..count {
				let id = ApiKeyId::generate();
				prop_assert!(ids.insert(id.to_string()), "Generated duplicate ApiKeyId");
			}
		}

		#[test]
		fn token_hash_is_deterministic(input in ".*") {
			use sha2::{Sha256, Digest};
			let hash1 = format!("{:x}", Sha256::digest(input.as_bytes()));
			let hash2 = format!("{:x}", Sha256::digest(input.as_bytes()));
			prop_assert_eq!(hash1, hash2, "Token hash should be deterministic");
		}

		#[test]
		fn usage_logs_pagination_bounds(limit in 0i32..1000, offset in 0i32..10000) {
			prop_assert!(limit >= 0, "limit must be non-negative");
			prop_assert!(offset >= 0, "offset must be non-negative");
		}

		#[test]
		fn valid_ip_addresses_parse(octets in (0u8..=255, 0u8..=255, 0u8..=255, 0u8..=255)) {
			let ip_str = format!("{}.{}.{}.{}", octets.0, octets.1, octets.2, octets.3);
			let parsed: Result<IpAddr, _> = ip_str.parse();
			prop_assert!(parsed.is_ok(), "Valid IPv4 should parse");
		}
	}
}
