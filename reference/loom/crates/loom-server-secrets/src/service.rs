// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! Secrets service providing high-level secret management operations.
//!
//! This service combines:
//! - Secret storage
//! - Envelope encryption
//! - ABAC policy enforcement
//! - Audit logging

use std::sync::Arc;

use loom_common_secret::SecretString;
use loom_server_auth::types::{OrgId, UserId};
use tracing::{info, instrument, warn};
use uuid::Uuid;

use crate::encryption::{self, EncryptedData};
use crate::error::{SecretsError, SecretsResult};
use crate::key_backend::KeyBackend;
use crate::policy::{can_access_secret, WeaverPrincipal};
use crate::store::{
	CreateSecretRequest, CreateVersionRequest, SecretFilter, SecretStore, StoredSecret,
};
use crate::svid::WeaverClaims;
use crate::types::{Secret, SecretId, SecretScope, WeaverId};

/// Request to create a new secret.
#[derive(Debug)]
pub struct CreateSecretInput {
	pub org_id: OrgId,
	pub scope: SecretScope,
	pub repo_id: Option<Uuid>,
	pub weaver_id: Option<String>,
	pub name: String,
	pub value: SecretString,
	pub description: Option<String>,
	pub created_by: UserId,
}

/// Response after creating a secret.
#[derive(Debug, Clone)]
pub struct SecretMetadata {
	pub id: SecretId,
	pub org_id: OrgId,
	pub scope: SecretScope,
	pub repo_id: Option<Uuid>,
	pub weaver_id: Option<String>,
	pub name: String,
	pub description: Option<String>,
	pub current_version: i32,
	pub created_by: UserId,
}

impl From<StoredSecret> for SecretMetadata {
	fn from(s: StoredSecret) -> Self {
		Self {
			id: s.id,
			org_id: s.org_id,
			scope: s.scope,
			repo_id: s.repo_id,
			weaver_id: s.weaver_id,
			name: s.name,
			description: s.description,
			current_version: s.current_version,
			created_by: s.created_by,
		}
	}
}

/// Decrypted secret value with metadata.
#[derive(Debug)]
pub struct SecretValue {
	pub name: String,
	pub scope: SecretScope,
	pub version: i32,
	pub value: SecretString,
}

/// Secrets service for managing encrypted secrets.
///
/// # Security
///
/// This service provides the core secret management functionality but does not
/// enforce user-level authorization for administrative operations like
/// `get_secret`, `list_secrets`, `rotate_secret`, and `delete_secret`.
///
/// Callers MUST enforce authorization at the API layer before invoking these
/// methods. For weaver access, use `get_secret_for_weaver` which enforces ABAC
/// policies based on the weaver's validated claims.
pub struct SecretsService<K: KeyBackend, S: SecretStore> {
	key_backend: Arc<K>,
	store: Arc<S>,
}

impl<K: KeyBackend, S: SecretStore> SecretsService<K, S> {
	pub fn new(key_backend: Arc<K>, store: Arc<S>) -> Self {
		Self { key_backend, store }
	}

	#[instrument(skip(self, input), fields(name = %input.name, scope = ?input.scope))]
	pub async fn create_secret(&self, input: CreateSecretInput) -> SecretsResult<SecretMetadata> {
		validate_create_secret_input(&input)?;

		let dek = encryption::generate_key();
		let encrypted_value = encryption::encrypt_secret_value(&dek, input.value.expose().as_bytes())?;
		let encrypted_dek = self.key_backend.encrypt_dek(&dek).await?;

		self.store.store_dek(&encrypted_dek).await?;

		let request = CreateSecretRequest {
			org_id: input.org_id,
			scope: input.scope,
			repo_id: input.repo_id,
			weaver_id: input.weaver_id,
			name: input.name.clone(),
			description: input.description,
			ciphertext: encrypted_value.ciphertext,
			nonce: encrypted_value.nonce.to_vec(),
			dek_id: encrypted_dek.id,
			created_by: input.created_by,
		};

		let stored = self.store.create_secret(request).await?;

		info!(secret_id = %stored.id, name = %input.name, "Created secret");

		Ok(stored.into())
	}

	pub async fn get_secret(&self, id: SecretId) -> SecretsResult<Option<SecretMetadata>> {
		let stored = self.store.get_secret(id).await?;
		Ok(stored.map(|s| s.into()))
	}

	pub async fn get_secret_by_name(
		&self,
		org_id: OrgId,
		scope: SecretScope,
		repo_id: Option<Uuid>,
		weaver_id: Option<&str>,
		name: &str,
	) -> SecretsResult<Option<SecretMetadata>> {
		let stored = self
			.store
			.get_secret_by_name(org_id, scope, repo_id, weaver_id, name)
			.await?;
		Ok(stored.map(|s| s.into()))
	}

	pub async fn list_secrets(&self, filter: &SecretFilter) -> SecretsResult<Vec<SecretMetadata>> {
		let stored = self.store.list_secrets(filter).await?;
		Ok(stored.into_iter().map(|s| s.into()).collect())
	}

	#[instrument(skip(self, claims), fields(weaver_id = %claims.weaver_id, name = %name, scope = ?scope))]
	pub async fn get_secret_for_weaver(
		&self,
		claims: &WeaverClaims,
		scope: SecretScope,
		name: &str,
	) -> SecretsResult<SecretValue> {
		// Validate secret name for consistency with create path (defense in depth)
		validate_secret_name(name)?;

		let org_id = OrgId::new(
			Uuid::parse_str(&claims.org_id).map_err(|_| SecretsError::InvalidClaim("org_id".into()))?,
		);

		let repo_id = match scope {
			SecretScope::Repo { .. } => {
				let repo = claims.repo_id.as_ref().ok_or_else(|| {
					SecretsError::AccessDenied("weaver has no repo_id for repo-scoped secret".into())
				})?;
				Some(Uuid::parse_str(repo).map_err(|_| SecretsError::InvalidClaim("repo_id".into()))?)
			}
			_ => None,
		};

		let weaver_id = match scope {
			SecretScope::Weaver { .. } => Some(claims.weaver_id.as_str()),
			_ => None,
		};

		let stored = self
			.store
			.get_secret_by_name(org_id, scope.clone(), repo_id, weaver_id, name)
			.await?
			.ok_or_else(|| SecretsError::SecretNotFound(name.into()))?;

		let secret = stored_to_secret(&stored);

		let principal = build_weaver_principal(claims)?;

		if !can_access_secret(&principal, &secret) {
			warn!(
				weaver_id = %claims.weaver_id,
				secret_name = %name,
				"Access denied to secret"
			);
			return Err(SecretsError::AccessDenied(
				"insufficient permissions to access this secret".into(),
			));
		}

		let version = self
			.store
			.get_current_version(stored.id)
			.await?
			.ok_or_else(|| SecretsError::SecretNotFound(name.into()))?;

		if version.disabled_at.is_some() {
			return Err(SecretsError::SecretDisabled(name.into()));
		}

		let value = self
			.decrypt_version(&version.dek_id, &version.ciphertext, &version.nonce)
			.await?;

		info!(
			weaver_id = %claims.weaver_id,
			secret_name = %name,
			version = version.version,
			"Secret accessed by weaver"
		);

		Ok(SecretValue {
			name: stored.name,
			scope: stored.scope,
			version: version.version,
			value,
		})
	}

	#[instrument(skip(self, new_value), fields(secret_id = %secret_id))]
	pub async fn rotate_secret(
		&self,
		secret_id: SecretId,
		new_value: SecretString,
		rotated_by: UserId,
	) -> SecretsResult<i32> {
		// Verify secret exists
		let _secret = self
			.store
			.get_secret(secret_id)
			.await?
			.ok_or(SecretsError::SecretNotFoundById(secret_id))?;

		// ALWAYS generate new DEK for rotation (defense in depth)
		let new_dek = encryption::generate_key();
		let encrypted_dek = self.key_backend.encrypt_dek(&new_dek).await?;
		self.store.store_dek(&encrypted_dek).await?;

		// Encrypt new value with new DEK
		let encrypted = encryption::encrypt_secret_value(&new_dek, new_value.expose().as_bytes())?;

		// Create new version
		let request = CreateVersionRequest {
			secret_id,
			ciphertext: encrypted.ciphertext,
			nonce: encrypted.nonce.to_vec(),
			dek_id: encrypted_dek.id,
			created_by: rotated_by,
			expires_at: None,
		};

		let new_version = self.store.create_version(request).await?;

		info!(
			secret_id = %secret_id,
			new_version = new_version.version,
			"Rotated secret with new DEK"
		);

		Ok(new_version.version)
	}

	pub async fn delete_secret(&self, secret_id: SecretId, deleted_by: UserId) -> SecretsResult<()> {
		info!(secret_id = %secret_id, deleted_by = %deleted_by, "Deleting secret");
		self.store.delete_secret(secret_id).await
	}

	async fn decrypt_version(
		&self,
		dek_id: &str,
		ciphertext: &[u8],
		nonce: &[u8],
	) -> SecretsResult<SecretString> {
		let encrypted_dek = self
			.store
			.get_dek(dek_id)
			.await?
			.ok_or_else(|| SecretsError::DekNotFound(dek_id.into()))?;

		let dek = self.key_backend.decrypt_dek(&encrypted_dek).await?;

		if nonce.len() != 12 {
			return Err(SecretsError::InvalidNonce(format!(
				"expected 12-byte nonce, got {} bytes",
				nonce.len()
			)));
		}
		let mut nonce_arr = [0u8; 12];
		nonce_arr.copy_from_slice(nonce);

		let encrypted = EncryptedData {
			ciphertext: ciphertext.to_vec(),
			nonce: nonce_arr,
		};

		let plaintext = encryption::decrypt_secret_value(&dek, &encrypted).map_err(|e| {
			warn!(dek_id = %dek_id, "Failed to decrypt secret version: {e}");
			SecretsError::Decryption("failed to decrypt secret".into())
		})?;
		let value_str = String::from_utf8(plaintext.to_vec())
			.map_err(|e| SecretsError::Decryption(format!("invalid UTF-8: {e}")))?;

		Ok(SecretString::new(value_str))
	}
}

fn stored_to_secret(stored: &StoredSecret) -> Secret {
	Secret {
		id: stored.id,
		name: stored.name.clone(),
		description: stored.description.clone(),
		scope: stored.scope.clone(),
		current_version: stored.current_version as u32,
		created_by: stored.created_by,
		created_at: stored.created_at,
		updated_by: stored.created_by,
		updated_at: stored.updated_at,
		expires_at: None,
	}
}

fn build_weaver_principal(claims: &WeaverClaims) -> SecretsResult<WeaverPrincipal> {
	let weaver_id = claims
		.weaver_id
		.parse::<uuid7::Uuid>()
		.map(WeaverId::new)
		.map_err(|_| SecretsError::InvalidClaim("weaver_id".into()))?;

	let org_id = OrgId::new(
		Uuid::parse_str(&claims.org_id).map_err(|_| SecretsError::InvalidClaim("org_id".into()))?,
	);

	Ok(WeaverPrincipal {
		weaver_id,
		org_id,
		repo_id: claims.repo_id.clone(),
	})
}

fn validate_secret_name(name: &str) -> SecretsResult<()> {
	if name.is_empty() || name.len() > 128 {
		return Err(SecretsError::InvalidSecretName(
			"name must be 1-128 characters".into(),
		));
	}

	let first_char = name.chars().next().unwrap();
	if !first_char.is_ascii_uppercase() {
		return Err(SecretsError::InvalidSecretName(
			"name must start with uppercase letter".into(),
		));
	}

	if !name
		.chars()
		.all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
	{
		return Err(SecretsError::InvalidSecretName(
			"name must contain only uppercase letters, digits, and underscores".into(),
		));
	}

	Ok(())
}

const MAX_SECRET_VALUE_SIZE: usize = 64 * 1024;

fn validate_create_secret_input(input: &CreateSecretInput) -> SecretsResult<()> {
	validate_secret_name(&input.name)?;

	if input.value.expose().len() > MAX_SECRET_VALUE_SIZE {
		return Err(SecretsError::InvalidSecretName(
			"secret value too large (max 64 KiB)".into(),
		));
	}

	match input.scope {
		SecretScope::Org { .. } => {
			if input.repo_id.is_some() || input.weaver_id.is_some() {
				return Err(SecretsError::InvalidClaim(
					"org-scoped secrets must not specify repo_id or weaver_id".into(),
				));
			}
		}
		SecretScope::Repo { .. } => {
			if input.repo_id.is_none() {
				return Err(SecretsError::InvalidClaim(
					"repo-scoped secrets require repo_id".into(),
				));
			}
			if input.weaver_id.is_some() {
				return Err(SecretsError::InvalidClaim(
					"repo-scoped secrets must not specify weaver_id".into(),
				));
			}
		}
		SecretScope::Weaver { .. } => {
			let weaver_id = input.weaver_id.as_ref().ok_or_else(|| {
				SecretsError::InvalidClaim("weaver-scoped secrets require weaver_id".into())
			})?;
			weaver_id
				.parse::<uuid7::Uuid>()
				.map_err(|_| SecretsError::InvalidClaim("invalid weaver_id format".into()))?;
		}
	}

	Ok(())
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn valid_secret_names() {
		assert!(validate_secret_name("API_KEY").is_ok());
		assert!(validate_secret_name("STRIPE_API_KEY").is_ok());
		assert!(validate_secret_name("AWS_ACCESS_KEY_ID").is_ok());
		assert!(validate_secret_name("A").is_ok());
		assert!(validate_secret_name("A123").is_ok());
	}

	#[test]
	fn invalid_secret_names() {
		assert!(validate_secret_name("").is_err());
		assert!(validate_secret_name("api_key").is_err());
		assert!(validate_secret_name("123KEY").is_err());
		assert!(validate_secret_name("API-KEY").is_err());
		assert!(validate_secret_name("API KEY").is_err());
	}
}
