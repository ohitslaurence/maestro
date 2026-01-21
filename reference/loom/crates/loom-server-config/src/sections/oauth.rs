// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! OAuth configuration section aggregating all OAuth provider configs.

use crate::error::ConfigError;
use loom_common_config::SecretString;
use serde::{Deserialize, Serialize};

/// Configuration layer for GitHub OAuth (all fields optional for layering).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GitHubOAuthConfigLayer {
	/// The OAuth application client ID.
	pub client_id: Option<String>,
	/// The OAuth application client secret.
	#[serde(skip_serializing)]
	pub client_secret: Option<SecretString>,
	/// The callback URL where GitHub redirects after authorization.
	pub redirect_uri: Option<String>,
	/// OAuth scopes to request.
	pub scopes: Option<Vec<String>>,
}

impl GitHubOAuthConfigLayer {
	/// Merge with another layer, preferring values from `other`.
	pub fn merge(&mut self, other: GitHubOAuthConfigLayer) {
		if other.client_id.is_some() {
			self.client_id = other.client_id;
		}
		if other.client_secret.is_some() {
			self.client_secret = other.client_secret;
		}
		if other.redirect_uri.is_some() {
			self.redirect_uri = other.redirect_uri;
		}
		if other.scopes.is_some() {
			self.scopes = other.scopes;
		}
	}

	/// Check if GitHub OAuth is configured.
	pub fn is_configured(&self) -> bool {
		self.client_id.as_ref().is_some_and(|s| !s.is_empty())
	}

	/// Build the final config, returning None if not configured.
	pub fn build(self) -> Result<Option<GitHubOAuthConfig>, ConfigError> {
		let Some(client_id) = self.client_id.filter(|s| !s.is_empty()) else {
			return Ok(None);
		};

		let client_secret = self.client_secret.ok_or_else(|| {
			ConfigError::Validation(
				"GitHub OAuth client_secret is required when client_id is set".to_string(),
			)
		})?;

		if client_secret.expose().is_empty() {
			return Err(ConfigError::Validation(
				"GitHub OAuth client_secret cannot be empty".to_string(),
			));
		}

		let redirect_uri = self.redirect_uri.ok_or_else(|| {
			ConfigError::Validation(
				"GitHub OAuth redirect_uri is required when client_id is set".to_string(),
			)
		})?;

		if redirect_uri.is_empty() {
			return Err(ConfigError::Validation(
				"GitHub OAuth redirect_uri cannot be empty".to_string(),
			));
		}

		let scopes = self
			.scopes
			.unwrap_or_else(|| vec!["user:email".to_string(), "read:user".to_string()]);

		Ok(Some(GitHubOAuthConfig {
			client_id,
			client_secret,
			redirect_uri,
			scopes,
		}))
	}
}

/// Validated GitHub OAuth configuration.
#[derive(Debug, Clone)]
pub struct GitHubOAuthConfig {
	/// The OAuth application client ID.
	pub client_id: String,
	/// The OAuth application client secret.
	pub client_secret: SecretString,
	/// The callback URL where GitHub redirects after authorization.
	pub redirect_uri: String,
	/// OAuth scopes to request.
	pub scopes: Vec<String>,
}

impl GitHubOAuthConfig {
	/// Join scopes into a space-separated string.
	pub fn scopes_string(&self) -> String {
		self.scopes.join(" ")
	}
}

/// Configuration layer for Google OAuth (all fields optional for layering).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GoogleOAuthConfigLayer {
	/// The OAuth application client ID.
	pub client_id: Option<String>,
	/// The OAuth application client secret.
	#[serde(skip_serializing)]
	pub client_secret: Option<SecretString>,
	/// The callback URL where Google redirects after authorization.
	pub redirect_uri: Option<String>,
	/// OAuth/OIDC scopes to request.
	pub scopes: Option<Vec<String>>,
}

impl GoogleOAuthConfigLayer {
	/// Merge with another layer, preferring values from `other`.
	pub fn merge(&mut self, other: GoogleOAuthConfigLayer) {
		if other.client_id.is_some() {
			self.client_id = other.client_id;
		}
		if other.client_secret.is_some() {
			self.client_secret = other.client_secret;
		}
		if other.redirect_uri.is_some() {
			self.redirect_uri = other.redirect_uri;
		}
		if other.scopes.is_some() {
			self.scopes = other.scopes;
		}
	}

	/// Check if Google OAuth is configured.
	pub fn is_configured(&self) -> bool {
		self.client_id.as_ref().is_some_and(|s| !s.is_empty())
	}

	/// Build the final config, returning None if not configured.
	pub fn build(self) -> Result<Option<GoogleOAuthConfig>, ConfigError> {
		let Some(client_id) = self.client_id.filter(|s| !s.is_empty()) else {
			return Ok(None);
		};

		let client_secret = self.client_secret.ok_or_else(|| {
			ConfigError::Validation(
				"Google OAuth client_secret is required when client_id is set".to_string(),
			)
		})?;

		if client_secret.expose().is_empty() {
			return Err(ConfigError::Validation(
				"Google OAuth client_secret cannot be empty".to_string(),
			));
		}

		let redirect_uri = self.redirect_uri.ok_or_else(|| {
			ConfigError::Validation(
				"Google OAuth redirect_uri is required when client_id is set".to_string(),
			)
		})?;

		if redirect_uri.is_empty() {
			return Err(ConfigError::Validation(
				"Google OAuth redirect_uri cannot be empty".to_string(),
			));
		}

		let scopes = self.scopes.unwrap_or_else(|| {
			vec![
				"openid".to_string(),
				"email".to_string(),
				"profile".to_string(),
			]
		});

		Ok(Some(GoogleOAuthConfig {
			client_id,
			client_secret,
			redirect_uri,
			scopes,
		}))
	}
}

/// Validated Google OAuth configuration.
#[derive(Debug, Clone)]
pub struct GoogleOAuthConfig {
	/// The OAuth application client ID.
	pub client_id: String,
	/// The OAuth application client secret.
	pub client_secret: SecretString,
	/// The callback URL where Google redirects after authorization.
	pub redirect_uri: String,
	/// OAuth/OIDC scopes to request.
	pub scopes: Vec<String>,
}

impl GoogleOAuthConfig {
	/// Join scopes into a space-separated string.
	pub fn scopes_string(&self) -> String {
		self.scopes.join(" ")
	}
}

/// Configuration layer for Okta OAuth (all fields optional for layering).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct OktaOAuthConfigLayer {
	/// The Okta domain (e.g., "dev-123456.okta.com").
	pub domain: Option<String>,
	/// The OAuth application client ID.
	pub client_id: Option<String>,
	/// The OAuth application client secret.
	#[serde(skip_serializing)]
	pub client_secret: Option<SecretString>,
	/// The callback URL where Okta redirects after authorization.
	pub redirect_uri: Option<String>,
	/// OIDC scopes to request.
	pub scopes: Option<Vec<String>>,
}

impl OktaOAuthConfigLayer {
	/// Merge with another layer, preferring values from `other`.
	pub fn merge(&mut self, other: OktaOAuthConfigLayer) {
		if other.domain.is_some() {
			self.domain = other.domain;
		}
		if other.client_id.is_some() {
			self.client_id = other.client_id;
		}
		if other.client_secret.is_some() {
			self.client_secret = other.client_secret;
		}
		if other.redirect_uri.is_some() {
			self.redirect_uri = other.redirect_uri;
		}
		if other.scopes.is_some() {
			self.scopes = other.scopes;
		}
	}

	/// Check if Okta OAuth is configured.
	pub fn is_configured(&self) -> bool {
		self.domain.as_ref().is_some_and(|s| !s.is_empty())
			|| self.client_id.as_ref().is_some_and(|s| !s.is_empty())
	}

	/// Build the final config, returning None if not configured.
	pub fn build(self) -> Result<Option<OktaOAuthConfig>, ConfigError> {
		let has_any = self.domain.as_ref().is_some_and(|s| !s.is_empty())
			|| self.client_id.as_ref().is_some_and(|s| !s.is_empty())
			|| self.client_secret.is_some()
			|| self.redirect_uri.as_ref().is_some_and(|s| !s.is_empty());

		if !has_any {
			return Ok(None);
		}

		let domain = self
			.domain
			.filter(|s| !s.is_empty())
			.ok_or_else(|| ConfigError::Validation("Okta OAuth domain is required".to_string()))?;

		let client_id = self
			.client_id
			.filter(|s| !s.is_empty())
			.ok_or_else(|| ConfigError::Validation("Okta OAuth client_id is required".to_string()))?;

		let client_secret = self
			.client_secret
			.ok_or_else(|| ConfigError::Validation("Okta OAuth client_secret is required".to_string()))?;

		if client_secret.expose().is_empty() {
			return Err(ConfigError::Validation(
				"Okta OAuth client_secret cannot be empty".to_string(),
			));
		}

		let redirect_uri = self
			.redirect_uri
			.filter(|s| !s.is_empty())
			.ok_or_else(|| ConfigError::Validation("Okta OAuth redirect_uri is required".to_string()))?;

		let scopes = self.scopes.unwrap_or_else(|| {
			vec![
				"openid".to_string(),
				"email".to_string(),
				"profile".to_string(),
			]
		});

		Ok(Some(OktaOAuthConfig {
			domain,
			client_id,
			client_secret,
			redirect_uri,
			scopes,
		}))
	}
}

/// Validated Okta OAuth configuration.
#[derive(Debug, Clone)]
pub struct OktaOAuthConfig {
	/// The Okta domain (e.g., "dev-123456.okta.com").
	pub domain: String,
	/// The OAuth application client ID.
	pub client_id: String,
	/// The OAuth application client secret.
	pub client_secret: SecretString,
	/// The callback URL where Okta redirects after authorization.
	pub redirect_uri: String,
	/// OIDC scopes to request.
	pub scopes: Vec<String>,
}

impl OktaOAuthConfig {
	/// Construct the issuer URL from the domain.
	pub fn issuer_url(&self) -> String {
		format!("https://{}/oauth2/default", self.domain)
	}

	/// Join scopes into a space-separated string.
	pub fn scopes_string(&self) -> String {
		self.scopes.join(" ")
	}
}

/// Aggregated OAuth configuration layer for all providers.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct OAuthConfigLayer {
	/// GitHub OAuth configuration.
	pub github: GitHubOAuthConfigLayer,
	/// Google OAuth configuration.
	pub google: GoogleOAuthConfigLayer,
	/// Okta OAuth configuration.
	pub okta: OktaOAuthConfigLayer,
}

impl OAuthConfigLayer {
	/// Merge with another layer, preferring values from `other`.
	pub fn merge(&mut self, other: OAuthConfigLayer) {
		self.github.merge(other.github);
		self.google.merge(other.google);
		self.okta.merge(other.okta);
	}

	/// Build the final aggregated OAuth config.
	pub fn build(self) -> Result<OAuthConfig, ConfigError> {
		Ok(OAuthConfig {
			github: self.github.build()?,
			google: self.google.build()?,
			okta: self.okta.build()?,
		})
	}

	/// Finalize the layer into a runtime configuration.
	pub fn finalize(self) -> OAuthConfig {
		self.build().unwrap_or_default()
	}
}

/// Aggregated validated OAuth configuration for all providers.
#[derive(Debug, Clone, Default)]
pub struct OAuthConfig {
	/// GitHub OAuth configuration (if configured).
	pub github: Option<GitHubOAuthConfig>,
	/// Google OAuth configuration (if configured).
	pub google: Option<GoogleOAuthConfig>,
	/// Okta OAuth configuration (if configured).
	pub okta: Option<OktaOAuthConfig>,
}

impl OAuthConfig {
	/// Check if any OAuth provider is configured.
	pub fn has_any_provider(&self) -> bool {
		self.github.is_some() || self.google.is_some() || self.okta.is_some()
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use loom_common_config::Secret;

	mod github {
		use super::*;

		#[test]
		fn returns_none_when_not_configured() {
			let layer = GitHubOAuthConfigLayer::default();
			assert!(!layer.is_configured());
			assert!(layer.build().unwrap().is_none());
		}

		#[test]
		fn requires_client_secret_when_client_id_set() {
			let layer = GitHubOAuthConfigLayer {
				client_id: Some("test-id".to_string()),
				..Default::default()
			};
			assert!(layer.build().is_err());
		}

		#[test]
		fn requires_redirect_uri_when_client_id_set() {
			let layer = GitHubOAuthConfigLayer {
				client_id: Some("test-id".to_string()),
				client_secret: Some(Secret::new("secret".to_string())),
				..Default::default()
			};
			assert!(layer.build().is_err());
		}

		#[test]
		fn builds_valid_config() {
			let layer = GitHubOAuthConfigLayer {
				client_id: Some("test-id".to_string()),
				client_secret: Some(Secret::new("secret".to_string())),
				redirect_uri: Some("https://example.com/callback".to_string()),
				scopes: None,
			};

			let config = layer.build().unwrap().unwrap();
			assert_eq!(config.client_id, "test-id");
			assert_eq!(config.redirect_uri, "https://example.com/callback");
			assert_eq!(config.scopes, vec!["user:email", "read:user"]);
		}

		#[test]
		fn custom_scopes_override_defaults() {
			let layer = GitHubOAuthConfigLayer {
				client_id: Some("test-id".to_string()),
				client_secret: Some(Secret::new("secret".to_string())),
				redirect_uri: Some("https://example.com/callback".to_string()),
				scopes: Some(vec!["repo".to_string()]),
			};

			let config = layer.build().unwrap().unwrap();
			assert_eq!(config.scopes, vec!["repo"]);
		}

		#[test]
		fn client_secret_not_in_debug() {
			let layer = GitHubOAuthConfigLayer {
				client_secret: Some(Secret::new("super_secret".to_string())),
				..Default::default()
			};

			let debug = format!("{layer:?}");
			assert!(!debug.contains("super_secret"));
			assert!(debug.contains("[REDACTED]"));
		}
	}

	mod google {
		use super::*;

		#[test]
		fn returns_none_when_not_configured() {
			let layer = GoogleOAuthConfigLayer::default();
			assert!(!layer.is_configured());
			assert!(layer.build().unwrap().is_none());
		}

		#[test]
		fn builds_valid_config_with_defaults() {
			let layer = GoogleOAuthConfigLayer {
				client_id: Some("test-id".to_string()),
				client_secret: Some(Secret::new("secret".to_string())),
				redirect_uri: Some("https://example.com/callback".to_string()),
				scopes: None,
			};

			let config = layer.build().unwrap().unwrap();
			assert_eq!(config.scopes, vec!["openid", "email", "profile"]);
		}
	}

	mod okta {
		use super::*;

		#[test]
		fn returns_none_when_not_configured() {
			let layer = OktaOAuthConfigLayer::default();
			assert!(!layer.is_configured());
			assert!(layer.build().unwrap().is_none());
		}

		#[test]
		fn requires_all_fields() {
			let layer = OktaOAuthConfigLayer {
				domain: Some("dev-123456.okta.com".to_string()),
				..Default::default()
			};
			assert!(layer.build().is_err());
		}

		#[test]
		fn builds_valid_config() {
			let layer = OktaOAuthConfigLayer {
				domain: Some("dev-123456.okta.com".to_string()),
				client_id: Some("test-id".to_string()),
				client_secret: Some(Secret::new("secret".to_string())),
				redirect_uri: Some("https://example.com/callback".to_string()),
				scopes: None,
			};

			let config = layer.build().unwrap().unwrap();
			assert_eq!(config.domain, "dev-123456.okta.com");
			assert_eq!(
				config.issuer_url(),
				"https://dev-123456.okta.com/oauth2/default"
			);
			assert_eq!(config.scopes, vec!["openid", "email", "profile"]);
		}
	}

	mod oauth_config {
		use super::*;

		#[test]
		fn has_any_provider_returns_false_when_empty() {
			let config = OAuthConfig::default();
			assert!(!config.has_any_provider());
		}

		#[test]
		fn has_any_provider_returns_true_with_github() {
			let config = OAuthConfig {
				github: Some(GitHubOAuthConfig {
					client_id: "id".to_string(),
					client_secret: Secret::new("secret".to_string()),
					redirect_uri: "https://example.com".to_string(),
					scopes: vec![],
				}),
				..Default::default()
			};
			assert!(config.has_any_provider());
		}

		#[test]
		fn layer_builds_with_no_providers() {
			let layer = OAuthConfigLayer::default();
			let config = layer.build().unwrap();
			assert!(config.github.is_none());
			assert!(config.google.is_none());
			assert!(config.okta.is_none());
		}

		#[test]
		fn layer_merge_combines_providers() {
			let mut base = OAuthConfigLayer {
				github: GitHubOAuthConfigLayer {
					client_id: Some("github-id".to_string()),
					..Default::default()
				},
				..Default::default()
			};

			let overlay = OAuthConfigLayer {
				google: GoogleOAuthConfigLayer {
					client_id: Some("google-id".to_string()),
					..Default::default()
				},
				..Default::default()
			};

			base.merge(overlay);

			assert_eq!(base.github.client_id, Some("github-id".to_string()));
			assert_eq!(base.google.client_id, Some("google-id".to_string()));
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
		fn github_secret_never_in_debug(
			secret in "[a-zA-Z0-9]{10,40}"
		) {
			prop_assume!(!secret.contains("REDACTED"));

			let layer = GitHubOAuthConfigLayer {
				client_secret: Some(Secret::new(secret.clone())),
				..Default::default()
			};

			let debug = format!("{layer:?}");
			prop_assert!(!debug.contains(&secret));
		}

		#[test]
		fn google_secret_never_in_debug(
			secret in "[a-zA-Z0-9]{10,40}"
		) {
			prop_assume!(!secret.contains("REDACTED"));

			let layer = GoogleOAuthConfigLayer {
				client_secret: Some(Secret::new(secret.clone())),
				..Default::default()
			};

			let debug = format!("{layer:?}");
			prop_assert!(!debug.contains(&secret));
		}

		#[test]
		fn okta_secret_never_in_debug(
			secret in "[a-zA-Z0-9]{10,40}"
		) {
			prop_assume!(!secret.contains("REDACTED"));

			let layer = OktaOAuthConfigLayer {
				client_secret: Some(Secret::new(secret.clone())),
				..Default::default()
			};

			let debug = format!("{layer:?}");
			prop_assert!(!debug.contains(&secret));
		}

		#[test]
		fn valid_github_config_builds_successfully(
			client_id in "[a-zA-Z0-9]{5,20}",
			client_secret in "[a-zA-Z0-9]{10,40}",
			redirect_uri in "https://[a-z]{3,10}\\.[a-z]{2,5}/[a-z]{3,10}",
		) {
			let layer = GitHubOAuthConfigLayer {
				client_id: Some(client_id.clone()),
				client_secret: Some(Secret::new(client_secret)),
				redirect_uri: Some(redirect_uri.clone()),
				scopes: None,
			};

			let result = layer.build();
			prop_assert!(result.is_ok());

			let config = result.unwrap().unwrap();
			prop_assert_eq!(config.client_id, client_id);
			prop_assert_eq!(config.redirect_uri, redirect_uri);
		}

		#[test]
		fn okta_issuer_url_is_correct(
			domain in "[a-z]{3,10}-[0-9]{1,6}\\.okta\\.com",
		) {
			let layer = OktaOAuthConfigLayer {
				domain: Some(domain.clone()),
				client_id: Some("test-id".to_string()),
				client_secret: Some(Secret::new("secret".to_string())),
				redirect_uri: Some("https://example.com/callback".to_string()),
				scopes: None,
			};

			let config = layer.build().unwrap().unwrap();
			let issuer = config.issuer_url();

			prop_assert!(issuer.starts_with("https://"));
			prop_assert!(issuer.contains(&domain));
			prop_assert!(issuer.ends_with("/oauth2/default"));
		}
	}
}
