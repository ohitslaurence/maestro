// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use async_trait::async_trait;

use crate::error::CredentialError;
use crate::store::CredentialStore;
use crate::value::{CredentialValue, PersistedCredentialValue};

#[derive(Debug, Clone)]
pub struct KeyringCredentialStore {
	service: String,
}

impl KeyringCredentialStore {
	pub fn new(service: impl Into<String>) -> Self {
		Self {
			service: service.into(),
		}
	}
}

#[async_trait]
impl CredentialStore for KeyringCredentialStore {
	async fn load(&self, provider: &str) -> Result<Option<CredentialValue>, CredentialError> {
		let service = self.service.clone();
		let provider = provider.to_string();

		tokio::task::spawn_blocking(move || {
			let entry = keyring::Entry::new(&service, &provider)
				.map_err(|e| CredentialError::Backend(e.to_string()))?;

			match entry.get_password() {
				Ok(data) => {
					let persisted: PersistedCredentialValue =
						serde_json::from_str(&data).map_err(|e| CredentialError::Parse(e.to_string()))?;
					Ok(Some(CredentialValue::from(persisted)))
				}
				Err(keyring::Error::NoEntry) => Ok(None),
				Err(e) => Err(CredentialError::Backend(e.to_string())),
			}
		})
		.await
		.map_err(|e| CredentialError::Backend(e.to_string()))?
	}

	async fn save(&self, provider: &str, creds: &CredentialValue) -> Result<(), CredentialError> {
		let service = self.service.clone();
		let provider = provider.to_string();
		let persisted = PersistedCredentialValue::from(creds);
		let data =
			serde_json::to_string(&persisted).map_err(|e| CredentialError::Parse(e.to_string()))?;

		tokio::task::spawn_blocking(move || {
			let entry = keyring::Entry::new(&service, &provider)
				.map_err(|e| CredentialError::Backend(e.to_string()))?;
			entry
				.set_password(&data)
				.map_err(|e| CredentialError::Backend(e.to_string()))?;

			// Verify the save worked by reading back with a NEW entry instance.
			// This detects mock backends that only store in-memory per-instance.
			let verify_entry = keyring::Entry::new(&service, &provider)
				.map_err(|e| CredentialError::Backend(e.to_string()))?;
			match verify_entry.get_password() {
				Ok(stored) if stored == data => Ok(()),
				Ok(_) => Err(CredentialError::Backend(
					"keyring verification failed: stored data mismatch".to_string(),
				)),
				Err(keyring::Error::NoEntry) => Err(CredentialError::Backend(
					"keyring verification failed: credential not persisted (mock backend?)".to_string(),
				)),
				Err(e) => Err(CredentialError::Backend(format!(
					"keyring verification failed: {e}"
				))),
			}
		})
		.await
		.map_err(|e| CredentialError::Backend(e.to_string()))?
	}

	async fn delete(&self, provider: &str) -> Result<(), CredentialError> {
		let service = self.service.clone();
		let provider = provider.to_string();

		tokio::task::spawn_blocking(move || {
			let entry = keyring::Entry::new(&service, &provider)
				.map_err(|e| CredentialError::Backend(e.to_string()))?;

			match entry.delete_credential() {
				Ok(()) => Ok(()),
				Err(keyring::Error::NoEntry) => Ok(()),
				Err(e) => Err(CredentialError::Backend(e.to_string())),
			}
		})
		.await
		.map_err(|e| CredentialError::Backend(e.to_string()))?
	}
}
