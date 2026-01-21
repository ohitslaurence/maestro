// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! Core type definitions for secrets management.
//!
//! This module defines the foundational types used throughout the secrets system:
//!
//! - **ID newtypes**: Type-safe wrappers around UUIDs for secrets and versions
//! - **Scope enum**: Access scope levels for secrets (Org, Repo, Weaver)
//! - **Secret types**: Metadata structures (never contain plaintext values)
//! - **Encryption types**: Envelope encryption structures

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;
use uuid::Uuid;

use loom_server_auth::types::{OrgId, UserId};

// =============================================================================
// ID Newtypes
// =============================================================================

macro_rules! define_id_type {
	($name:ident, $doc:expr) => {
		#[doc = $doc]
		#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
		#[serde(transparent)]
		pub struct $name(Uuid);

		impl $name {
			/// Create a new ID from a UUID.
			pub fn new(id: Uuid) -> Self {
				Self(id)
			}

			/// Generate a new random ID.
			pub fn generate() -> Self {
				Self(Uuid::new_v4())
			}

			/// Get the inner UUID value.
			pub fn into_inner(self) -> Uuid {
				self.0
			}

			/// Get a reference to the inner UUID.
			pub fn as_uuid(&self) -> &Uuid {
				&self.0
			}
		}

		impl fmt::Display for $name {
			fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
				write!(f, "{}", self.0)
			}
		}

		impl From<Uuid> for $name {
			fn from(id: Uuid) -> Self {
				Self(id)
			}
		}

		impl From<$name> for Uuid {
			fn from(id: $name) -> Self {
				id.0
			}
		}
	};
}

define_id_type!(SecretId, "Unique identifier for a secret.");
define_id_type!(
	SecretVersionId,
	"Unique identifier for a specific version of a secret."
);

/// Unique identifier for a weaver instance.
///
/// Uses UUID7 for time-ordered generation, allowing efficient database indexing
/// and natural chronological ordering of weavers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct WeaverId(uuid7::Uuid);

impl WeaverId {
	/// Create a new WeaverId from a UUID7.
	pub fn new(id: uuid7::Uuid) -> Self {
		Self(id)
	}

	/// Generate a new time-ordered weaver ID.
	pub fn generate() -> Self {
		Self(uuid7::uuid7())
	}

	/// Get the inner UUID7 value.
	pub fn into_inner(self) -> uuid7::Uuid {
		self.0
	}

	/// Get a reference to the inner UUID7.
	pub fn as_uuid7(&self) -> &uuid7::Uuid {
		&self.0
	}
}

impl fmt::Display for WeaverId {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		write!(f, "{}", self.0)
	}
}

impl From<uuid7::Uuid> for WeaverId {
	fn from(id: uuid7::Uuid) -> Self {
		Self(id)
	}
}

impl From<WeaverId> for uuid7::Uuid {
	fn from(id: WeaverId) -> Self {
		id.0
	}
}

// =============================================================================
// Secret Scope
// =============================================================================

/// The scope at which a secret is accessible.
///
/// Secrets can be scoped to different levels of the hierarchy:
/// - Organization-wide secrets are available to all weavers in the org
/// - Repository-scoped secrets are only available for specific repos
/// - Weaver-scoped secrets are ephemeral and tied to a single weaver instance
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SecretScope {
	/// Secret is accessible to all weavers in the organization.
	Org { org_id: OrgId },
	/// Secret is accessible to weavers working on a specific repository.
	Repo { org_id: OrgId, repo_id: String },
	/// Secret is accessible only to a specific weaver instance (ephemeral).
	Weaver { weaver_id: WeaverId },
}

impl SecretScope {
	/// Returns the organization ID if this scope is org or repo level.
	pub fn org_id(&self) -> Option<OrgId> {
		match self {
			SecretScope::Org { org_id } => Some(*org_id),
			SecretScope::Repo { org_id, .. } => Some(*org_id),
			SecretScope::Weaver { .. } => None,
		}
	}

	/// Returns true if this is an ephemeral weaver-scoped secret.
	pub fn is_ephemeral(&self) -> bool {
		matches!(self, SecretScope::Weaver { .. })
	}

	/// Returns the scope type as a string for database storage.
	pub fn as_str(&self) -> &'static str {
		match self {
			SecretScope::Org { .. } => "org",
			SecretScope::Repo { .. } => "repo",
			SecretScope::Weaver { .. } => "weaver",
		}
	}

	/// Parse a scope type from a string.
	///
	/// # Warning
	///
	/// This function creates a scope with placeholder IDs. It is only intended
	/// for use when reconstructing a full SecretScope from database rows where
	/// the caller will provide the actual IDs separately. The returned scope
	/// should NOT be used for authorization decisions without proper ID population.
	///
	/// Returns an error for unknown scope types to prevent silent data corruption.
	///
	/// # Security
	///
	/// Use `is_placeholder()` to verify a scope has been properly populated
	/// before using it in authorization decisions.
	pub fn from_str_type(s: &str) -> Result<Self, &'static str> {
		match s {
			"org" => Ok(SecretScope::Org {
				org_id: OrgId::new(uuid::Uuid::nil()),
			}),
			"repo" => Ok(SecretScope::Repo {
				org_id: OrgId::new(uuid::Uuid::nil()),
				repo_id: String::new(),
			}),
			"weaver" => Ok(SecretScope::Weaver {
				weaver_id: WeaverId::new(uuid7::Uuid::from(0u128)),
			}),
			_ => Err("unknown scope type"),
		}
	}

	/// Check if this scope contains placeholder (nil/empty) IDs.
	///
	/// Returns `true` if this scope was created via `from_str_type` and has
	/// not been properly populated with real IDs. Such scopes should NOT be
	/// used for authorization decisions.
	///
	/// # Security
	///
	/// Always verify `!scope.is_placeholder()` before using a scope in
	/// authorization checks to prevent accidentally granting access based
	/// on nil/empty ID comparisons.
	pub fn is_placeholder(&self) -> bool {
		match self {
			SecretScope::Org { org_id } => org_id.into_inner().is_nil(),
			SecretScope::Repo { org_id, repo_id } => org_id.into_inner().is_nil() || repo_id.is_empty(),
			SecretScope::Weaver { weaver_id } => u128::from(weaver_id.into_inner()) == 0,
		}
	}
}

impl fmt::Display for SecretScope {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		match self {
			SecretScope::Org { org_id } => write!(f, "org:{org_id}"),
			SecretScope::Repo { org_id, repo_id } => write!(f, "repo:{org_id}/{repo_id}"),
			SecretScope::Weaver { weaver_id } => write!(f, "weaver:{weaver_id}"),
		}
	}
}

// =============================================================================
// Secret Metadata
// =============================================================================

/// Secret metadata without the plaintext value.
///
/// This struct contains all information about a secret except its actual value,
/// making it safe to log, return in list operations, and pass around freely.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Secret {
	/// Unique identifier for this secret.
	pub id: SecretId,
	/// Human-readable name for the secret (e.g., "GITHUB_TOKEN").
	pub name: String,
	/// Optional description of what this secret is used for.
	pub description: Option<String>,
	/// The scope at which this secret is accessible.
	pub scope: SecretScope,
	/// Current version number (incremented on each update).
	pub current_version: u32,
	/// User who created the secret.
	pub created_by: UserId,
	/// When the secret was created.
	pub created_at: DateTime<Utc>,
	/// User who last updated the secret.
	pub updated_by: UserId,
	/// When the secret was last updated.
	pub updated_at: DateTime<Utc>,
	/// Optional expiration time for the secret.
	pub expires_at: Option<DateTime<Utc>>,
}

impl Secret {
	/// Returns true if this secret has expired.
	pub fn is_expired(&self) -> bool {
		self.expires_at.map(|exp| exp < Utc::now()).unwrap_or(false)
	}
}

// =============================================================================
// Secret Version
// =============================================================================

/// Information about a specific version of a secret.
///
/// Each update to a secret creates a new version, allowing for:
/// - Audit trail of changes
/// - Rollback capabilities
/// - Grace periods during rotation
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct SecretVersion {
	/// Unique identifier for this version.
	pub id: SecretVersionId,
	/// The secret this version belongs to.
	pub secret_id: SecretId,
	/// Version number (1-indexed, sequential).
	pub version: u32,
	/// User who created this version.
	pub created_by: UserId,
	/// When this version was created.
	pub created_at: DateTime<Utc>,
	/// Whether this version is currently active.
	pub is_active: bool,
}

// =============================================================================
// Envelope Encryption Types
// =============================================================================

/// An encrypted Data Encryption Key (DEK).
///
/// In envelope encryption, each secret has its own DEK which is used to encrypt
/// the secret value. The DEK itself is encrypted with the master Key Encryption
/// Key (KEK) before storage.
///
/// This structure contains:
/// - The encrypted DEK bytes
/// - The nonce used for DEK encryption
/// - Key version for KEK rotation support
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct EncryptedDek {
	/// The encrypted DEK bytes (base64 encoded in JSON).
	#[serde(with = "base64_bytes")]
	pub ciphertext: Vec<u8>,
	/// The nonce used for encrypting this DEK.
	#[serde(with = "base64_bytes")]
	pub nonce: Vec<u8>,
	/// Version of the KEK used to encrypt this DEK.
	/// Enables key rotation by tracking which master key version was used.
	pub kek_version: u32,
}

/// Serde helper for base64 encoding/decoding byte vectors.
mod base64_bytes {
	use base64::{engine::general_purpose::STANDARD, Engine};
	use serde::{Deserialize, Deserializer, Serializer};

	pub fn serialize<S>(bytes: &[u8], serializer: S) -> Result<S::Ok, S::Error>
	where
		S: Serializer,
	{
		serializer.serialize_str(&STANDARD.encode(bytes))
	}

	pub fn deserialize<'de, D>(deserializer: D) -> Result<Vec<u8>, D::Error>
	where
		D: Deserializer<'de>,
	{
		let s = String::deserialize(deserializer)?;
		STANDARD.decode(&s).map_err(serde::de::Error::custom)
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use proptest::prelude::*;

	mod id_types {
		use super::*;

		#[test]
		fn secret_id_roundtrips() {
			let uuid = Uuid::new_v4();
			let secret_id = SecretId::new(uuid);
			assert_eq!(secret_id.into_inner(), uuid);
		}

		#[test]
		fn secret_id_generates_unique() {
			let id1 = SecretId::generate();
			let id2 = SecretId::generate();
			assert_ne!(id1, id2);
		}

		#[test]
		fn secret_id_serializes_as_uuid() {
			let uuid = Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap();
			let secret_id = SecretId::new(uuid);
			let json = serde_json::to_string(&secret_id).unwrap();
			assert_eq!(json, "\"550e8400-e29b-41d4-a716-446655440000\"");
		}

		#[test]
		fn weaver_id_generates_unique() {
			let id1 = WeaverId::generate();
			let id2 = WeaverId::generate();
			assert_ne!(id1, id2);
		}

		proptest! {
				#[test]
				fn secret_id_roundtrip_any_uuid(a: u128) {
						let uuid = Uuid::from_u128(a);
						let secret_id = SecretId::new(uuid);
						prop_assert_eq!(secret_id.into_inner(), uuid);
						prop_assert_eq!(Uuid::from(secret_id), uuid);
				}

				#[test]
				fn secret_version_id_roundtrip_any_uuid(a: u128) {
						let uuid = Uuid::from_u128(a);
						let version_id = SecretVersionId::new(uuid);
						prop_assert_eq!(version_id.into_inner(), uuid);
				}

				#[test]
				fn secret_id_serde_roundtrip(a: u128) {
						let uuid = Uuid::from_u128(a);
						let secret_id = SecretId::new(uuid);
						let json = serde_json::to_string(&secret_id).unwrap();
						let deserialized: SecretId = serde_json::from_str(&json).unwrap();
						prop_assert_eq!(secret_id, deserialized);
				}
		}
	}

	mod secret_scope {
		use super::*;

		#[test]
		fn org_scope_returns_org_id() {
			let org_id = OrgId::generate();
			let scope = SecretScope::Org { org_id };
			assert_eq!(scope.org_id(), Some(org_id));
			assert!(!scope.is_ephemeral());
		}

		#[test]
		fn repo_scope_returns_org_id() {
			let org_id = OrgId::generate();
			let scope = SecretScope::Repo {
				org_id,
				repo_id: "my-repo".to_string(),
			};
			assert_eq!(scope.org_id(), Some(org_id));
			assert!(!scope.is_ephemeral());
		}

		#[test]
		fn weaver_scope_is_ephemeral() {
			let weaver_id = WeaverId::generate();
			let scope = SecretScope::Weaver { weaver_id };
			assert_eq!(scope.org_id(), None);
			assert!(scope.is_ephemeral());
		}

		#[test]
		fn scope_serializes_with_tag() {
			let org_id = OrgId::generate();
			let scope = SecretScope::Org { org_id };
			let json = serde_json::to_string(&scope).unwrap();
			assert!(json.contains("\"type\":\"org\""), "got: {json}");
		}

		#[test]
		fn scope_display_format() {
			let org_id = OrgId::generate();
			let scope = SecretScope::Org { org_id };
			assert!(scope.to_string().starts_with("org:"));
		}
	}

	mod encrypted_dek {
		use super::*;

		#[test]
		fn encrypted_dek_serializes_base64() {
			let dek = EncryptedDek {
				ciphertext: vec![1, 2, 3, 4],
				nonce: vec![5, 6, 7, 8],
				kek_version: 1,
			};
			let json = serde_json::to_string(&dek).unwrap();
			assert!(json.contains("AQIDBA=="), "got: {json}"); // base64 of [1,2,3,4]
		}

		#[test]
		fn encrypted_dek_roundtrips() {
			let dek = EncryptedDek {
				ciphertext: vec![1, 2, 3, 4, 5],
				nonce: vec![10, 20, 30],
				kek_version: 42,
			};
			let json = serde_json::to_string(&dek).unwrap();
			let deserialized: EncryptedDek = serde_json::from_str(&json).unwrap();
			assert_eq!(dek.ciphertext, deserialized.ciphertext);
			assert_eq!(dek.nonce, deserialized.nonce);
			assert_eq!(dek.kek_version, deserialized.kek_version);
		}
	}
}
