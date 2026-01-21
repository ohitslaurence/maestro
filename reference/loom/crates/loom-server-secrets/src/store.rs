// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! Secret storage with SQLite backend.
//!
//! Secrets are stored encrypted. This module handles:
//! - Secret CRUD operations
//! - Version management
//! - DEK storage
//! - Scope-based queries

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::SqlitePool;
use tracing::{debug, instrument};
use uuid::Uuid;

use crate::error::{SecretsError, SecretsResult};
use crate::key_backend::EncryptedDekData;
use crate::types::{SecretId, SecretScope, SecretVersionId, WeaverId};
use loom_server_auth::types::{OrgId, UserId};
use loom_server_db::{
	CreateSecretParams, CreateVersionParams, SecretFilterParams, SecretRow, SecretVersionRow,
	SecretsRepository, StoreDekParams,
};

/// A stored secret with encrypted value.
#[derive(Debug, Clone)]
pub struct StoredSecret {
	pub id: SecretId,
	pub org_id: OrgId,
	pub scope: SecretScope,
	pub repo_id: Option<Uuid>,
	pub weaver_id: Option<String>,
	pub name: String,
	pub description: Option<String>,
	pub current_version: i32,
	pub created_by: UserId,
	pub created_at: DateTime<Utc>,
	pub updated_at: DateTime<Utc>,
}

/// A stored secret version with encrypted data.
#[derive(Debug, Clone)]
pub struct StoredSecretVersion {
	pub id: SecretVersionId,
	pub secret_id: SecretId,
	pub version: i32,
	pub ciphertext: Vec<u8>,
	pub nonce: Vec<u8>,
	pub dek_id: String,
	pub created_by: UserId,
	pub created_at: DateTime<Utc>,
	pub expires_at: Option<DateTime<Utc>>,
	pub disabled_at: Option<DateTime<Utc>>,
}

impl TryFrom<loom_server_db::EncryptedDekRow> for EncryptedDekData {
	type Error = SecretsError;

	fn try_from(stored: loom_server_db::EncryptedDekRow) -> Result<Self, Self::Error> {
		if stored.nonce.len() != 12 {
			return Err(SecretsError::InvalidNonce(format!(
				"expected 12 bytes, got {}",
				stored.nonce.len()
			)));
		}
		let mut nonce = [0u8; 12];
		nonce.copy_from_slice(&stored.nonce);
		Ok(Self {
			id: stored.id,
			encrypted_key: stored.encrypted_key,
			nonce,
			kek_version: stored.kek_version as u32,
		})
	}
}

/// Request to create a new secret.
#[derive(Debug, Clone)]
pub struct CreateSecretRequest {
	pub org_id: OrgId,
	pub scope: SecretScope,
	pub repo_id: Option<Uuid>,
	pub weaver_id: Option<String>,
	pub name: String,
	pub description: Option<String>,
	pub ciphertext: Vec<u8>,
	pub nonce: Vec<u8>,
	pub dek_id: String,
	pub created_by: UserId,
}

/// Request to create a new secret version.
#[derive(Debug, Clone)]
pub struct CreateVersionRequest {
	pub secret_id: SecretId,
	pub ciphertext: Vec<u8>,
	pub nonce: Vec<u8>,
	pub dek_id: String,
	pub created_by: UserId,
	pub expires_at: Option<DateTime<Utc>>,
}

/// Filter for listing secrets.
#[derive(Debug, Clone, Default)]
pub struct SecretFilter {
	pub org_id: Option<OrgId>,
	pub scope: Option<SecretScope>,
	pub repo_id: Option<Uuid>,
	pub weaver_id: Option<String>,
	pub name: Option<String>,
}

/// Trait for secret storage operations.
#[async_trait]
pub trait SecretStore: Send + Sync {
	/// Create a new secret with initial version.
	async fn create_secret(&self, request: CreateSecretRequest) -> SecretsResult<StoredSecret>;

	/// Get a secret by ID.
	async fn get_secret(&self, id: SecretId) -> SecretsResult<Option<StoredSecret>>;

	/// Get a secret by name and scope.
	async fn get_secret_by_name(
		&self,
		org_id: OrgId,
		scope: SecretScope,
		repo_id: Option<Uuid>,
		weaver_id: Option<&str>,
		name: &str,
	) -> SecretsResult<Option<StoredSecret>>;

	/// List secrets matching a filter.
	async fn list_secrets(&self, filter: &SecretFilter) -> SecretsResult<Vec<StoredSecret>>;

	/// Create a new version of a secret.
	async fn create_version(
		&self,
		request: CreateVersionRequest,
	) -> SecretsResult<StoredSecretVersion>;

	/// Get the current version of a secret.
	async fn get_current_version(
		&self,
		secret_id: SecretId,
	) -> SecretsResult<Option<StoredSecretVersion>>;

	/// Get a specific version of a secret.
	async fn get_version(
		&self,
		secret_id: SecretId,
		version: i32,
	) -> SecretsResult<Option<StoredSecretVersion>>;

	/// Disable a secret version (revocation).
	async fn disable_version(&self, version_id: SecretVersionId) -> SecretsResult<()>;

	/// Soft delete a secret.
	async fn delete_secret(&self, id: SecretId) -> SecretsResult<()>;

	/// Store an encrypted DEK.
	async fn store_dek(&self, dek: &EncryptedDekData) -> SecretsResult<()>;

	/// Get an encrypted DEK by ID.
	async fn get_dek(&self, id: &str) -> SecretsResult<Option<EncryptedDekData>>;
}

/// SQLite implementation of SecretStore.
pub struct SqliteSecretStore {
	repo: SecretsRepository,
}

impl SqliteSecretStore {
	/// Create a new SQLite secret store.
	pub fn new(pool: SqlitePool) -> Self {
		Self {
			repo: SecretsRepository::new(pool),
		}
	}
}

#[async_trait]
impl SecretStore for SqliteSecretStore {
	#[instrument(skip(self, request), fields(name = %request.name, scope = ?request.scope))]
	async fn create_secret(&self, request: CreateSecretRequest) -> SecretsResult<StoredSecret> {
		let secret_id = SecretId::generate();
		let version_id = SecretVersionId::generate();
		let now = Utc::now();
		let now_str = now.to_rfc3339();

		let scope_str = request.scope.as_str();
		let repo_id_str = request.repo_id.map(|id| id.to_string());

		let secret_params = CreateSecretParams {
			id: secret_id.to_string(),
			org_id: request.org_id.to_string(),
			scope: scope_str.to_string(),
			repo_id: repo_id_str.clone(),
			weaver_id: request.weaver_id.clone(),
			name: request.name.clone(),
			description: request.description.clone(),
			created_by: request.created_by.to_string(),
			created_at: now_str.clone(),
			updated_at: now_str.clone(),
		};

		self
			.repo
			.insert_secret(&secret_params)
			.await
			.map_err(|e| match e {
				loom_server_db::DbError::Conflict(_) => {
					SecretsError::SecretAlreadyExists(request.name.clone())
				}
				loom_server_db::DbError::Sqlx(e) => SecretsError::Database(e),
				other => SecretsError::Database(sqlx::Error::Protocol(other.to_string())),
			})?;

		let version_params = CreateVersionParams {
			id: version_id.to_string(),
			secret_id: secret_id.to_string(),
			version: 1,
			ciphertext: request.ciphertext,
			nonce: request.nonce,
			dek_id: request.dek_id,
			created_by: request.created_by.to_string(),
			created_at: now_str,
			expires_at: None,
		};

		self
			.repo
			.insert_version(&version_params)
			.await
			.map_err(|e| SecretsError::Database(sqlx::Error::Protocol(e.to_string())))?;

		debug!(secret_id = %secret_id, name = %request.name, "Created secret");

		Ok(StoredSecret {
			id: secret_id,
			org_id: request.org_id,
			scope: request.scope,
			repo_id: request.repo_id,
			weaver_id: request.weaver_id,
			name: request.name,
			description: request.description,
			current_version: 1,
			created_by: request.created_by,
			created_at: now,
			updated_at: now,
		})
	}

	async fn get_secret(&self, id: SecretId) -> SecretsResult<Option<StoredSecret>> {
		let row = self
			.repo
			.get_secret(&id.to_string())
			.await
			.map_err(|e| SecretsError::Database(sqlx::Error::Protocol(e.to_string())))?;

		match row {
			Some(row) => Ok(Some(parse_secret_row(&row)?)),
			None => Ok(None),
		}
	}

	async fn get_secret_by_name(
		&self,
		org_id: OrgId,
		scope: SecretScope,
		repo_id: Option<Uuid>,
		weaver_id: Option<&str>,
		name: &str,
	) -> SecretsResult<Option<StoredSecret>> {
		let scope_str = scope.as_str();
		let repo_id_str = repo_id.map(|id| id.to_string());

		let row = self
			.repo
			.get_secret_by_name(
				&org_id.to_string(),
				scope_str,
				repo_id_str.as_deref(),
				weaver_id,
				name,
			)
			.await
			.map_err(|e| SecretsError::Database(sqlx::Error::Protocol(e.to_string())))?;

		match row {
			Some(row) => Ok(Some(parse_secret_row(&row)?)),
			None => Ok(None),
		}
	}

	async fn list_secrets(&self, filter: &SecretFilter) -> SecretsResult<Vec<StoredSecret>> {
		let filter_params = SecretFilterParams {
			org_id: filter.org_id.map(|id| id.to_string()),
			scope: filter.scope.as_ref().map(|s| s.as_str().to_string()),
			repo_id: filter.repo_id.map(|id| id.to_string()),
			weaver_id: filter.weaver_id.clone(),
			name: filter.name.clone(),
		};

		let rows = self
			.repo
			.list_secrets(&filter_params)
			.await
			.map_err(|e| SecretsError::Database(sqlx::Error::Protocol(e.to_string())))?;

		rows.iter().map(parse_secret_row).collect()
	}

	async fn create_version(
		&self,
		request: CreateVersionRequest,
	) -> SecretsResult<StoredSecretVersion> {
		let version_id = SecretVersionId::generate();
		let now = Utc::now();
		let now_str = now.to_rfc3339();
		let expires_at_str = request.expires_at.map(|dt| dt.to_rfc3339());

		let mut tx = self
			.repo
			.begin()
			.await
			.map_err(|e| SecretsError::Database(sqlx::Error::Protocol(e.to_string())))?;

		let next_version = self
			.repo
			.get_next_version_number(&mut tx, &request.secret_id.to_string())
			.await
			.map_err(|e| SecretsError::Database(sqlx::Error::Protocol(e.to_string())))?;

		let version_params = CreateVersionParams {
			id: version_id.to_string(),
			secret_id: request.secret_id.to_string(),
			version: next_version,
			ciphertext: request.ciphertext.clone(),
			nonce: request.nonce.clone(),
			dek_id: request.dek_id.clone(),
			created_by: request.created_by.to_string(),
			created_at: now_str.clone(),
			expires_at: expires_at_str,
		};

		self
			.repo
			.insert_version_in_tx(&mut tx, &version_params)
			.await
			.map_err(|e| SecretsError::Database(sqlx::Error::Protocol(e.to_string())))?;

		let rows_affected = self
			.repo
			.update_current_version_in_tx(
				&mut tx,
				&request.secret_id.to_string(),
				next_version,
				&now_str,
			)
			.await
			.map_err(|e| SecretsError::Database(sqlx::Error::Protocol(e.to_string())))?;

		if rows_affected == 0 {
			return Err(SecretsError::SecretNotFoundById(request.secret_id));
		}

		tx.commit()
			.await
			.map_err(|e| SecretsError::Database(sqlx::Error::Protocol(e.to_string())))?;

		debug!(secret_id = %request.secret_id, version = next_version, "Created secret version");

		Ok(StoredSecretVersion {
			id: version_id,
			secret_id: request.secret_id,
			version: next_version,
			ciphertext: request.ciphertext,
			nonce: request.nonce,
			dek_id: request.dek_id,
			created_by: request.created_by,
			created_at: now,
			expires_at: request.expires_at,
			disabled_at: None,
		})
	}

	async fn get_current_version(
		&self,
		secret_id: SecretId,
	) -> SecretsResult<Option<StoredSecretVersion>> {
		let row = self
			.repo
			.get_current_version(&secret_id.to_string())
			.await
			.map_err(|e| SecretsError::Database(sqlx::Error::Protocol(e.to_string())))?;

		match row {
			Some(row) => Ok(Some(parse_version_row(&row)?)),
			None => Ok(None),
		}
	}

	async fn get_version(
		&self,
		secret_id: SecretId,
		version: i32,
	) -> SecretsResult<Option<StoredSecretVersion>> {
		let row = self
			.repo
			.get_version(&secret_id.to_string(), version)
			.await
			.map_err(|e| SecretsError::Database(sqlx::Error::Protocol(e.to_string())))?;

		match row {
			Some(row) => Ok(Some(parse_version_row(&row)?)),
			None => Ok(None),
		}
	}

	async fn disable_version(&self, version_id: SecretVersionId) -> SecretsResult<()> {
		self
			.repo
			.disable_version(&version_id.to_string())
			.await
			.map_err(|e| SecretsError::Database(sqlx::Error::Protocol(e.to_string())))?;

		Ok(())
	}

	async fn delete_secret(&self, id: SecretId) -> SecretsResult<()> {
		self
			.repo
			.delete_secret(&id.to_string())
			.await
			.map_err(|e| SecretsError::Database(sqlx::Error::Protocol(e.to_string())))?;

		Ok(())
	}

	async fn store_dek(&self, dek: &EncryptedDekData) -> SecretsResult<()> {
		let now_str = Utc::now().to_rfc3339();
		let params = StoreDekParams {
			id: dek.id.clone(),
			encrypted_key: dek.encrypted_key.clone(),
			nonce: dek.nonce.to_vec(),
			kek_version: dek.kek_version as i32,
			created_at: now_str,
		};

		self
			.repo
			.store_dek(&params)
			.await
			.map_err(|e| SecretsError::Database(sqlx::Error::Protocol(e.to_string())))?;

		Ok(())
	}

	async fn get_dek(&self, id: &str) -> SecretsResult<Option<EncryptedDekData>> {
		let row = self
			.repo
			.get_dek(id)
			.await
			.map_err(|e| SecretsError::Database(sqlx::Error::Protocol(e.to_string())))?;

		match row {
			Some(stored) => Ok(Some(stored.try_into()?)),
			None => Ok(None),
		}
	}
}

fn parse_secret_row(row: &SecretRow) -> SecretsResult<StoredSecret> {
	let parsed_org_id = OrgId::new(
		Uuid::parse_str(&row.org_id)
			.map_err(|_| SecretsError::CorruptedData(format!("invalid org id: {}", row.org_id)))?,
	);

	let parsed_repo_id = row
		.repo_id
		.as_ref()
		.map(|s| {
			Uuid::parse_str(s).map_err(|_| SecretsError::CorruptedData(format!("invalid repo id: {}", s)))
		})
		.transpose()?;

	let parsed_scope = match row.scope.as_str() {
		"org" => SecretScope::Org {
			org_id: parsed_org_id,
		},
		"repo" => {
			let repo_id_str = row
				.repo_id
				.as_ref()
				.ok_or_else(|| SecretsError::CorruptedData("repo scope requires repo_id".into()))?;
			SecretScope::Repo {
				org_id: parsed_org_id,
				repo_id: repo_id_str.clone(),
			}
		}
		"weaver" => {
			let weaver_id_str = row
				.weaver_id
				.as_ref()
				.ok_or_else(|| SecretsError::CorruptedData("weaver scope requires weaver_id".into()))?;
			let wid = weaver_id_str.parse::<uuid7::Uuid>().map_err(|_| {
				SecretsError::CorruptedData(format!("invalid weaver_id: {}", weaver_id_str))
			})?;
			SecretScope::Weaver {
				weaver_id: WeaverId::new(wid),
			}
		}
		other => {
			return Err(SecretsError::CorruptedData(format!(
				"unknown scope type: {}",
				other
			)));
		}
	};

	Ok(StoredSecret {
		id: SecretId::new(
			Uuid::parse_str(&row.id)
				.map_err(|_| SecretsError::CorruptedData(format!("invalid secret id: {}", row.id)))?,
		),
		org_id: parsed_org_id,
		scope: parsed_scope,
		repo_id: parsed_repo_id,
		weaver_id: row.weaver_id.clone(),
		name: row.name.clone(),
		description: row.description.clone(),
		current_version: row.current_version,
		created_by: UserId::new(Uuid::parse_str(&row.created_by).map_err(|_| {
			SecretsError::CorruptedData(format!("invalid created_by id: {}", row.created_by))
		})?),
		created_at: DateTime::parse_from_rfc3339(&row.created_at)
			.map(|dt| dt.with_timezone(&Utc))
			.map_err(|_| {
				SecretsError::CorruptedData(format!("invalid created_at timestamp: {}", row.created_at))
			})?,
		updated_at: DateTime::parse_from_rfc3339(&row.updated_at)
			.map(|dt| dt.with_timezone(&Utc))
			.map_err(|_| {
				SecretsError::CorruptedData(format!("invalid updated_at timestamp: {}", row.updated_at))
			})?,
	})
}

fn parse_version_row(row: &SecretVersionRow) -> SecretsResult<StoredSecretVersion> {
	Ok(StoredSecretVersion {
		id: SecretVersionId::new(
			Uuid::parse_str(&row.id)
				.map_err(|_| SecretsError::CorruptedData(format!("invalid version id: {}", row.id)))?,
		),
		secret_id: SecretId::new(
			Uuid::parse_str(&row.secret_id).map_err(|_| {
				SecretsError::CorruptedData(format!("invalid secret id: {}", row.secret_id))
			})?,
		),
		version: row.version,
		ciphertext: row.ciphertext.clone(),
		nonce: row.nonce.clone(),
		dek_id: row.dek_id.clone(),
		created_by: UserId::new(Uuid::parse_str(&row.created_by).map_err(|_| {
			SecretsError::CorruptedData(format!("invalid created_by id: {}", row.created_by))
		})?),
		created_at: DateTime::parse_from_rfc3339(&row.created_at)
			.map(|dt| dt.with_timezone(&Utc))
			.map_err(|_| {
				SecretsError::CorruptedData(format!("invalid created_at timestamp: {}", row.created_at))
			})?,
		expires_at: row
			.expires_at
			.as_ref()
			.map(|s| {
				DateTime::parse_from_rfc3339(s)
					.map(|dt| dt.with_timezone(&Utc))
					.map_err(|_| SecretsError::CorruptedData(format!("invalid expires_at timestamp: {}", s)))
			})
			.transpose()?,
		disabled_at: row
			.disabled_at
			.as_ref()
			.map(|s| {
				DateTime::parse_from_rfc3339(s)
					.map(|dt| dt.with_timezone(&Utc))
					.map_err(|_| SecretsError::CorruptedData(format!("invalid disabled_at timestamp: {}", s)))
			})
			.transpose()?,
	})
}

#[cfg(test)]
mod tests {
	use super::*;
	use loom_server_auth::types::OrgId;

	async fn create_test_pool() -> SqlitePool {
		let pool = SqlitePool::connect(":memory:").await.unwrap();
		run_test_migrations(&pool).await.unwrap();
		pool
	}

	async fn run_test_migrations(pool: &SqlitePool) -> Result<(), sqlx::Error> {
		let migration = include_str!("../../loom-server/migrations/025_weaver_secrets.sql");
		for stmt in migration.split(';').filter(|s| !s.trim().is_empty()) {
			let trimmed = stmt.trim();
			if !trimmed.is_empty() {
				sqlx::query(trimmed).execute(pool).await?;
			}
		}
		Ok(())
	}

	fn test_user_id() -> UserId {
		UserId::new(Uuid::new_v4())
	}

	fn test_org_id() -> OrgId {
		OrgId::new(Uuid::new_v4())
	}

	#[tokio::test]
	async fn test_create_secret() {
		let pool = create_test_pool().await;
		let store = SqliteSecretStore::new(pool);

		let org_id = test_org_id();
		let user_id = test_user_id();
		let dek_id = Uuid::new_v4().to_string();

		store
			.store_dek(&EncryptedDekData {
				id: dek_id.clone(),
				encrypted_key: vec![0u8; 32],
				nonce: [1u8; 12],
				kek_version: 1,
			})
			.await
			.unwrap();

		let request = CreateSecretRequest {
			org_id,
			scope: SecretScope::Org { org_id },
			repo_id: None,
			weaver_id: None,
			name: "TEST_SECRET".to_string(),
			description: Some("A test secret".to_string()),
			ciphertext: vec![0u8; 16],
			nonce: vec![0u8; 12],
			dek_id: dek_id.clone(),
			created_by: user_id,
		};

		let secret = store.create_secret(request).await.unwrap();
		assert_eq!(secret.name, "TEST_SECRET");
		assert_eq!(secret.current_version, 1);
		assert_eq!(secret.org_id, org_id);
	}

	#[tokio::test]
	async fn test_get_secret_by_id() {
		let pool = create_test_pool().await;
		let store = SqliteSecretStore::new(pool);

		let org_id = test_org_id();
		let user_id = test_user_id();
		let dek_id = Uuid::new_v4().to_string();

		store
			.store_dek(&EncryptedDekData {
				id: dek_id.clone(),
				encrypted_key: vec![0u8; 32],
				nonce: [1u8; 12],
				kek_version: 1,
			})
			.await
			.unwrap();

		let request = CreateSecretRequest {
			org_id,
			scope: SecretScope::Org { org_id },
			repo_id: None,
			weaver_id: None,
			name: "GET_TEST".to_string(),
			description: None,
			ciphertext: vec![0u8; 16],
			nonce: vec![0u8; 12],
			dek_id,
			created_by: user_id,
		};

		let created = store.create_secret(request).await.unwrap();
		let fetched = store.get_secret(created.id).await.unwrap();

		assert!(fetched.is_some());
		let fetched = fetched.unwrap();
		assert_eq!(fetched.id, created.id);
		assert_eq!(fetched.name, "GET_TEST");
	}

	#[tokio::test]
	async fn test_get_secret_by_name() {
		let pool = create_test_pool().await;
		let store = SqliteSecretStore::new(pool);

		let org_id = test_org_id();
		let user_id = test_user_id();
		let dek_id = Uuid::new_v4().to_string();

		store
			.store_dek(&EncryptedDekData {
				id: dek_id.clone(),
				encrypted_key: vec![0u8; 32],
				nonce: [1u8; 12],
				kek_version: 1,
			})
			.await
			.unwrap();

		let scope = SecretScope::Org { org_id };
		let request = CreateSecretRequest {
			org_id,
			scope: scope.clone(),
			repo_id: None,
			weaver_id: None,
			name: "API_KEY".to_string(),
			description: None,
			ciphertext: vec![0u8; 16],
			nonce: vec![0u8; 12],
			dek_id,
			created_by: user_id,
		};

		let created = store.create_secret(request).await.unwrap();
		let fetched = store
			.get_secret_by_name(org_id, scope, None, None, "API_KEY")
			.await
			.unwrap();

		assert!(fetched.is_some());
		assert_eq!(fetched.unwrap().id, created.id);
	}

	#[tokio::test]
	async fn test_list_secrets() {
		let pool = create_test_pool().await;
		let store = SqliteSecretStore::new(pool);

		let org_id = test_org_id();
		let user_id = test_user_id();

		for i in 0..3 {
			let dek_id = Uuid::new_v4().to_string();
			store
				.store_dek(&EncryptedDekData {
					id: dek_id.clone(),
					encrypted_key: vec![0u8; 32],
					nonce: [1u8; 12],
					kek_version: 1,
				})
				.await
				.unwrap();

			let request = CreateSecretRequest {
				org_id,
				scope: SecretScope::Org { org_id },
				repo_id: None,
				weaver_id: None,
				name: format!("SECRET_{}", i),
				description: None,
				ciphertext: vec![0u8; 16],
				nonce: vec![0u8; 12],
				dek_id,
				created_by: user_id,
			};
			store.create_secret(request).await.unwrap();
		}

		let filter = SecretFilter {
			org_id: Some(org_id),
			..Default::default()
		};
		let secrets = store.list_secrets(&filter).await.unwrap();
		assert_eq!(secrets.len(), 3);
	}

	#[tokio::test]
	async fn test_create_version() {
		let pool = create_test_pool().await;
		let store = SqliteSecretStore::new(pool);

		let org_id = test_org_id();
		let user_id = test_user_id();
		let dek_id = Uuid::new_v4().to_string();

		store
			.store_dek(&EncryptedDekData {
				id: dek_id.clone(),
				encrypted_key: vec![0u8; 32],
				nonce: [1u8; 12],
				kek_version: 1,
			})
			.await
			.unwrap();

		let request = CreateSecretRequest {
			org_id,
			scope: SecretScope::Org { org_id },
			repo_id: None,
			weaver_id: None,
			name: "VERSIONED_SECRET".to_string(),
			description: None,
			ciphertext: vec![0u8; 16],
			nonce: vec![0u8; 12],
			dek_id: dek_id.clone(),
			created_by: user_id,
		};

		let secret = store.create_secret(request).await.unwrap();
		assert_eq!(secret.current_version, 1);

		let version_request = CreateVersionRequest {
			secret_id: secret.id,
			ciphertext: vec![1u8; 16],
			nonce: vec![1u8; 12],
			dek_id: dek_id.clone(),
			created_by: user_id,
			expires_at: None,
		};

		let version = store.create_version(version_request).await.unwrap();
		assert_eq!(version.version, 2);

		let updated = store.get_secret(secret.id).await.unwrap().unwrap();
		assert_eq!(updated.current_version, 2);
	}

	#[tokio::test]
	async fn test_get_current_version() {
		let pool = create_test_pool().await;
		let store = SqliteSecretStore::new(pool);

		let org_id = test_org_id();
		let user_id = test_user_id();
		let dek_id = Uuid::new_v4().to_string();

		store
			.store_dek(&EncryptedDekData {
				id: dek_id.clone(),
				encrypted_key: vec![0u8; 32],
				nonce: [1u8; 12],
				kek_version: 1,
			})
			.await
			.unwrap();

		let request = CreateSecretRequest {
			org_id,
			scope: SecretScope::Org { org_id },
			repo_id: None,
			weaver_id: None,
			name: "CURRENT_VERSION_TEST".to_string(),
			description: None,
			ciphertext: vec![0u8; 16],
			nonce: vec![0u8; 12],
			dek_id,
			created_by: user_id,
		};

		let secret = store.create_secret(request).await.unwrap();
		let current = store.get_current_version(secret.id).await.unwrap();

		assert!(current.is_some());
		let current = current.unwrap();
		assert_eq!(current.version, 1);
		assert_eq!(current.ciphertext, vec![0u8; 16]);
	}

	#[tokio::test]
	async fn test_get_specific_version() {
		let pool = create_test_pool().await;
		let store = SqliteSecretStore::new(pool);

		let org_id = test_org_id();
		let user_id = test_user_id();
		let dek_id = Uuid::new_v4().to_string();

		store
			.store_dek(&EncryptedDekData {
				id: dek_id.clone(),
				encrypted_key: vec![0u8; 32],
				nonce: [1u8; 12],
				kek_version: 1,
			})
			.await
			.unwrap();

		let request = CreateSecretRequest {
			org_id,
			scope: SecretScope::Org { org_id },
			repo_id: None,
			weaver_id: None,
			name: "SPECIFIC_VERSION_TEST".to_string(),
			description: None,
			ciphertext: vec![1u8; 16],
			nonce: vec![0u8; 12],
			dek_id: dek_id.clone(),
			created_by: user_id,
		};

		let secret = store.create_secret(request).await.unwrap();

		store
			.create_version(CreateVersionRequest {
				secret_id: secret.id,
				ciphertext: vec![2u8; 16],
				nonce: vec![1u8; 12],
				dek_id: dek_id.clone(),
				created_by: user_id,
				expires_at: None,
			})
			.await
			.unwrap();

		let v1 = store.get_version(secret.id, 1).await.unwrap().unwrap();
		let v2 = store.get_version(secret.id, 2).await.unwrap().unwrap();

		assert_eq!(v1.ciphertext, vec![1u8; 16]);
		assert_eq!(v2.ciphertext, vec![2u8; 16]);
	}

	#[tokio::test]
	async fn test_disable_version() {
		let pool = create_test_pool().await;
		let store = SqliteSecretStore::new(pool);

		let org_id = test_org_id();
		let user_id = test_user_id();
		let dek_id = Uuid::new_v4().to_string();

		store
			.store_dek(&EncryptedDekData {
				id: dek_id.clone(),
				encrypted_key: vec![0u8; 32],
				nonce: [1u8; 12],
				kek_version: 1,
			})
			.await
			.unwrap();

		let request = CreateSecretRequest {
			org_id,
			scope: SecretScope::Org { org_id },
			repo_id: None,
			weaver_id: None,
			name: "DISABLE_TEST".to_string(),
			description: None,
			ciphertext: vec![0u8; 16],
			nonce: vec![0u8; 12],
			dek_id,
			created_by: user_id,
		};

		let secret = store.create_secret(request).await.unwrap();
		let version = store.get_current_version(secret.id).await.unwrap().unwrap();

		store.disable_version(version.id).await.unwrap();

		let current = store.get_current_version(secret.id).await.unwrap();
		assert!(current.is_none());
	}

	#[tokio::test]
	async fn test_delete_secret() {
		let pool = create_test_pool().await;
		let store = SqliteSecretStore::new(pool);

		let org_id = test_org_id();
		let user_id = test_user_id();
		let dek_id = Uuid::new_v4().to_string();

		store
			.store_dek(&EncryptedDekData {
				id: dek_id.clone(),
				encrypted_key: vec![0u8; 32],
				nonce: [1u8; 12],
				kek_version: 1,
			})
			.await
			.unwrap();

		let request = CreateSecretRequest {
			org_id,
			scope: SecretScope::Org { org_id },
			repo_id: None,
			weaver_id: None,
			name: "DELETE_TEST".to_string(),
			description: None,
			ciphertext: vec![0u8; 16],
			nonce: vec![0u8; 12],
			dek_id,
			created_by: user_id,
		};

		let secret = store.create_secret(request).await.unwrap();
		store.delete_secret(secret.id).await.unwrap();

		let fetched = store.get_secret(secret.id).await.unwrap();
		assert!(fetched.is_none());
	}

	#[tokio::test]
	async fn test_store_and_get_dek() {
		let pool = create_test_pool().await;
		let store = SqliteSecretStore::new(pool);

		let dek = EncryptedDekData {
			id: Uuid::new_v4().to_string(),
			encrypted_key: vec![
				1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25,
				26, 27, 28, 29, 30, 31, 32,
			],
			nonce: [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12],
			kek_version: 1,
		};

		store.store_dek(&dek).await.unwrap();

		let fetched = store.get_dek(&dek.id).await.unwrap();
		assert!(fetched.is_some());
		let fetched = fetched.unwrap();
		assert_eq!(fetched.id, dek.id);
		assert_eq!(fetched.encrypted_key, dek.encrypted_key);
		assert_eq!(fetched.nonce, dek.nonce);
		assert_eq!(fetched.kek_version, dek.kek_version);
	}

	#[tokio::test]
	async fn test_secret_id_uniqueness() {
		let pool = create_test_pool().await;
		let store = SqliteSecretStore::new(pool);

		let org_id = test_org_id();
		let user_id = test_user_id();
		let dek_id = Uuid::new_v4().to_string();

		store
			.store_dek(&EncryptedDekData {
				id: dek_id.clone(),
				encrypted_key: vec![0u8; 32],
				nonce: [1u8; 12],
				kek_version: 1,
			})
			.await
			.unwrap();

		let secret1 = store
			.create_secret(CreateSecretRequest {
				org_id,
				scope: SecretScope::Org { org_id },
				repo_id: None,
				weaver_id: None,
				name: "SECRET_A".to_string(),
				description: None,
				ciphertext: vec![0u8; 16],
				nonce: vec![0u8; 12],
				dek_id: dek_id.clone(),
				created_by: user_id,
			})
			.await
			.unwrap();

		let secret2 = store
			.create_secret(CreateSecretRequest {
				org_id,
				scope: SecretScope::Org { org_id },
				repo_id: None,
				weaver_id: None,
				name: "SECRET_B".to_string(),
				description: None,
				ciphertext: vec![0u8; 16],
				nonce: vec![0u8; 12],
				dek_id: dek_id.clone(),
				created_by: user_id,
			})
			.await
			.unwrap();

		assert_ne!(secret1.id, secret2.id);
	}

	#[tokio::test]
	async fn test_repo_scoped_secret() {
		let pool = create_test_pool().await;
		let store = SqliteSecretStore::new(pool);

		let org_id = test_org_id();
		let user_id = test_user_id();
		let repo_id = Uuid::new_v4();
		let dek_id = Uuid::new_v4().to_string();

		store
			.store_dek(&EncryptedDekData {
				id: dek_id.clone(),
				encrypted_key: vec![0u8; 32],
				nonce: [1u8; 12],
				kek_version: 1,
			})
			.await
			.unwrap();

		let scope = SecretScope::Repo {
			org_id,
			repo_id: repo_id.to_string(),
		};

		let request = CreateSecretRequest {
			org_id,
			scope: scope.clone(),
			repo_id: Some(repo_id),
			weaver_id: None,
			name: "REPO_SECRET".to_string(),
			description: None,
			ciphertext: vec![0u8; 16],
			nonce: vec![0u8; 12],
			dek_id,
			created_by: user_id,
		};

		let secret = store.create_secret(request).await.unwrap();
		assert_eq!(secret.repo_id, Some(repo_id));

		let filter = SecretFilter {
			org_id: Some(org_id),
			repo_id: Some(repo_id),
			..Default::default()
		};
		let secrets = store.list_secrets(&filter).await.unwrap();
		assert_eq!(secrets.len(), 1);
		assert_eq!(secrets[0].name, "REPO_SECRET");
	}

	#[tokio::test]
	async fn test_weaver_scoped_secret() {
		let pool = create_test_pool().await;
		let store = SqliteSecretStore::new(pool);

		let org_id = test_org_id();
		let user_id = test_user_id();
		let weaver_id = WeaverId::generate();
		let dek_id = Uuid::new_v4().to_string();

		store
			.store_dek(&EncryptedDekData {
				id: dek_id.clone(),
				encrypted_key: vec![0u8; 32],
				nonce: [1u8; 12],
				kek_version: 1,
			})
			.await
			.unwrap();

		let scope = SecretScope::Weaver { weaver_id };

		let request = CreateSecretRequest {
			org_id,
			scope,
			repo_id: None,
			weaver_id: Some(weaver_id.to_string()),
			name: "WEAVER_SECRET".to_string(),
			description: None,
			ciphertext: vec![0u8; 16],
			nonce: vec![0u8; 12],
			dek_id,
			created_by: user_id,
		};

		let secret = store.create_secret(request).await.unwrap();
		assert_eq!(secret.weaver_id, Some(weaver_id.to_string()));

		let filter = SecretFilter {
			weaver_id: Some(weaver_id.to_string()),
			..Default::default()
		};
		let secrets = store.list_secrets(&filter).await.unwrap();
		assert_eq!(secrets.len(), 1);
	}
}
