// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! Credential storage backends.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use async_trait::async_trait;
use tokio::fs;
use tokio::io::AsyncWriteExt;
use tracing::{debug, warn};

use crate::error::CredentialError;
use crate::value::{CredentialValue, PersistedCredentialValue};

/// On-disk credential store format.
pub type PersistedCredentialStore = HashMap<String, PersistedCredentialValue>;

/// Trait for credential storage backends.
///
/// Implementations store credentials keyed by provider ID (e.g., "anthropic", "openai").
#[async_trait]
pub trait CredentialStore: Send + Sync + std::fmt::Debug {
	/// Load credentials for a provider.
	async fn load(&self, provider: &str) -> Result<Option<CredentialValue>, CredentialError>;

	/// Save credentials for a provider.
	async fn save(&self, provider: &str, creds: &CredentialValue) -> Result<(), CredentialError>;

	/// Delete credentials for a provider.
	async fn delete(&self, provider: &str) -> Result<(), CredentialError>;

	/// Check if credentials exist for a provider.
	async fn exists(&self, provider: &str) -> Result<bool, CredentialError> {
		Ok(self.load(provider).await?.is_some())
	}
}

/// File-based credential store with JSON format.
///
/// Credentials are stored in a JSON file with restricted permissions (0600 on Unix).
#[derive(Debug, Clone)]
pub struct FileCredentialStore {
	path: PathBuf,
}

impl FileCredentialStore {
	/// Create a new file credential store at the given path.
	pub fn new(path: impl Into<PathBuf>) -> Self {
		Self { path: path.into() }
	}

	/// Get the path to the credential file.
	pub fn path(&self) -> &Path {
		&self.path
	}

	/// Read the entire credential store from disk.
	pub async fn read_store(&self) -> Result<PersistedCredentialStore, CredentialError> {
		if !self.path.exists() {
			return Ok(HashMap::new());
		}

		let contents = fs::read_to_string(&self.path).await?;
		let store: PersistedCredentialStore = serde_json::from_str(&contents)?;
		Ok(store)
	}

	async fn write_store(&self, store: &PersistedCredentialStore) -> Result<(), CredentialError> {
		if let Some(parent) = self.path.parent() {
			fs::create_dir_all(parent).await?;
		}

		let contents = serde_json::to_string_pretty(store)?;

		let temp_path = self.path.with_extension("tmp");
		let mut file = fs::File::create(&temp_path).await?;
		file.write_all(contents.as_bytes()).await?;
		file.sync_all().await?;
		drop(file);

		#[cfg(unix)]
		{
			use std::os::unix::fs::PermissionsExt;
			let perms = std::fs::Permissions::from_mode(0o600);
			if let Err(e) = std::fs::set_permissions(&temp_path, perms) {
				warn!(path = ?temp_path, error = %e, "Failed to set file permissions to 0600");
			}
		}

		fs::rename(&temp_path, &self.path).await?;

		debug!(path = ?self.path, "Credential store written");
		Ok(())
	}
}

#[async_trait]
impl CredentialStore for FileCredentialStore {
	async fn load(&self, provider: &str) -> Result<Option<CredentialValue>, CredentialError> {
		let store = self.read_store().await?;
		Ok(store.get(provider).cloned().map(CredentialValue::from))
	}

	async fn save(&self, provider: &str, creds: &CredentialValue) -> Result<(), CredentialError> {
		let mut store = self.read_store().await?;
		store.insert(provider.to_string(), PersistedCredentialValue::from(creds));
		self.write_store(&store).await
	}

	async fn delete(&self, provider: &str) -> Result<(), CredentialError> {
		let mut store = self.read_store().await?;
		store.remove(provider);
		self.write_store(&store).await
	}
}

/// In-memory credential store for testing.
#[derive(Debug, Default)]
pub struct MemoryCredentialStore {
	credentials: tokio::sync::RwLock<HashMap<String, CredentialValue>>,
}

impl MemoryCredentialStore {
	/// Create a new empty in-memory store.
	pub fn new() -> Self {
		Self::default()
	}
}

#[async_trait]
impl CredentialStore for MemoryCredentialStore {
	async fn load(&self, provider: &str) -> Result<Option<CredentialValue>, CredentialError> {
		let creds = self.credentials.read().await;
		Ok(creds.get(provider).cloned())
	}

	async fn save(&self, provider: &str, creds: &CredentialValue) -> Result<(), CredentialError> {
		let mut store = self.credentials.write().await;
		store.insert(provider.to_string(), creds.clone());
		Ok(())
	}

	async fn delete(&self, provider: &str) -> Result<(), CredentialError> {
		let mut store = self.credentials.write().await;
		store.remove(provider);
		Ok(())
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use loom_common_secret::SecretString;

	#[tokio::test]
	async fn test_memory_store_roundtrip() {
		let store = MemoryCredentialStore::new();

		let creds = CredentialValue::ApiKey {
			key: SecretString::new("sk-test-key".to_string()),
		};

		store.save("anthropic", &creds).await.unwrap();

		let loaded = store.load("anthropic").await.unwrap();
		assert!(loaded.is_some());

		if let CredentialValue::ApiKey { key } = loaded.unwrap() {
			assert_eq!(key.expose(), "sk-test-key");
		} else {
			panic!("Expected ApiKey credentials");
		}
	}

	#[tokio::test]
	async fn test_memory_store_oauth() {
		let store = MemoryCredentialStore::new();

		let creds = CredentialValue::OAuth {
			refresh: SecretString::new("rt_refresh".to_string()),
			access: SecretString::new("at_access".to_string()),
			expires: 1735500000000,
		};

		store.save("anthropic", &creds).await.unwrap();

		let loaded = store.load("anthropic").await.unwrap().unwrap();
		if let CredentialValue::OAuth {
			refresh,
			access,
			expires,
		} = loaded
		{
			assert_eq!(refresh.expose(), "rt_refresh");
			assert_eq!(access.expose(), "at_access");
			assert_eq!(expires, 1735500000000);
		} else {
			panic!("Expected OAuth credentials");
		}
	}

	#[tokio::test]
	async fn test_memory_store_not_found() {
		let store = MemoryCredentialStore::new();
		let loaded = store.load("nonexistent").await.unwrap();
		assert!(loaded.is_none());
	}

	#[tokio::test]
	async fn test_memory_store_delete() {
		let store = MemoryCredentialStore::new();

		let creds = CredentialValue::ApiKey {
			key: SecretString::new("sk-test-key".to_string()),
		};

		store.save("anthropic", &creds).await.unwrap();
		assert!(store.exists("anthropic").await.unwrap());

		store.delete("anthropic").await.unwrap();
		assert!(!store.exists("anthropic").await.unwrap());
	}

	#[tokio::test]
	async fn test_file_store_roundtrip() {
		let temp_dir = tempfile::tempdir().unwrap();
		let path = temp_dir.path().join("credentials.json");
		let store = FileCredentialStore::new(&path);

		let creds = CredentialValue::ApiKey {
			key: SecretString::new("sk-file-test".to_string()),
		};

		store.save("anthropic", &creds).await.unwrap();
		assert!(path.exists());

		let loaded = store.load("anthropic").await.unwrap().unwrap();
		if let CredentialValue::ApiKey { key } = loaded {
			assert_eq!(key.expose(), "sk-file-test");
		} else {
			panic!("Expected ApiKey credentials");
		}
	}

	#[tokio::test]
	async fn test_file_store_multiple_providers() {
		let temp_dir = tempfile::tempdir().unwrap();
		let path = temp_dir.path().join("credentials.json");
		let store = FileCredentialStore::new(&path);

		let anthropic_creds = CredentialValue::ApiKey {
			key: SecretString::new("sk-anthropic".to_string()),
		};
		let openai_creds = CredentialValue::ApiKey {
			key: SecretString::new("sk-openai".to_string()),
		};

		store.save("anthropic", &anthropic_creds).await.unwrap();
		store.save("openai", &openai_creds).await.unwrap();

		let loaded_anthropic = store.load("anthropic").await.unwrap().unwrap();
		let loaded_openai = store.load("openai").await.unwrap().unwrap();

		if let CredentialValue::ApiKey { key } = loaded_anthropic {
			assert_eq!(key.expose(), "sk-anthropic");
		}
		if let CredentialValue::ApiKey { key } = loaded_openai {
			assert_eq!(key.expose(), "sk-openai");
		}
	}
}
