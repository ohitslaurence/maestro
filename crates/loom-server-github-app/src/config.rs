// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights
// reserved. SPDX-License-Identifier: Proprietary

//! Configuration for GitHub App client.

use std::env;

use loom_common_config::{load_secret_env, Secret, SecretString};
use loom_common_http::RetryConfig;
use reqwest::Url;
use tracing::warn;

use crate::error::GithubAppError;

const DEFAULT_BASE_URL: &str = "https://api.github.com";
const DEFAULT_APP_SLUG: &str = "loom";

/// Configuration for the GitHub App client.
///
/// Sensitive fields (private key, webhook secret) are stored as
/// [`SecretString`] to prevent accidental logging. Use `.expose()` to access
/// the actual values.
#[derive(Clone)]
pub struct GithubAppConfig {
	/// GitHub App numeric ID
	app_id: u64,

	/// PEM-encoded RSA private key for JWT signing
	private_key_pem: SecretString,

	/// Secret for webhook signature verification
	webhook_secret: Option<SecretString>,

	/// App slug for installation URL generation
	app_slug: String,

	/// Base URL for GitHub API (validated HTTPS, parsed)
	base_url: Url,

	/// HTTP retry configuration
	pub retry_config: RetryConfig,
}

impl std::fmt::Debug for GithubAppConfig {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.debug_struct("GithubAppConfig")
			.field("app_id", &self.app_id)
			.field("private_key_pem", &self.private_key_pem)
			.field("webhook_secret", &self.webhook_secret)
			.field("app_slug", &self.app_slug)
			.field("base_url", &self.base_url.as_str())
			.field("retry_config", &self.retry_config)
			.finish()
	}
}

impl GithubAppConfig {
	/// Validate and normalize a base URL.
	///
	/// Requirements:
	/// - Must be a valid URL
	/// - Must use HTTPS scheme (security requirement)
	/// - Must have a host
	/// - Trailing slashes are normalized
	fn validate_and_normalize_base_url(raw: &str) -> Result<Url, GithubAppError> {
		let url = Url::parse(raw)
			.map_err(|e| GithubAppError::Config(format!("Invalid GitHub base URL '{raw}': {e}")))?;

		if url.scheme() != "https" {
			return Err(GithubAppError::Config(format!(
				"GitHub base URL must use https, got '{}'",
				url.scheme()
			)));
		}

		let host = url
			.host_str()
			.ok_or_else(|| GithubAppError::Config("GitHub base URL must include a host".to_string()))?;

		if host == "localhost" || host == "127.0.0.1" || host == "::1" {
			return Err(GithubAppError::Config(
				"GitHub base URL must not be localhost".to_string(),
			));
		}

		Ok(url)
	}

	/// Create a new configuration with required fields.
	///
	/// Uses the default GitHub API URL (https://api.github.com).
	pub fn new(app_id: u64, private_key_pem: impl Into<String>) -> Self {
		Self {
			app_id,
			private_key_pem: Secret::new(private_key_pem.into()),
			webhook_secret: None,
			app_slug: DEFAULT_APP_SLUG.to_string(),
			base_url: Url::parse(DEFAULT_BASE_URL).expect("default URL is valid"),
			retry_config: RetryConfig::default(),
		}
	}

	/// Create configuration from environment variables.
	///
	/// Required environment variables:
	/// - `LOOM_SERVER_GITHUB_APP_ID`: GitHub App numeric ID
	/// - `LOOM_SERVER_GITHUB_APP_PRIVATE_KEY`: PEM-encoded RSA private key (or `_FILE`
	///   suffix for file path)
	///
	/// Optional environment variables:
	/// - `LOOM_SERVER_GITHUB_APP_WEBHOOK_SECRET`: Secret for webhook verification (or
	///   `_FILE` suffix)
	/// - `LOOM_SERVER_GITHUB_APP_SLUG`: App slug (defaults to "loom")
	/// - `LOOM_SERVER_GITHUB_APP_BASE_URL`: API base URL (defaults to api.github.com,
	///   must be HTTPS)
	pub fn from_env() -> Result<Self, GithubAppError> {
		let app_id_str = env::var("LOOM_SERVER_GITHUB_APP_ID")
			.map_err(|_| GithubAppError::Config("LOOM_SERVER_GITHUB_APP_ID not set".to_string()))?;

		let app_id: u64 = app_id_str.parse().map_err(|_| {
			GithubAppError::Config(format!("Invalid LOOM_SERVER_GITHUB_APP_ID: {app_id_str}"))
		})?;

		let private_key_pem = load_secret_env("LOOM_SERVER_GITHUB_APP_PRIVATE_KEY")
			.map_err(|e| GithubAppError::Config(e.to_string()))?
			.ok_or_else(|| {
				GithubAppError::Config("LOOM_SERVER_GITHUB_APP_PRIVATE_KEY not set".to_string())
			})?;

		if private_key_pem.expose().is_empty() {
			return Err(GithubAppError::Config(
				"LOOM_SERVER_GITHUB_APP_PRIVATE_KEY is empty".to_string(),
			));
		}

		let webhook_secret = load_secret_env("LOOM_SERVER_GITHUB_APP_WEBHOOK_SECRET")
			.map_err(|e| GithubAppError::Config(e.to_string()))?;

		let app_slug =
			env::var("LOOM_SERVER_GITHUB_APP_SLUG").unwrap_or_else(|_| DEFAULT_APP_SLUG.to_string());

		let base_url_raw =
			env::var("LOOM_SERVER_GITHUB_APP_BASE_URL").unwrap_or_else(|_| DEFAULT_BASE_URL.to_string());
		let base_url = Self::validate_and_normalize_base_url(&base_url_raw)?;

		Ok(Self {
			app_id,
			private_key_pem,
			webhook_secret,
			app_slug,
			base_url,
			retry_config: RetryConfig::default(),
		})
	}

	/// Set a custom base URL (for GitHub Enterprise or testing).
	///
	/// The URL must be HTTPS and have a valid host.
	/// If validation fails, logs a warning and keeps the previous value.
	pub fn with_base_url(mut self, url: impl Into<String>) -> Self {
		let url_str = url.into();
		match Self::validate_and_normalize_base_url(&url_str) {
			Ok(validated) => self.base_url = validated,
			Err(e) => {
				warn!(error = %e, url = %url_str, "Invalid base_url in with_base_url, keeping previous value");
			}
		}
		self
	}

	/// Set a custom retry configuration.
	pub fn with_retry_config(mut self, config: RetryConfig) -> Self {
		self.retry_config = config;
		self
	}

	/// Set the webhook secret.
	pub fn with_webhook_secret(mut self, secret: impl Into<String>) -> Self {
		self.webhook_secret = Some(Secret::new(secret.into()));
		self
	}

	/// Set the app slug.
	pub fn with_app_slug(mut self, slug: impl Into<String>) -> Self {
		self.app_slug = slug.into();
		self
	}

	/// Get the GitHub App ID.
	pub fn app_id(&self) -> u64 {
		self.app_id
	}

	/// Get the private key PEM (for internal JWT generation).
	pub(crate) fn private_key_pem(&self) -> &str {
		self.private_key_pem.expose()
	}

	/// Get the webhook secret, if configured.
	pub fn webhook_secret(&self) -> Option<&str> {
		self.webhook_secret.as_ref().map(|s| s.expose().as_str())
	}

	/// Get the app slug.
	pub fn app_slug(&self) -> &str {
		&self.app_slug
	}

	/// Get the validated base URL.
	pub fn base_url(&self) -> &Url {
		&self.base_url
	}

	/// Get the installation URL for users to install the app.
	pub fn installation_url(&self) -> String {
		if self.base_url.as_str().starts_with(DEFAULT_BASE_URL) {
			format!(
				"https://github.com/apps/{}/installations/new",
				self.app_slug
			)
		} else {
			let base = self.base_url.as_str().trim_end_matches("/api/v3");
			format!("{}/apps/{}/installations/new", base, self.app_slug)
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_config_new() {
		let config = GithubAppConfig::new(12345, "test-private-key");
		assert_eq!(config.app_id(), 12345);
		assert_eq!(config.private_key_pem(), "test-private-key");
		assert!(config
			.base_url()
			.as_str()
			.starts_with("https://api.github.com"));
		assert_eq!(config.app_slug(), DEFAULT_APP_SLUG);
		assert!(config.webhook_secret().is_none());
	}

	#[test]
	fn test_config_builders() {
		let config = GithubAppConfig::new(12345, "key")
			.with_base_url("https://github.example.com/api/v3")
			.with_app_slug("my-app")
			.with_webhook_secret("secret123");

		assert_eq!(
			config.base_url().as_str(),
			"https://github.example.com/api/v3"
		);
		assert_eq!(config.app_slug(), "my-app");
		assert_eq!(config.webhook_secret(), Some("secret123"));
	}

	#[test]
	fn test_base_url_validation_rejects_http() {
		let result = GithubAppConfig::validate_and_normalize_base_url("http://api.github.com");
		assert!(result.is_err());
		assert!(result.unwrap_err().to_string().contains("https"));
	}

	#[test]
	fn test_base_url_validation_rejects_localhost() {
		let result = GithubAppConfig::validate_and_normalize_base_url("https://localhost:8080");
		assert!(result.is_err());
		assert!(result.unwrap_err().to_string().contains("localhost"));
	}

	#[test]
	fn test_base_url_validation_rejects_invalid_url() {
		let result = GithubAppConfig::validate_and_normalize_base_url("not-a-url");
		assert!(result.is_err());
	}

	#[test]
	fn test_base_url_validation_accepts_github_enterprise() {
		let result =
			GithubAppConfig::validate_and_normalize_base_url("https://github.example.com/api/v3");
		assert!(result.is_ok());
	}

	#[test]
	fn test_with_base_url_invalid_keeps_previous() {
		let config = GithubAppConfig::new(12345, "key").with_base_url("http://insecure.example.com");

		assert!(config
			.base_url()
			.as_str()
			.starts_with("https://api.github.com"));
	}

	#[test]
	fn test_installation_url_github_com() {
		let config = GithubAppConfig::new(12345, "key");
		assert_eq!(
			config.installation_url(),
			"https://github.com/apps/loom/installations/new"
		);
	}

	#[test]
	fn test_installation_url_enterprise() {
		let config = GithubAppConfig::new(12345, "key")
			.with_base_url("https://github.example.com/api/v3")
			.with_app_slug("my-app");
		assert_eq!(
			config.installation_url(),
			"https://github.example.com/apps/my-app/installations/new"
		);
	}

	/// Verifies that Debug output never contains sensitive values.
	/// This is critical for security - secrets must never appear in logs.
	#[test]
	fn test_debug_redacts_secrets() {
		let config =
			GithubAppConfig::new(12345, "super-secret-key").with_webhook_secret("webhook-secret");
		let debug_str = format!("{config:?}");

		assert!(!debug_str.contains("super-secret-key"));
		assert!(!debug_str.contains("webhook-secret"));
		assert!(debug_str.contains("[REDACTED]"));
	}
}
