// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! OAuth client for managing Anthropic credentials with automatic token refresh.

use std::sync::Arc;

use loom_cli_credentials::{CredentialError, CredentialStore, CredentialValue};
use loom_common_secret::SecretString;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

use super::oauth_flow::{refresh_token, ExchangeResult};

/// OAuth credentials with runtime state.
#[derive(Debug, Clone)]
pub struct OAuthCredentials {
	pub refresh: SecretString,
	pub access: SecretString,
	pub expires: u64,
}

impl OAuthCredentials {
	/// Create new OAuth credentials.
	pub fn new(refresh: SecretString, access: SecretString, expires: u64) -> Self {
		Self {
			refresh,
			access,
			expires,
		}
	}

	/// Check if the token is expired (with 60 second buffer).
	pub fn is_expired(&self) -> bool {
		let now_ms = std::time::SystemTime::now()
			.duration_since(std::time::UNIX_EPOCH)
			.unwrap()
			.as_millis() as u64;

		self.expires < now_ms + 60_000
	}
}

/// Manages OAuth credentials with automatic token refresh.
#[derive(Debug)]
pub struct OAuthClient<S: CredentialStore> {
	provider_id: String,
	credentials: Arc<RwLock<OAuthCredentials>>,
	store: Arc<S>,
}

impl<S: CredentialStore> Clone for OAuthClient<S> {
	fn clone(&self) -> Self {
		Self {
			provider_id: self.provider_id.clone(),
			credentials: Arc::clone(&self.credentials),
			store: Arc::clone(&self.store),
		}
	}
}

impl<S: CredentialStore> OAuthClient<S> {
	/// Create a new OAuth client.
	pub fn new(provider_id: impl Into<String>, credentials: OAuthCredentials, store: Arc<S>) -> Self {
		Self {
			provider_id: provider_id.into(),
			credentials: Arc::new(RwLock::new(credentials)),
			store,
		}
	}

	/// Get a valid access token, refreshing if necessary.
	pub async fn get_access_token(&self) -> Result<String, CredentialError> {
		{
			let creds = self.credentials.read().await;
			if !creds.is_expired() {
				return Ok(creds.access.expose().clone());
			}
		}

		let mut creds = self.credentials.write().await;

		if !creds.is_expired() {
			return Ok(creds.access.expose().clone());
		}

		info!(provider = %self.provider_id, "Access token expired, refreshing");

		match refresh_token(creds.refresh.expose()).await? {
			ExchangeResult::Success {
				access,
				refresh,
				expires,
			} => {
				creds.access = SecretString::new(access.clone());
				creds.refresh = SecretString::new(refresh.clone());
				creds.expires = expires;

				let stored_creds = CredentialValue::OAuth {
					refresh: creds.refresh.clone(),
					access: creds.access.clone(),
					expires: creds.expires,
				};
				if let Err(e) = self.store.save(&self.provider_id, &stored_creds).await {
					warn!(error = %e, "Failed to persist refreshed credentials");
				}

				debug!(provider = %self.provider_id, "Token refreshed successfully");
				Ok(access)
			}
			ExchangeResult::Failed { error } => Err(CredentialError::RefreshFailed(error)),
		}
	}

	/// Get the current credentials (for inspection, not for use).
	pub async fn current_credentials(&self) -> OAuthCredentials {
		self.credentials.read().await.clone()
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use loom_cli_credentials::MemoryCredentialStore;

	#[test]
	fn test_oauth_credentials_not_expired() {
		let now_ms = std::time::SystemTime::now()
			.duration_since(std::time::UNIX_EPOCH)
			.unwrap()
			.as_millis() as u64;

		let creds = OAuthCredentials::new(
			SecretString::new("rt_test".to_string()),
			SecretString::new("at_test".to_string()),
			now_ms + 120_000,
		);

		assert!(!creds.is_expired());
	}

	#[test]
	fn test_oauth_credentials_expired() {
		let now_ms = std::time::SystemTime::now()
			.duration_since(std::time::UNIX_EPOCH)
			.unwrap()
			.as_millis() as u64;

		let creds = OAuthCredentials::new(
			SecretString::new("rt_test".to_string()),
			SecretString::new("at_test".to_string()),
			now_ms - 1000,
		);

		assert!(creds.is_expired());
	}

	#[test]
	fn test_oauth_credentials_within_buffer() {
		let now_ms = std::time::SystemTime::now()
			.duration_since(std::time::UNIX_EPOCH)
			.unwrap()
			.as_millis() as u64;

		let creds = OAuthCredentials::new(
			SecretString::new("rt_test".to_string()),
			SecretString::new("at_test".to_string()),
			now_ms + 30_000,
		);

		assert!(creds.is_expired());
	}

	#[tokio::test]
	async fn test_oauth_client_returns_valid_token() {
		let store = Arc::new(MemoryCredentialStore::new());
		let now_ms = std::time::SystemTime::now()
			.duration_since(std::time::UNIX_EPOCH)
			.unwrap()
			.as_millis() as u64;

		let creds = OAuthCredentials::new(
			SecretString::new("rt_test".to_string()),
			SecretString::new("at_valid".to_string()),
			now_ms + 120_000,
		);

		let client = OAuthClient::new("anthropic", creds, store);
		let token = client.get_access_token().await.unwrap();

		assert_eq!(token, "at_valid");
	}
}
