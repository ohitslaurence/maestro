// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights
// reserved. SPDX-License-Identifier: Proprietary

//! OAuth state parameter store for CSRF protection.
//!
//! Stores state parameters temporarily during OAuth flows to validate
//! that callback requests originated from legitimate login attempts.
//!
//! # Security Properties
//!
//! - **CSRF Protection**: State parameters are cryptographically random and single-use
//! - **Replay Prevention**: Each state can only be consumed once
//! - **Time-Limited**: States expire after 10 minutes
//! - **Provider Binding**: States are bound to a specific OAuth provider
//! - **Nonce Support**: Optional nonce for OpenID Connect flows
//!
//! # Usage
//!
//! ```ignore
//! let store = OAuthStateStore::new();
//!
//! // Before OAuth redirect
//! let state = generate_state();
//! let nonce = generate_nonce();
//! store.store(state.clone(), "github".to_string(), Some(nonce)).await;
//!
//! // After OAuth callback
//! if let Some(entry) = store.validate_and_consume(&callback_state, "github").await {
//!     // Valid state, proceed with token exchange
//! } else {
//!     // Invalid or expired state, reject the request
//! }
//! ```
//!
//! # Cleanup
//!
//! Call [`OAuthStateStore::cleanup_expired`] periodically to remove stale entries:
//!
//! ```ignore
//! tokio::spawn(async move {
//!     loop {
//!         tokio::time::sleep(Duration::from_secs(60)).await;
//!         store.cleanup_expired().await;
//!     }
//! });
//! ```

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use tracing::instrument;

/// State expiry time in seconds (10 minutes).
///
/// This is a balance between:
/// - User experience (allowing time for slow OAuth providers or user hesitation)
/// - Security (limiting the window for state parameter attacks)
const STATE_EXPIRY_SECONDS: u64 = 600;

/// An entry in the OAuth state store.
///
/// Contains metadata about a pending OAuth flow, including the provider
/// and optional nonce for OpenID Connect.
#[derive(Debug, Clone)]
pub struct OAuthStateEntry {
	/// The OAuth provider (e.g., "github", "google", "okta")
	pub provider: String,
	/// Optional nonce for OpenID Connect flows
	pub nonce: Option<String>,
	/// When this entry was created
	pub created_at: Instant,
	/// Optional redirect URL after successful authentication
	pub redirect_url: Option<String>,
}

/// Check if a redirect URL is safe (relative path only).
///
/// Prevents open redirect attacks by ensuring the URL is a relative
/// path starting with "/" but not "//".
pub fn is_safe_redirect(url: &str) -> bool {
	url.starts_with('/') && !url.starts_with("//")
}

/// Sanitize a redirect URL, returning "/" if the URL is not safe.
pub fn sanitize_redirect(url: Option<&str>) -> String {
	match url {
		Some(u) if is_safe_redirect(u) => u.to_string(),
		_ => "/".to_string(),
	}
}

/// In-memory store for OAuth state parameters.
///
/// This store provides thread-safe storage for OAuth state parameters,
/// ensuring each state can only be used once and expires after a timeout.
///
/// # Thread Safety
///
/// Uses `RwLock` for concurrent access, allowing multiple readers but
/// exclusive writers. The lock is held briefly during operations.
///
/// # Memory Management
///
/// Call [`cleanup_expired`](Self::cleanup_expired) periodically to remove
/// stale entries and prevent memory growth.
#[derive(Debug, Clone, Default)]
pub struct OAuthStateStore {
	states: Arc<RwLock<HashMap<String, OAuthStateEntry>>>,
}

impl OAuthStateStore {
	/// Create a new empty state store.
	pub fn new() -> Self {
		Self {
			states: Arc::new(RwLock::new(HashMap::new())),
		}
	}

	/// Store a new OAuth state parameter.
	///
	/// The state will be valid for 10 minutes and can only be consumed once.
	///
	/// # Arguments
	///
	/// * `state` - The cryptographically random state parameter
	/// * `provider` - The OAuth provider name (e.g., "github")
	/// * `nonce` - Optional nonce for OpenID Connect flows
	/// * `redirect_url` - Optional redirect URL after successful authentication
	///
	/// # Security
	///
	/// - State values should be generated with [`generate_state`]
	/// - Never log the state value in plain text
	/// - Redirect URLs are sanitized before use (see [`sanitize_redirect`])
	#[instrument(skip(self, state, nonce, redirect_url), fields(provider = %provider))]
	pub async fn store(
		&self,
		state: String,
		provider: String,
		nonce: Option<String>,
		redirect_url: Option<String>,
	) {
		let entry = OAuthStateEntry {
			provider,
			nonce,
			created_at: Instant::now(),
			redirect_url,
		};
		let mut states = self.states.write().await;
		states.insert(state, entry);
		tracing::debug!(total_states = states.len(), "Stored OAuth state");
	}

	/// Validate and consume an OAuth state parameter.
	///
	/// If the state is valid (exists, not expired, matches provider), it is
	/// removed from the store and returned. This ensures single-use semantics.
	///
	/// # Arguments
	///
	/// * `state` - The state parameter from the OAuth callback
	/// * `expected_provider` - The expected OAuth provider name
	///
	/// # Returns
	///
	/// - `Some(entry)` if the state is valid and matches the provider
	/// - `None` if the state is missing, expired, or provider mismatch
	///
	/// # Security
	///
	/// - The state is always removed from the store, even if validation fails
	/// - This prevents timing attacks that could probe for valid states
	#[instrument(skip(self, state), fields(expected_provider = %expected_provider))]
	pub async fn validate_and_consume(
		&self,
		state: &str,
		expected_provider: &str,
	) -> Option<OAuthStateEntry> {
		let mut states = self.states.write().await;

		if let Some(entry) = states.remove(state) {
			let expiry = Duration::from_secs(STATE_EXPIRY_SECONDS);
			let elapsed = entry.created_at.elapsed();

			if elapsed >= expiry {
				tracing::debug!(
					elapsed_secs = elapsed.as_secs(),
					expiry_secs = STATE_EXPIRY_SECONDS,
					"OAuth state expired"
				);
				return None;
			}

			if entry.provider != expected_provider {
				tracing::warn!(
					actual_provider = %entry.provider,
					"OAuth state provider mismatch"
				);
				return None;
			}

			tracing::debug!("OAuth state validated and consumed");
			return Some(entry);
		}

		tracing::debug!("OAuth state not found");
		None
	}

	/// Remove all expired state entries.
	///
	/// Call this periodically (e.g., every minute) to prevent memory growth
	/// from abandoned OAuth flows.
	///
	/// # Returns
	///
	/// The number of entries removed.
	#[instrument(skip(self))]
	pub async fn cleanup_expired(&self) -> usize {
		let expiry = Duration::from_secs(STATE_EXPIRY_SECONDS);
		let mut states = self.states.write().await;
		let before = states.len();
		states.retain(|_, entry| entry.created_at.elapsed() < expiry);
		let removed = before - states.len();
		if removed > 0 {
			tracing::debug!(
				removed = removed,
				remaining = states.len(),
				"Cleaned up expired OAuth states"
			);
		}
		removed
	}

	/// Get the current number of stored states.
	///
	/// Useful for monitoring and debugging.
	pub async fn len(&self) -> usize {
		self.states.read().await.len()
	}

	/// Check if the store is empty.
	pub async fn is_empty(&self) -> bool {
		self.states.read().await.is_empty()
	}
}

/// Generate a cryptographically random state parameter.
///
/// Uses UUID v4 which provides 122 bits of randomness, sufficient
/// for CSRF protection.
///
/// # Security
///
/// - The returned value should be treated as sensitive
/// - Do not log state values in plain text
#[instrument]
pub fn generate_state() -> String {
	let state = uuid::Uuid::new_v4().to_string();
	tracing::debug!("Generated OAuth state");
	state
}

/// Generate a cryptographically random nonce for OpenID Connect.
///
/// Uses UUID v4 which provides 122 bits of randomness, sufficient
/// for replay protection.
///
/// # Security
///
/// - The returned value should be treated as sensitive
/// - Do not log nonce values in plain text
#[instrument]
pub fn generate_nonce() -> String {
	let nonce = uuid::Uuid::new_v4().to_string();
	tracing::debug!("Generated OAuth nonce");
	nonce
}

#[cfg(test)]
mod tests {
	use super::*;
	use proptest::prelude::*;
	use std::collections::HashSet;

	#[tokio::test]
	async fn test_store_and_validate() {
		let store = OAuthStateStore::new();
		let state = generate_state();

		store
			.store(state.clone(), "github".to_string(), None, None)
			.await;

		let entry = store.validate_and_consume(&state, "github").await;
		assert!(entry.is_some());
		assert_eq!(entry.unwrap().provider, "github");

		let entry2 = store.validate_and_consume(&state, "github").await;
		assert!(entry2.is_none());
	}

	#[tokio::test]
	async fn test_wrong_provider_fails() {
		let store = OAuthStateStore::new();
		let state = generate_state();

		store
			.store(state.clone(), "github".to_string(), None, None)
			.await;

		let entry = store.validate_and_consume(&state, "google").await;
		assert!(entry.is_none());
	}

	#[tokio::test]
	async fn test_nonce_stored() {
		let store = OAuthStateStore::new();
		let state = generate_state();
		let nonce = generate_nonce();

		store
			.store(
				state.clone(),
				"google".to_string(),
				Some(nonce.clone()),
				None,
			)
			.await;

		let entry = store.validate_and_consume(&state, "google").await;
		assert!(entry.is_some());
		assert_eq!(entry.unwrap().nonce, Some(nonce));
	}

	#[tokio::test]
	async fn test_redirect_url_stored() {
		let store = OAuthStateStore::new();
		let state = generate_state();

		store
			.store(
				state.clone(),
				"github".to_string(),
				None,
				Some("/dashboard".to_string()),
			)
			.await;

		let entry = store.validate_and_consume(&state, "github").await;
		assert!(entry.is_some());
		assert_eq!(entry.unwrap().redirect_url, Some("/dashboard".to_string()));
	}

	#[tokio::test]
	async fn test_cleanup_expired() {
		let store = OAuthStateStore::new();

		store
			.store("state1".to_string(), "github".to_string(), None, None)
			.await;
		store
			.store("state2".to_string(), "google".to_string(), None, None)
			.await;

		assert_eq!(store.len().await, 2);

		let removed = store.cleanup_expired().await;
		assert_eq!(removed, 0);
		assert_eq!(store.len().await, 2);
	}

	#[tokio::test]
	async fn test_len_and_is_empty() {
		let store = OAuthStateStore::new();

		assert!(store.is_empty().await);
		assert_eq!(store.len().await, 0);

		store
			.store("state1".to_string(), "github".to_string(), None, None)
			.await;

		assert!(!store.is_empty().await);
		assert_eq!(store.len().await, 1);
	}

	#[tokio::test]
	async fn test_nonexistent_state() {
		let store = OAuthStateStore::new();
		let entry = store.validate_and_consume("nonexistent", "github").await;
		assert!(entry.is_none());
	}

	#[tokio::test]
	async fn test_double_consume_fails() {
		let store = OAuthStateStore::new();
		let state = generate_state();

		store
			.store(state.clone(), "github".to_string(), None, None)
			.await;

		let entry1 = store.validate_and_consume(&state, "github").await;
		assert!(entry1.is_some());

		let entry2 = store.validate_and_consume(&state, "github").await;
		assert!(entry2.is_none());
	}

	mod property_tests {
		use super::*;

		proptest! {
			/// Verifies that generate_state produces unique values.
			/// This is critical for CSRF protection.
			#[test]
			fn generate_state_is_unique(_seed in 0u64..1000) {
				let mut states = HashSet::new();
				for _ in 0..100 {
					let state = generate_state();
					prop_assert!(states.insert(state), "Generated duplicate state");
				}
			}

			/// Verifies that generate_nonce produces unique values.
			/// This is critical for replay protection in OIDC.
			#[test]
			fn generate_nonce_is_unique(_seed in 0u64..1000) {
				let mut nonces = HashSet::new();
				for _ in 0..100 {
					let nonce = generate_nonce();
					prop_assert!(nonces.insert(nonce), "Generated duplicate nonce");
				}
			}

			/// Verifies that generated states are valid UUIDs.
			#[test]
			fn generate_state_is_valid_uuid(_seed in 0u64..100) {
				let state = generate_state();
				let parsed = uuid::Uuid::parse_str(&state);
				prop_assert!(parsed.is_ok(), "State should be a valid UUID");
			}

			/// Verifies that generated nonces are valid UUIDs.
			#[test]
			fn generate_nonce_is_valid_uuid(_seed in 0u64..100) {
				let nonce = generate_nonce();
				let parsed = uuid::Uuid::parse_str(&nonce);
				prop_assert!(parsed.is_ok(), "Nonce should be a valid UUID");
			}

			/// Verifies that provider matching is exact (case-sensitive).
			#[test]
			fn provider_matching_is_case_sensitive(provider in "[a-z]{4,10}") {
				let rt = tokio::runtime::Builder::new_current_thread()
					.enable_all()
					.build()
					.unwrap();

				rt.block_on(async {
					let store = OAuthStateStore::new();
					let state = generate_state();

					store.store(state.clone(), provider.clone(), None, None).await;

					let upper = provider.to_uppercase();
					if upper != provider {
						let entry = store.validate_and_consume(&state, &upper).await;
						assert!(entry.is_none(), "Provider match should be case-sensitive");
					}
				});
			}

			/// Verifies that each state can only be consumed once (single-use).
			#[test]
			fn states_are_single_use(provider in "[a-z]{4,10}") {
				let rt = tokio::runtime::Builder::new_current_thread()
					.enable_all()
					.build()
					.unwrap();

				rt.block_on(async {
					let store = OAuthStateStore::new();
					let state = generate_state();

					store.store(state.clone(), provider.clone(), None, None).await;

					let first = store.validate_and_consume(&state, &provider).await;
					assert!(first.is_some(), "First consume should succeed");

					let second = store.validate_and_consume(&state, &provider).await;
					assert!(second.is_none(), "Second consume should fail");
				});
			}
		}
	}
}

#[cfg(test)]
mod redirect_validation_tests {
	use super::*;

	#[test]
	fn test_is_safe_redirect_valid_paths() {
		assert!(is_safe_redirect("/"));
		assert!(is_safe_redirect("/dashboard"));
		assert!(is_safe_redirect("/threads/123"));
		assert!(is_safe_redirect("/path?query=value"));
		assert!(is_safe_redirect("/path#fragment"));
	}

	#[test]
	fn test_is_safe_redirect_rejects_absolute_urls() {
		assert!(!is_safe_redirect("https://evil.com"));
		assert!(!is_safe_redirect("http://evil.com"));
		assert!(!is_safe_redirect("//evil.com"));
		assert!(!is_safe_redirect("//evil.com/path"));
	}

	#[test]
	fn test_is_safe_redirect_rejects_relative_without_slash() {
		assert!(!is_safe_redirect("dashboard"));
		assert!(!is_safe_redirect(""));
	}

	#[test]
	fn test_sanitize_redirect() {
		assert_eq!(sanitize_redirect(Some("/")), "/");
		assert_eq!(sanitize_redirect(Some("/dashboard")), "/dashboard");
		assert_eq!(sanitize_redirect(Some("https://evil.com")), "/");
		assert_eq!(sanitize_redirect(Some("//evil.com")), "/");
		assert_eq!(sanitize_redirect(None), "/");
	}
}
