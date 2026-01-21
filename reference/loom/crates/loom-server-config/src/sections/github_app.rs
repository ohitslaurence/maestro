// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! GitHub App configuration section.

use crate::error::ConfigError;
use loom_common_config::SecretString;
use serde::{Deserialize, Serialize};

const DEFAULT_APP_SLUG: &str = "loom";
const DEFAULT_BASE_URL: &str = "https://api.github.com";

/// Configuration layer for GitHub App (all fields optional for layering).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GitHubAppConfigLayer {
	/// GitHub App numeric ID.
	pub app_id: Option<u64>,
	/// PEM-encoded RSA private key for JWT signing.
	#[serde(skip_serializing)]
	pub private_key_pem: Option<SecretString>,
	/// Secret for webhook signature verification.
	#[serde(skip_serializing)]
	pub webhook_secret: Option<SecretString>,
	/// App slug for installation URL generation.
	pub app_slug: Option<String>,
	/// Base URL for GitHub API.
	pub base_url: Option<String>,
}

impl GitHubAppConfigLayer {
	/// Merge with another layer, preferring values from `other`.
	pub fn merge(&mut self, other: GitHubAppConfigLayer) {
		if other.app_id.is_some() {
			self.app_id = other.app_id;
		}
		if other.private_key_pem.is_some() {
			self.private_key_pem = other.private_key_pem;
		}
		if other.webhook_secret.is_some() {
			self.webhook_secret = other.webhook_secret;
		}
		if other.app_slug.is_some() {
			self.app_slug = other.app_slug;
		}
		if other.base_url.is_some() {
			self.base_url = other.base_url;
		}
	}

	/// Check if GitHub App is configured (app_id is set).
	pub fn is_configured(&self) -> bool {
		self.app_id.is_some()
	}

	/// Check if any GitHub App field is set (for partial config detection).
	pub fn has_any_field(&self) -> bool {
		self.app_id.is_some()
			|| self.private_key_pem.is_some()
			|| self.webhook_secret.is_some()
			|| self.app_slug.is_some()
			|| self
				.base_url
				.as_ref()
				.is_some_and(|s| s != DEFAULT_BASE_URL)
	}

	/// Finalize the layer into a runtime configuration.
	pub fn finalize(self) -> Option<GitHubAppConfig> {
		self.build().ok().flatten()
	}

	/// Build the final config, returning None if not configured.
	pub fn build(self) -> Result<Option<GitHubAppConfig>, ConfigError> {
		let has_any = self.app_id.is_some() || self.private_key_pem.is_some();

		if !has_any {
			return Ok(None);
		}

		let app_id = self
			.app_id
			.ok_or_else(|| ConfigError::Validation("GitHub App app_id is required".to_string()))?;

		let private_key_pem = self.private_key_pem.ok_or_else(|| {
			ConfigError::Validation(
				"GitHub App private_key_pem is required when app_id is set".to_string(),
			)
		})?;

		if private_key_pem.expose().is_empty() {
			return Err(ConfigError::Validation(
				"GitHub App private_key_pem cannot be empty".to_string(),
			));
		}

		let base_url = self
			.base_url
			.unwrap_or_else(|| DEFAULT_BASE_URL.to_string());

		Self::validate_base_url(&base_url)?;

		Ok(Some(GitHubAppConfig {
			app_id,
			private_key_pem,
			webhook_secret: self.webhook_secret,
			app_slug: self
				.app_slug
				.unwrap_or_else(|| DEFAULT_APP_SLUG.to_string()),
			base_url,
		}))
	}

	/// Validate the base URL format.
	fn validate_base_url(url: &str) -> Result<(), ConfigError> {
		if !url.starts_with("https://") {
			return Err(ConfigError::Validation(format!(
				"GitHub App base_url must use HTTPS, got: {url}"
			)));
		}

		if url.contains("localhost") || url.contains("127.0.0.1") || url.contains("::1") {
			return Err(ConfigError::Validation(
				"GitHub App base_url must not be localhost".to_string(),
			));
		}

		Ok(())
	}
}

/// Validated GitHub App configuration.
#[derive(Debug, Clone)]
pub struct GitHubAppConfig {
	/// GitHub App numeric ID.
	app_id: u64,
	/// PEM-encoded RSA private key for JWT signing.
	private_key_pem: SecretString,
	/// Secret for webhook signature verification (optional).
	webhook_secret: Option<SecretString>,
	/// App slug for installation URL generation.
	app_slug: String,
	/// Base URL for GitHub API.
	base_url: String,
}

impl GitHubAppConfig {
	/// Get the GitHub App ID.
	pub fn app_id(&self) -> u64 {
		self.app_id
	}

	/// Get the private key PEM (for internal JWT generation).
	pub fn private_key_pem(&self) -> &str {
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

	/// Get the base URL.
	pub fn base_url(&self) -> &str {
		&self.base_url
	}

	/// Get the installation URL for users to install the app.
	pub fn installation_url(&self) -> String {
		if self.base_url.starts_with(DEFAULT_BASE_URL) {
			format!(
				"https://github.com/apps/{}/installations/new",
				self.app_slug
			)
		} else {
			let base = self.base_url.trim_end_matches("/api/v3");
			format!("{}/apps/{}/installations/new", base, self.app_slug)
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use loom_common_config::Secret;

	mod config_layer {
		use super::*;

		#[test]
		fn returns_none_when_not_configured() {
			let layer = GitHubAppConfigLayer::default();
			assert!(!layer.is_configured());
			assert!(layer.build().unwrap().is_none());
		}

		#[test]
		fn requires_private_key_when_app_id_set() {
			let layer = GitHubAppConfigLayer {
				app_id: Some(12345),
				..Default::default()
			};
			assert!(layer.build().is_err());
		}

		#[test]
		fn requires_app_id_when_private_key_set() {
			let layer = GitHubAppConfigLayer {
				private_key_pem: Some(Secret::new("-----BEGIN RSA PRIVATE KEY-----".to_string())),
				..Default::default()
			};
			assert!(layer.build().is_err());
		}

		#[test]
		fn rejects_empty_private_key() {
			let layer = GitHubAppConfigLayer {
				app_id: Some(12345),
				private_key_pem: Some(Secret::new(String::new())),
				..Default::default()
			};
			assert!(layer.build().is_err());
		}

		#[test]
		fn builds_minimal_config() {
			let layer = GitHubAppConfigLayer {
				app_id: Some(12345),
				private_key_pem: Some(Secret::new("-----BEGIN RSA PRIVATE KEY-----".to_string())),
				..Default::default()
			};

			let config = layer.build().unwrap().unwrap();
			assert_eq!(config.app_id(), 12345);
			assert_eq!(config.app_slug(), DEFAULT_APP_SLUG);
			assert_eq!(config.base_url(), DEFAULT_BASE_URL);
			assert!(config.webhook_secret().is_none());
		}

		#[test]
		fn builds_full_config() {
			let layer = GitHubAppConfigLayer {
				app_id: Some(12345),
				private_key_pem: Some(Secret::new("-----BEGIN RSA PRIVATE KEY-----".to_string())),
				webhook_secret: Some(Secret::new("webhook-secret".to_string())),
				app_slug: Some("my-app".to_string()),
				base_url: Some("https://github.example.com/api/v3".to_string()),
			};

			let config = layer.build().unwrap().unwrap();
			assert_eq!(config.app_id(), 12345);
			assert_eq!(config.app_slug(), "my-app");
			assert_eq!(config.base_url(), "https://github.example.com/api/v3");
			assert_eq!(config.webhook_secret(), Some("webhook-secret"));
		}

		#[test]
		fn rejects_http_base_url() {
			let layer = GitHubAppConfigLayer {
				app_id: Some(12345),
				private_key_pem: Some(Secret::new("key".to_string())),
				base_url: Some("http://api.github.com".to_string()),
				..Default::default()
			};
			assert!(layer.build().is_err());
		}

		#[test]
		fn rejects_localhost_base_url() {
			let layer = GitHubAppConfigLayer {
				app_id: Some(12345),
				private_key_pem: Some(Secret::new("key".to_string())),
				base_url: Some("https://localhost:8080".to_string()),
				..Default::default()
			};
			assert!(layer.build().is_err());
		}

		#[test]
		fn merge_prefers_other_values() {
			let mut base = GitHubAppConfigLayer {
				app_id: Some(11111),
				app_slug: Some("base-app".to_string()),
				..Default::default()
			};

			let overlay = GitHubAppConfigLayer {
				app_id: Some(22222),
				private_key_pem: Some(Secret::new("overlay-key".to_string())),
				..Default::default()
			};

			base.merge(overlay);

			assert_eq!(base.app_id, Some(22222));
			assert_eq!(base.app_slug, Some("base-app".to_string())); // Not overwritten
			assert!(base.private_key_pem.is_some());
		}

		#[test]
		fn has_any_field_detects_partial_config() {
			let layer = GitHubAppConfigLayer {
				webhook_secret: Some(Secret::new("secret".to_string())),
				..Default::default()
			};
			assert!(layer.has_any_field());
		}
	}

	mod config {
		use super::*;

		#[test]
		fn installation_url_github_com() {
			let layer = GitHubAppConfigLayer {
				app_id: Some(12345),
				private_key_pem: Some(Secret::new("key".to_string())),
				app_slug: Some("my-app".to_string()),
				..Default::default()
			};

			let config = layer.build().unwrap().unwrap();
			assert_eq!(
				config.installation_url(),
				"https://github.com/apps/my-app/installations/new"
			);
		}

		#[test]
		fn installation_url_enterprise() {
			let layer = GitHubAppConfigLayer {
				app_id: Some(12345),
				private_key_pem: Some(Secret::new("key".to_string())),
				app_slug: Some("my-app".to_string()),
				base_url: Some("https://github.example.com/api/v3".to_string()),
				..Default::default()
			};

			let config = layer.build().unwrap().unwrap();
			assert_eq!(
				config.installation_url(),
				"https://github.example.com/apps/my-app/installations/new"
			);
		}
	}

	mod secret_redaction {
		use super::*;

		#[test]
		fn private_key_not_in_debug() {
			let layer = GitHubAppConfigLayer {
				private_key_pem: Some(Secret::new("super_secret_key".to_string())),
				..Default::default()
			};

			let debug = format!("{layer:?}");
			assert!(!debug.contains("super_secret_key"));
			assert!(debug.contains("[REDACTED]"));
		}

		#[test]
		fn webhook_secret_not_in_debug() {
			let layer = GitHubAppConfigLayer {
				webhook_secret: Some(Secret::new("webhook_secret_value".to_string())),
				..Default::default()
			};

			let debug = format!("{layer:?}");
			assert!(!debug.contains("webhook_secret_value"));
			assert!(debug.contains("[REDACTED]"));
		}

		#[test]
		fn secrets_not_serialized() {
			let layer = GitHubAppConfigLayer {
				app_id: Some(12345),
				private_key_pem: Some(Secret::new("secret-key".to_string())),
				webhook_secret: Some(Secret::new("webhook-secret".to_string())),
				..Default::default()
			};

			let json = serde_json::to_string(&layer).unwrap();
			assert!(!json.contains("secret-key"));
			assert!(!json.contains("webhook-secret"));
			assert!(!json.contains("private_key_pem"));
			assert!(!json.contains("webhook_secret"));
		}

		#[test]
		fn config_debug_redacts_secrets() {
			let layer = GitHubAppConfigLayer {
				app_id: Some(12345),
				private_key_pem: Some(Secret::new("super-secret-key".to_string())),
				webhook_secret: Some(Secret::new("webhook-secret".to_string())),
				..Default::default()
			};

			let config = layer.build().unwrap().unwrap();
			let debug = format!("{config:?}");

			assert!(!debug.contains("super-secret-key"));
			assert!(!debug.contains("webhook-secret"));
			assert!(debug.contains("[REDACTED]"));
		}
	}
}

#[cfg(test)]
mod proptests {
	use super::*;
	use loom_common_config::Secret;
	use proptest::prelude::*;

	proptest! {
		#[test]
		fn private_key_never_in_debug(
			key in "[a-zA-Z0-9]{10,40}"
		) {
			prop_assume!(!key.contains("REDACTED"));

			let layer = GitHubAppConfigLayer {
				private_key_pem: Some(Secret::new(key.clone())),
				..Default::default()
			};

			let debug = format!("{layer:?}");
			prop_assert!(!debug.contains(&key));
		}

		#[test]
		fn webhook_secret_never_in_debug(
			secret in "[a-zA-Z0-9]{10,40}"
		) {
			prop_assume!(!secret.contains("REDACTED"));

			let layer = GitHubAppConfigLayer {
				webhook_secret: Some(Secret::new(secret.clone())),
				..Default::default()
			};

			let debug = format!("{layer:?}");
			prop_assert!(!debug.contains(&secret));
		}

		#[test]
		fn valid_config_builds_successfully(
			app_id in 1u64..1000000,
			private_key in "[a-zA-Z0-9]{20,100}",
		) {
			let layer = GitHubAppConfigLayer {
				app_id: Some(app_id),
				private_key_pem: Some(Secret::new(private_key)),
				..Default::default()
			};

			let result = layer.build();
			prop_assert!(result.is_ok());

			let config = result.unwrap().unwrap();
			prop_assert_eq!(config.app_id(), app_id);
			prop_assert_eq!(config.app_slug(), DEFAULT_APP_SLUG);
			prop_assert_eq!(config.base_url(), DEFAULT_BASE_URL);
		}

		#[test]
		fn installation_url_for_github_com(
			app_slug in "[a-z]{3,20}"
		) {
			let layer = GitHubAppConfigLayer {
				app_id: Some(12345),
				private_key_pem: Some(Secret::new("key".to_string())),
				app_slug: Some(app_slug.clone()),
				..Default::default()
			};

			let config = layer.build().unwrap().unwrap();
			let url = config.installation_url();

			prop_assert!(url.starts_with("https://github.com/apps/"));
			prop_assert!(url.contains(&app_slug));
			prop_assert!(url.ends_with("/installations/new"));
		}

		#[test]
		fn base_url_validation_rejects_http(
			domain in "[a-z]{3,10}\\.[a-z]{2,5}"
		) {
			let layer = GitHubAppConfigLayer {
				app_id: Some(12345),
				private_key_pem: Some(Secret::new("key".to_string())),
				base_url: Some(format!("http://{domain}")),
				..Default::default()
			};

			let result = layer.build();
			prop_assert!(result.is_err());
		}
	}
}
