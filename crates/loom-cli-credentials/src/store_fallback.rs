// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use std::path::PathBuf;

use async_trait::async_trait;
use tracing::warn;

use crate::error::CredentialError;
use crate::store::{CredentialStore, FileCredentialStore};
use crate::store_keyring::KeyringCredentialStore;
use crate::value::CredentialValue;

#[derive(Debug)]
pub struct KeyringThenFileStore {
	keyring: KeyringCredentialStore,
	file: FileCredentialStore,
}

impl KeyringThenFileStore {
	pub fn new(service: impl Into<String>, file_path: impl Into<PathBuf>) -> Self {
		Self {
			keyring: KeyringCredentialStore::new(service),
			file: FileCredentialStore::new(file_path),
		}
	}
}

#[async_trait]
impl CredentialStore for KeyringThenFileStore {
	async fn load(&self, provider: &str) -> Result<Option<CredentialValue>, CredentialError> {
		match self.keyring.load(provider).await {
			Ok(Some(creds)) => {
				tracing::debug!(provider = %provider, "loaded credentials from keyring");
				return Ok(Some(creds));
			}
			Ok(None) => {
				tracing::debug!(provider = %provider, "keyring returned None, trying file store");
			}
			Err(e) => {
				warn!(provider = %provider, error = %e, "keyring load failed, trying file store");
			}
		}
		let result = self.file.load(provider).await;
		tracing::debug!(provider = %provider, found = result.as_ref().map(|r| r.is_some()).unwrap_or(false), "file store load result");
		result
	}

	async fn save(&self, provider: &str, creds: &CredentialValue) -> Result<(), CredentialError> {
		match self.keyring.save(provider, creds).await {
			Ok(()) => return Ok(()),
			Err(e) => {
				warn!(provider = %provider, error = %e, "keyring save failed, falling back to file store");
			}
		}
		self.file.save(provider, creds).await
	}

	async fn delete(&self, provider: &str) -> Result<(), CredentialError> {
		let keyring_result = self.keyring.delete(provider).await;
		let file_result = self.file.delete(provider).await;

		if keyring_result.is_err() && file_result.is_err() {
			return file_result;
		}
		Ok(())
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::store::FileCredentialStore;
	use loom_common_secret::SecretString;

	#[tokio::test]
	async fn test_file_fallback_roundtrip() {
		let temp_dir = tempfile::tempdir().unwrap();
		let path = temp_dir.path().join("credentials.json");
		let store = FileCredentialStore::new(&path);

		let creds = CredentialValue::ApiKey {
			key: SecretString::new("test-token".to_string()),
		};
		store.save("test-provider", &creds).await.unwrap();

		let loaded = store.load("test-provider").await.unwrap();
		assert!(loaded.is_some());

		store.delete("test-provider").await.unwrap();
		let loaded = store.load("test-provider").await.unwrap();
		assert!(loaded.is_none());
	}

	#[tokio::test]
	async fn test_keyring_then_file_creation() {
		let temp_dir = tempfile::tempdir().unwrap();
		let path = temp_dir.path().join("credentials.json");
		let store = KeyringThenFileStore::new("loom-test", &path);

		assert!(!path.exists());
		let _ = store.load("nonexistent").await;
	}
}
