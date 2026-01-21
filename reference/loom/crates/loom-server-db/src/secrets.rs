// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! Secrets repository for database operations.
//!
//! This module provides database access for secrets management including:
//! - Secret CRUD operations
//! - Version management
//! - Encrypted DEK storage
//! - Scope-based queries

use async_trait::async_trait;
use chrono::Utc;
use sqlx::{sqlite::SqlitePool, Row};

use crate::error::{DbError, Result};

/// Stored secret metadata row.
#[derive(Debug, Clone)]
pub struct SecretRow {
	pub id: String,
	pub org_id: String,
	pub scope: String,
	pub repo_id: Option<String>,
	pub weaver_id: Option<String>,
	pub name: String,
	pub description: Option<String>,
	pub current_version: i32,
	pub created_by: String,
	pub created_at: String,
	pub updated_at: String,
}

/// Stored secret version row.
#[derive(Debug, Clone)]
pub struct SecretVersionRow {
	pub id: String,
	pub secret_id: String,
	pub version: i32,
	pub ciphertext: Vec<u8>,
	pub nonce: Vec<u8>,
	pub dek_id: String,
	pub created_by: String,
	pub created_at: String,
	pub expires_at: Option<String>,
	pub disabled_at: Option<String>,
}

/// Stored encrypted DEK row.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct EncryptedDekRow {
	pub id: String,
	pub encrypted_key: Vec<u8>,
	pub nonce: Vec<u8>,
	pub kek_version: i32,
	pub created_at: String,
}

/// Parameters for creating a secret.
#[derive(Debug, Clone)]
pub struct CreateSecretParams {
	pub id: String,
	pub org_id: String,
	pub scope: String,
	pub repo_id: Option<String>,
	pub weaver_id: Option<String>,
	pub name: String,
	pub description: Option<String>,
	pub created_by: String,
	pub created_at: String,
	pub updated_at: String,
}

/// Parameters for creating a secret version.
#[derive(Debug, Clone)]
pub struct CreateVersionParams {
	pub id: String,
	pub secret_id: String,
	pub version: i32,
	pub ciphertext: Vec<u8>,
	pub nonce: Vec<u8>,
	pub dek_id: String,
	pub created_by: String,
	pub created_at: String,
	pub expires_at: Option<String>,
}

/// Parameters for storing an encrypted DEK.
#[derive(Debug, Clone)]
pub struct StoreDekParams {
	pub id: String,
	pub encrypted_key: Vec<u8>,
	pub nonce: Vec<u8>,
	pub kek_version: i32,
	pub created_at: String,
}

/// Filter for listing secrets.
#[derive(Debug, Clone, Default)]
pub struct SecretFilterParams {
	pub org_id: Option<String>,
	pub scope: Option<String>,
	pub repo_id: Option<String>,
	pub weaver_id: Option<String>,
	pub name: Option<String>,
}

#[async_trait]
pub trait SecretsStore: Send + Sync {
	async fn insert_secret(&self, params: &CreateSecretParams) -> Result<()>;
	async fn insert_version(&self, params: &CreateVersionParams) -> Result<()>;
	async fn get_secret(&self, id: &str) -> Result<Option<SecretRow>>;
	async fn get_secret_by_name(
		&self,
		org_id: &str,
		scope: &str,
		repo_id: Option<&str>,
		weaver_id: Option<&str>,
		name: &str,
	) -> Result<Option<SecretRow>>;
	async fn list_secrets(&self, filter: &SecretFilterParams) -> Result<Vec<SecretRow>>;
	async fn get_current_version(&self, secret_id: &str) -> Result<Option<SecretVersionRow>>;
	async fn get_version(&self, secret_id: &str, version: i32) -> Result<Option<SecretVersionRow>>;
	async fn disable_version(&self, version_id: &str) -> Result<()>;
	async fn delete_secret(&self, id: &str) -> Result<()>;
	async fn store_dek(&self, params: &StoreDekParams) -> Result<()>;
	async fn get_dek(&self, id: &str) -> Result<Option<EncryptedDekRow>>;
}

/// Repository for secrets database operations.
#[derive(Clone)]
pub struct SecretsRepository {
	pool: SqlitePool,
}

impl SecretsRepository {
	/// Create a new secrets repository with the given pool.
	pub fn new(pool: SqlitePool) -> Self {
		Self { pool }
	}

	/// Insert a secret record.
	///
	/// Returns `Err(DbError::Conflict)` if a secret with the same name already exists.
	#[tracing::instrument(skip(self, params), fields(secret_id = %params.id, name = %params.name))]
	pub async fn insert_secret(&self, params: &CreateSecretParams) -> Result<()> {
		let result = sqlx::query(
			r#"
			INSERT INTO secrets (id, org_id, scope, repo_id, weaver_id, name, description, current_version, created_by, created_at, updated_at)
			VALUES (?, ?, ?, ?, ?, ?, ?, 1, ?, ?, ?)
			"#,
		)
		.bind(&params.id)
		.bind(&params.org_id)
		.bind(&params.scope)
		.bind(&params.repo_id)
		.bind(&params.weaver_id)
		.bind(&params.name)
		.bind(&params.description)
		.bind(&params.created_by)
		.bind(&params.created_at)
		.bind(&params.updated_at)
		.execute(&self.pool)
		.await;

		match result {
			Ok(_) => {
				tracing::debug!(secret_id = %params.id, name = %params.name, "secret created");
				Ok(())
			}
			Err(e) if is_unique_constraint_error(&e) => Err(DbError::Conflict(format!(
				"secret already exists: {}",
				params.name
			))),
			Err(e) => Err(DbError::Sqlx(e)),
		}
	}

	/// Insert a secret version record.
	#[tracing::instrument(skip(self, params), fields(version_id = %params.id, secret_id = %params.secret_id, version = %params.version))]
	pub async fn insert_version(&self, params: &CreateVersionParams) -> Result<()> {
		sqlx::query(
			r#"
			INSERT INTO secret_versions (id, secret_id, version, ciphertext, nonce, dek_id, created_by, created_at, expires_at)
			VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
			"#,
		)
		.bind(&params.id)
		.bind(&params.secret_id)
		.bind(params.version)
		.bind(&params.ciphertext)
		.bind(&params.nonce)
		.bind(&params.dek_id)
		.bind(&params.created_by)
		.bind(&params.created_at)
		.bind(&params.expires_at)
		.execute(&self.pool)
		.await?;

		tracing::debug!(version_id = %params.id, secret_id = %params.secret_id, version = params.version, "secret version created");
		Ok(())
	}

	/// Get the next version number for a secret (within a transaction).
	pub async fn get_next_version_number(
		&self,
		tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
		secret_id: &str,
	) -> Result<i32> {
		let next_version: i32 = sqlx::query_scalar(
			"SELECT COALESCE(MAX(version), 0) + 1 FROM secret_versions WHERE secret_id = ?",
		)
		.bind(secret_id)
		.fetch_one(&mut **tx)
		.await?;

		Ok(next_version)
	}

	/// Insert a secret version within a transaction.
	pub async fn insert_version_in_tx(
		&self,
		tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
		params: &CreateVersionParams,
	) -> Result<()> {
		sqlx::query(
			r#"
			INSERT INTO secret_versions (id, secret_id, version, ciphertext, nonce, dek_id, created_by, created_at, expires_at)
			VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
			"#,
		)
		.bind(&params.id)
		.bind(&params.secret_id)
		.bind(params.version)
		.bind(&params.ciphertext)
		.bind(&params.nonce)
		.bind(&params.dek_id)
		.bind(&params.created_by)
		.bind(&params.created_at)
		.bind(&params.expires_at)
		.execute(&mut **tx)
		.await?;

		tracing::debug!(version_id = %params.id, secret_id = %params.secret_id, version = params.version, "secret version created in tx");
		Ok(())
	}

	/// Update a secret's current version within a transaction.
	///
	/// Returns the number of rows affected (0 if secret not found or deleted).
	pub async fn update_current_version_in_tx(
		&self,
		tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
		secret_id: &str,
		version: i32,
		updated_at: &str,
	) -> Result<u64> {
		let result = sqlx::query(
			"UPDATE secrets SET current_version = ?, updated_at = ? WHERE id = ? AND deleted_at IS NULL",
		)
		.bind(version)
		.bind(updated_at)
		.bind(secret_id)
		.execute(&mut **tx)
		.await?;

		Ok(result.rows_affected())
	}

	/// Get a secret by ID.
	#[tracing::instrument(skip(self), fields(secret_id = %id))]
	pub async fn get_secret(&self, id: &str) -> Result<Option<SecretRow>> {
		let row = sqlx::query(
			r#"
			SELECT id, org_id, scope, repo_id, weaver_id, name, description, current_version, created_by, created_at, updated_at
			FROM secrets
			WHERE id = ? AND deleted_at IS NULL
			"#,
		)
		.bind(id)
		.fetch_optional(&self.pool)
		.await?;

		match row {
			Some(row) => Ok(Some(parse_secret_row(&row)?)),
			None => Ok(None),
		}
	}

	/// Get a secret by name and scope.
	#[tracing::instrument(skip(self), fields(org_id = %org_id, scope = %scope, name = %name))]
	pub async fn get_secret_by_name(
		&self,
		org_id: &str,
		scope: &str,
		repo_id: Option<&str>,
		weaver_id: Option<&str>,
		name: &str,
	) -> Result<Option<SecretRow>> {
		let row = sqlx::query(
			r#"
			SELECT id, org_id, scope, repo_id, weaver_id, name, description, current_version, created_by, created_at, updated_at
			FROM secrets
			WHERE org_id = ? AND scope = ? AND (repo_id = ? OR (repo_id IS NULL AND ? IS NULL))
			  AND (weaver_id = ? OR (weaver_id IS NULL AND ? IS NULL))
			  AND name = ? AND deleted_at IS NULL
			"#,
		)
		.bind(org_id)
		.bind(scope)
		.bind(repo_id)
		.bind(repo_id)
		.bind(weaver_id)
		.bind(weaver_id)
		.bind(name)
		.fetch_optional(&self.pool)
		.await?;

		match row {
			Some(row) => Ok(Some(parse_secret_row(&row)?)),
			None => Ok(None),
		}
	}

	/// List secrets matching a filter.
	#[tracing::instrument(skip(self, filter))]
	pub async fn list_secrets(&self, filter: &SecretFilterParams) -> Result<Vec<SecretRow>> {
		let mut query = String::from(
			r#"
			SELECT id, org_id, scope, repo_id, weaver_id, name, description, current_version, created_by, created_at, updated_at
			FROM secrets
			WHERE deleted_at IS NULL
			"#,
		);

		if filter.org_id.is_some() {
			query.push_str(" AND org_id = ?");
		}
		if filter.scope.is_some() {
			query.push_str(" AND scope = ?");
		}
		if filter.repo_id.is_some() {
			query.push_str(" AND repo_id = ?");
		}
		if filter.weaver_id.is_some() {
			query.push_str(" AND weaver_id = ?");
		}
		if filter.name.is_some() {
			query.push_str(" AND name = ?");
		}

		query.push_str(" ORDER BY name ASC");

		let mut q = sqlx::query(&query);

		if let Some(ref org_id) = filter.org_id {
			q = q.bind(org_id);
		}
		if let Some(ref scope) = filter.scope {
			q = q.bind(scope);
		}
		if let Some(ref repo_id) = filter.repo_id {
			q = q.bind(repo_id);
		}
		if let Some(ref weaver_id) = filter.weaver_id {
			q = q.bind(weaver_id);
		}
		if let Some(ref name) = filter.name {
			q = q.bind(name);
		}

		let rows = q.fetch_all(&self.pool).await?;

		let mut secrets = Vec::with_capacity(rows.len());
		for row in rows {
			secrets.push(parse_secret_row(&row)?);
		}
		Ok(secrets)
	}

	/// Get the current version of a secret.
	#[tracing::instrument(skip(self), fields(secret_id = %secret_id))]
	pub async fn get_current_version(&self, secret_id: &str) -> Result<Option<SecretVersionRow>> {
		let row = sqlx::query(
			r#"
			SELECT v.id, v.secret_id, v.version, v.ciphertext, v.nonce, v.dek_id, v.created_by, v.created_at, v.expires_at, v.disabled_at
			FROM secret_versions v
			JOIN secrets s ON s.id = v.secret_id AND s.current_version = v.version
			WHERE v.secret_id = ? AND v.disabled_at IS NULL AND s.deleted_at IS NULL
			"#,
		)
		.bind(secret_id)
		.fetch_optional(&self.pool)
		.await?;

		match row {
			Some(row) => Ok(Some(parse_version_row(&row)?)),
			None => Ok(None),
		}
	}

	/// Get a specific version of a secret.
	#[tracing::instrument(skip(self), fields(secret_id = %secret_id, version = %version))]
	pub async fn get_version(
		&self,
		secret_id: &str,
		version: i32,
	) -> Result<Option<SecretVersionRow>> {
		let row = sqlx::query(
			r#"
			SELECT v.id, v.secret_id, v.version, v.ciphertext, v.nonce, v.dek_id, v.created_by, v.created_at, v.expires_at, v.disabled_at
			FROM secret_versions v
			JOIN secrets s ON s.id = v.secret_id
			WHERE v.secret_id = ? AND v.version = ? AND s.deleted_at IS NULL
			"#,
		)
		.bind(secret_id)
		.bind(version)
		.fetch_optional(&self.pool)
		.await?;

		match row {
			Some(row) => Ok(Some(parse_version_row(&row)?)),
			None => Ok(None),
		}
	}

	/// Disable a secret version (revocation).
	#[tracing::instrument(skip(self), fields(version_id = %version_id))]
	pub async fn disable_version(&self, version_id: &str) -> Result<()> {
		let now_str = Utc::now().to_rfc3339();
		sqlx::query("UPDATE secret_versions SET disabled_at = ? WHERE id = ?")
			.bind(&now_str)
			.bind(version_id)
			.execute(&self.pool)
			.await?;

		tracing::debug!(version_id = %version_id, "secret version disabled");
		Ok(())
	}

	/// Soft delete a secret.
	#[tracing::instrument(skip(self), fields(secret_id = %id))]
	pub async fn delete_secret(&self, id: &str) -> Result<()> {
		let now_str = Utc::now().to_rfc3339();
		sqlx::query("UPDATE secrets SET deleted_at = ? WHERE id = ?")
			.bind(&now_str)
			.bind(id)
			.execute(&self.pool)
			.await?;

		tracing::debug!(secret_id = %id, "secret deleted");
		Ok(())
	}

	/// Store an encrypted DEK.
	#[tracing::instrument(skip(self, params), fields(dek_id = %params.id))]
	pub async fn store_dek(&self, params: &StoreDekParams) -> Result<()> {
		sqlx::query(
			r#"
			INSERT INTO encrypted_deks (id, encrypted_key, nonce, kek_version, created_at)
			VALUES (?, ?, ?, ?, ?)
			"#,
		)
		.bind(&params.id)
		.bind(&params.encrypted_key)
		.bind(&params.nonce)
		.bind(params.kek_version)
		.bind(&params.created_at)
		.execute(&self.pool)
		.await?;

		tracing::debug!(dek_id = %params.id, "DEK stored");
		Ok(())
	}

	/// Get an encrypted DEK by ID.
	#[tracing::instrument(skip(self), fields(dek_id = %id))]
	pub async fn get_dek(&self, id: &str) -> Result<Option<EncryptedDekRow>> {
		let row = sqlx::query_as::<_, EncryptedDekRow>(
			"SELECT id, encrypted_key, nonce, kek_version, created_at FROM encrypted_deks WHERE id = ?",
		)
		.bind(id)
		.fetch_optional(&self.pool)
		.await?;

		Ok(row)
	}

	/// Begin a new transaction.
	pub async fn begin(&self) -> Result<sqlx::Transaction<'_, sqlx::Sqlite>> {
		Ok(self.pool.begin().await?)
	}
}

#[async_trait]
impl SecretsStore for SecretsRepository {
	async fn insert_secret(&self, params: &CreateSecretParams) -> Result<()> {
		SecretsRepository::insert_secret(self, params).await
	}

	async fn insert_version(&self, params: &CreateVersionParams) -> Result<()> {
		SecretsRepository::insert_version(self, params).await
	}

	async fn get_secret(&self, id: &str) -> Result<Option<SecretRow>> {
		SecretsRepository::get_secret(self, id).await
	}

	async fn get_secret_by_name(
		&self,
		org_id: &str,
		scope: &str,
		repo_id: Option<&str>,
		weaver_id: Option<&str>,
		name: &str,
	) -> Result<Option<SecretRow>> {
		SecretsRepository::get_secret_by_name(self, org_id, scope, repo_id, weaver_id, name).await
	}

	async fn list_secrets(&self, filter: &SecretFilterParams) -> Result<Vec<SecretRow>> {
		SecretsRepository::list_secrets(self, filter).await
	}

	async fn get_current_version(&self, secret_id: &str) -> Result<Option<SecretVersionRow>> {
		SecretsRepository::get_current_version(self, secret_id).await
	}

	async fn get_version(&self, secret_id: &str, version: i32) -> Result<Option<SecretVersionRow>> {
		SecretsRepository::get_version(self, secret_id, version).await
	}

	async fn disable_version(&self, version_id: &str) -> Result<()> {
		SecretsRepository::disable_version(self, version_id).await
	}

	async fn delete_secret(&self, id: &str) -> Result<()> {
		SecretsRepository::delete_secret(self, id).await
	}

	async fn store_dek(&self, params: &StoreDekParams) -> Result<()> {
		SecretsRepository::store_dek(self, params).await
	}

	async fn get_dek(&self, id: &str) -> Result<Option<EncryptedDekRow>> {
		SecretsRepository::get_dek(self, id).await
	}
}

fn parse_secret_row(row: &sqlx::sqlite::SqliteRow) -> Result<SecretRow> {
	Ok(SecretRow {
		id: row.get("id"),
		org_id: row.get("org_id"),
		scope: row.get("scope"),
		repo_id: row.get("repo_id"),
		weaver_id: row.get("weaver_id"),
		name: row.get("name"),
		description: row.get("description"),
		current_version: row.get("current_version"),
		created_by: row.get("created_by"),
		created_at: row.get("created_at"),
		updated_at: row.get("updated_at"),
	})
}

fn parse_version_row(row: &sqlx::sqlite::SqliteRow) -> Result<SecretVersionRow> {
	Ok(SecretVersionRow {
		id: row.get("id"),
		secret_id: row.get("secret_id"),
		version: row.get("version"),
		ciphertext: row.get("ciphertext"),
		nonce: row.get("nonce"),
		dek_id: row.get("dek_id"),
		created_by: row.get("created_by"),
		created_at: row.get("created_at"),
		expires_at: row.get("expires_at"),
		disabled_at: row.get("disabled_at"),
	})
}

fn is_unique_constraint_error(e: &sqlx::Error) -> bool {
	if let sqlx::Error::Database(ref db_err) = e {
		return db_err.message().contains("UNIQUE constraint failed");
	}
	false
}

#[cfg(test)]
mod tests {
	use super::*;
	use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
	use std::str::FromStr;

	async fn create_secrets_test_pool() -> sqlx::SqlitePool {
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
			CREATE TABLE IF NOT EXISTS secrets (
				id TEXT PRIMARY KEY,
				org_id TEXT NOT NULL,
				scope TEXT NOT NULL,
				repo_id TEXT,
				weaver_id TEXT,
				name TEXT NOT NULL,
				description TEXT,
				current_version INTEGER NOT NULL DEFAULT 1,
				created_by TEXT NOT NULL,
				created_at TEXT NOT NULL,
				updated_at TEXT NOT NULL,
				deleted_at TEXT
			)
			"#,
		)
		.execute(&pool)
		.await
		.unwrap();

		sqlx::query(
			r#"
			CREATE UNIQUE INDEX IF NOT EXISTS idx_secrets_unique_name
			ON secrets(org_id, scope, COALESCE(repo_id, ''), COALESCE(weaver_id, ''), name)
			WHERE deleted_at IS NULL
			"#,
		)
		.execute(&pool)
		.await
		.unwrap();

		sqlx::query(
			r#"
			CREATE TABLE IF NOT EXISTS secret_versions (
				id TEXT PRIMARY KEY,
				secret_id TEXT NOT NULL REFERENCES secrets(id),
				version INTEGER NOT NULL,
				ciphertext BLOB NOT NULL,
				nonce BLOB NOT NULL,
				dek_id TEXT NOT NULL,
				created_by TEXT NOT NULL,
				created_at TEXT NOT NULL,
				expires_at TEXT,
				disabled_at TEXT,
				UNIQUE(secret_id, version)
			)
			"#,
		)
		.execute(&pool)
		.await
		.unwrap();

		sqlx::query(
			r#"
			CREATE TABLE IF NOT EXISTS encrypted_deks (
				id TEXT PRIMARY KEY,
				encrypted_key BLOB NOT NULL,
				nonce BLOB NOT NULL,
				kek_version INTEGER NOT NULL,
				created_at TEXT NOT NULL
			)
			"#,
		)
		.execute(&pool)
		.await
		.unwrap();

		pool
	}

	async fn make_repo() -> SecretsRepository {
		let pool = create_secrets_test_pool().await;
		SecretsRepository::new(pool)
	}

	fn make_secret_params(id: &str, name: &str) -> CreateSecretParams {
		let now = chrono::Utc::now().to_rfc3339();
		CreateSecretParams {
			id: id.to_string(),
			org_id: "org-01234567-89ab-cdef-0123-456789abcdef".to_string(),
			scope: "organization".to_string(),
			repo_id: None,
			weaver_id: None,
			name: name.to_string(),
			description: Some("Test secret description".to_string()),
			created_by: "user-01234567-89ab-cdef-0123-456789abcdef".to_string(),
			created_at: now.clone(),
			updated_at: now,
		}
	}

	#[tokio::test]
	async fn test_insert_and_get_secret() {
		let repo = make_repo().await;
		let secret_id = "secret-01234567-89ab-cdef-0123-456789abcdef";
		let params = make_secret_params(secret_id, "API_KEY");

		repo.insert_secret(&params).await.unwrap();

		let secret = repo.get_secret(secret_id).await.unwrap();
		assert!(secret.is_some());
		let secret = secret.unwrap();
		assert_eq!(secret.id, secret_id);
		assert_eq!(secret.name, "API_KEY");
		assert_eq!(secret.scope, "organization");
		assert_eq!(secret.current_version, 1);
		assert_eq!(
			secret.description,
			Some("Test secret description".to_string())
		);
	}

	#[tokio::test]
	async fn test_get_secret_not_found() {
		let repo = make_repo().await;
		let result = repo.get_secret("nonexistent-secret-id").await.unwrap();
		assert!(result.is_none());
	}

	#[tokio::test]
	async fn test_insert_duplicate_name_conflict() {
		let repo = make_repo().await;
		let params1 = make_secret_params("secret-1111", "DUPLICATE_KEY");
		let params2 = make_secret_params("secret-2222", "DUPLICATE_KEY");

		repo.insert_secret(&params1).await.unwrap();

		let result = repo.insert_secret(&params2).await;
		assert!(result.is_err());
		match result {
			Err(DbError::Conflict(msg)) => {
				assert!(msg.contains("DUPLICATE_KEY"));
			}
			other => panic!("Expected Conflict error, got: {:?}", other),
		}
	}

	#[tokio::test]
	async fn test_soft_delete_secret() {
		let repo = make_repo().await;
		let secret_id = "secret-to-delete";
		let params = make_secret_params(secret_id, "DELETABLE_SECRET");

		repo.insert_secret(&params).await.unwrap();

		let secret = repo.get_secret(secret_id).await.unwrap();
		assert!(secret.is_some());

		repo.delete_secret(secret_id).await.unwrap();

		let secret_after = repo.get_secret(secret_id).await.unwrap();
		assert!(secret_after.is_none());
	}

	#[tokio::test]
	async fn test_list_secrets_with_filter() {
		let repo = make_repo().await;
		let params1 = make_secret_params("secret-aaa", "SECRET_A");
		let mut params2 = make_secret_params("secret-bbb", "SECRET_B");
		params2.org_id = "org-different".to_string();

		repo.insert_secret(&params1).await.unwrap();
		repo.insert_secret(&params2).await.unwrap();

		let filter = SecretFilterParams {
			org_id: Some(params1.org_id.clone()),
			..Default::default()
		};
		let secrets = repo.list_secrets(&filter).await.unwrap();
		assert_eq!(secrets.len(), 1);
		assert_eq!(secrets[0].name, "SECRET_A");
	}

	#[tokio::test]
	async fn test_insert_and_get_version() {
		let repo = make_repo().await;
		let secret_id = "secret-with-version";
		let secret_params = make_secret_params(secret_id, "VERSIONED_SECRET");
		repo.insert_secret(&secret_params).await.unwrap();

		let version_params = CreateVersionParams {
			id: "version-01234567".to_string(),
			secret_id: secret_id.to_string(),
			version: 1,
			ciphertext: vec![0x01, 0x02, 0x03, 0x04],
			nonce: vec![0xaa, 0xbb, 0xcc, 0xdd],
			dek_id: "dek-01234567".to_string(),
			created_by: "user-01234567".to_string(),
			created_at: chrono::Utc::now().to_rfc3339(),
			expires_at: None,
		};
		repo.insert_version(&version_params).await.unwrap();

		let version = repo.get_current_version(secret_id).await.unwrap();
		assert!(version.is_some());
		let version = version.unwrap();
		assert_eq!(version.version, 1);
		assert_eq!(version.ciphertext, vec![0x01, 0x02, 0x03, 0x04]);
		assert_eq!(version.nonce, vec![0xaa, 0xbb, 0xcc, 0xdd]);
	}

	#[tokio::test]
	async fn test_store_and_get_dek() {
		let repo = make_repo().await;
		let dek_params = StoreDekParams {
			id: "dek-01234567-89ab-cdef".to_string(),
			encrypted_key: vec![0x10, 0x20, 0x30, 0x40, 0x50],
			nonce: vec![0x11, 0x22, 0x33],
			kek_version: 1,
			created_at: chrono::Utc::now().to_rfc3339(),
		};

		repo.store_dek(&dek_params).await.unwrap();

		let dek = repo.get_dek(&dek_params.id).await.unwrap();
		assert!(dek.is_some());
		let dek = dek.unwrap();
		assert_eq!(dek.encrypted_key, vec![0x10, 0x20, 0x30, 0x40, 0x50]);
		assert_eq!(dek.kek_version, 1);
	}
}
