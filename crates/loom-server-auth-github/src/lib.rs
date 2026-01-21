// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! GitHub OAuth 2.0 authentication for Loom.
//!
//! This module implements the GitHub OAuth 2.0 authorization code flow for authenticating
//! users via their GitHub accounts.
//!
//! # OAuth Flow
//!
//! The GitHub OAuth flow consists of four steps:
//!
//! 1. **Authorization URL Generation**: Generate a URL with a state parameter for CSRF protection.
//!    The user is redirected to GitHub to authorize the application.
//!
//! 2. **User Authorization**: The user authorizes in their browser and is redirected back
//!    to the configured `redirect_uri` with an authorization `code` and `state` parameter.
//!
//! 3. **Code Exchange**: Exchange the authorization code for an access token by calling
//!    GitHub's token endpoint with the client credentials.
//!
//! 4. **API Access**: Use the access token to fetch user information and verified email
//!    addresses from GitHub's API.
//!
//! # Example
//!
//! ```rust,no_run
//! use loom_server_auth_github::{GitHubOAuthClient, GitHubOAuthConfig};
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let config = GitHubOAuthConfig::from_env()?;
//! let client = GitHubOAuthClient::new(config);
//!
//! // Step 1: Generate authorization URL with CSRF state
//! let auth_url = client.authorization_url("random-state-value");
//!
//! // Step 2: User authorizes and is redirected back with code
//! // (handled by your web server)
//!
//! // Step 3: Exchange code for access token
//! let token = client.exchange_code("authorization-code-from-callback").await?;
//!
//! // Step 4: Fetch user info
//! let user = client.get_user(token.access_token.expose()).await?;
//! let emails = client.get_emails(token.access_token.expose()).await?;
//! # Ok(())
//! # }
//! ```
//!
//! # Security Considerations
//!
//! - The `client_secret` is wrapped in [`SecretString`] to prevent accidental logging.
//! - Access tokens in [`GitHubTokenResponse`] are also wrapped to prevent exposure.
//! - All tracing instrumentation skips sensitive parameters.
//! - Always validate the `state` parameter in callbacks to prevent CSRF attacks.

use loom_common_secret::SecretString;
use serde::{Deserialize, Serialize};
use std::env;
use url::Url;

const GITHUB_AUTHORIZE_URL: &str = "https://github.com/login/oauth/authorize";
const GITHUB_TOKEN_URL: &str = "https://github.com/login/oauth/access_token";
const GITHUB_USER_API_URL: &str = "https://api.github.com/user";
const GITHUB_EMAILS_API_URL: &str = "https://api.github.com/user/emails";

// =============================================================================
// Errors
// =============================================================================

/// Errors that can occur when loading configuration.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
	/// A required environment variable was not set.
	#[error("missing environment variable: {0}")]
	MissingEnvVar(String),

	/// A configuration value was empty or invalid.
	#[error("invalid configuration: {0}")]
	InvalidConfig(String),
}

/// Errors that can occur during OAuth operations.
#[derive(Debug, thiserror::Error)]
pub enum OAuthError {
	/// The HTTP request to GitHub failed (network error, timeout, etc.).
	#[error("HTTP request failed: {0}")]
	HttpRequest(#[from] reqwest::Error),

	/// The response from GitHub could not be parsed as expected.
	#[error("failed to parse response: {0}")]
	ParseError(String),

	/// GitHub returned an error response (invalid code, expired token, etc.).
	#[error("GitHub API error: {0}")]
	GitHubError(String),
}

// =============================================================================
// Configuration
// =============================================================================

/// Configuration for the GitHub OAuth client.
///
/// This contains all the credentials and settings needed to authenticate users
/// via GitHub OAuth. The `client_secret` is wrapped in [`SecretString`] to prevent
/// accidental logging or exposure.
///
/// # Fields
///
/// - `client_id`: The OAuth application's client ID from GitHub.
/// - `client_secret`: The OAuth application's client secret (never logged).
/// - `redirect_uri`: The callback URL registered with GitHub where users are
///   redirected after authorization.
/// - `scopes`: The OAuth scopes to request. Default scopes are `user:email`
///   (to read email addresses) and `read:user` (to read profile information).
#[derive(Debug, Clone)]
pub struct GitHubOAuthConfig {
	/// The OAuth application client ID.
	pub client_id: String,
	/// The OAuth application client secret (wrapped to prevent logging).
	pub client_secret: SecretString,
	/// The callback URL where GitHub redirects after authorization.
	pub redirect_uri: String,
	/// OAuth scopes to request (e.g., "user:email", "read:user").
	pub scopes: Vec<String>,
}

impl GitHubOAuthConfig {
	/// Load configuration from environment variables.
	///
	/// # Required Environment Variables
	///
	/// - `LOOM_SERVER_GITHUB_CLIENT_ID`: The OAuth application's client ID.
	/// - `LOOM_SERVER_GITHUB_CLIENT_SECRET`: The OAuth application's client secret.
	/// - `LOOM_SERVER_GITHUB_REDIRECT_URI`: The callback URL for OAuth redirects.
	///
	/// # Returns
	///
	/// Returns the configuration with default scopes (`user:email`, `read:user`).
	///
	/// # Errors
	///
	/// Returns [`ConfigError::MissingEnvVar`] if any required variable is not set.
	pub fn from_env() -> Result<Self, ConfigError> {
		let client_id = env::var("LOOM_SERVER_GITHUB_CLIENT_ID")
			.map_err(|_| ConfigError::MissingEnvVar("LOOM_SERVER_GITHUB_CLIENT_ID".to_string()))?;

		let client_secret = env::var("LOOM_SERVER_GITHUB_CLIENT_SECRET")
			.map_err(|_| ConfigError::MissingEnvVar("LOOM_SERVER_GITHUB_CLIENT_SECRET".to_string()))?;

		let redirect_uri = env::var("LOOM_SERVER_GITHUB_REDIRECT_URI")
			.map_err(|_| ConfigError::MissingEnvVar("LOOM_SERVER_GITHUB_REDIRECT_URI".to_string()))?;

		Ok(Self {
			client_id,
			client_secret: SecretString::new(client_secret),
			redirect_uri,
			scopes: vec!["user:email".to_string(), "read:user".to_string()],
		})
	}

	/// Validate that all configuration fields are non-empty.
	///
	/// # Errors
	///
	/// Returns [`ConfigError::InvalidConfig`] if any field is empty.
	pub fn validate(&self) -> Result<(), ConfigError> {
		if self.client_id.is_empty() {
			return Err(ConfigError::InvalidConfig(
				"client_id cannot be empty".to_string(),
			));
		}
		if self.client_secret.expose().is_empty() {
			return Err(ConfigError::InvalidConfig(
				"client_secret cannot be empty".to_string(),
			));
		}
		if self.redirect_uri.is_empty() {
			return Err(ConfigError::InvalidConfig(
				"redirect_uri cannot be empty".to_string(),
			));
		}
		Ok(())
	}

	/// Join scopes into a space-separated string for the authorization URL.
	pub fn scopes_string(&self) -> String {
		self.scopes.join(" ")
	}

	/// Parse a scope string into a vector of individual scopes.
	pub fn parse_scopes(scope_str: &str) -> Vec<String> {
		scope_str
			.split([' ', ','])
			.map(|s| s.trim().to_string())
			.filter(|s| !s.is_empty())
			.collect()
	}
}

// =============================================================================
// Response types
// =============================================================================

/// Response from GitHub's token endpoint after exchanging an authorization code.
///
/// # Fields
///
/// - `access_token`: The OAuth access token for making API requests. This is
///   wrapped in [`SecretString`] to prevent accidental logging. Use `.expose()`
///   to access the token value when making API calls.
/// - `token_type`: Always "bearer" for GitHub OAuth tokens.
/// - `scope`: Comma or space-separated list of granted scopes. May differ from
///   requested scopes if the user didn't grant all permissions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitHubTokenResponse {
	/// The access token for API requests (wrapped to prevent logging).
	#[serde(deserialize_with = "deserialize_secret_string")]
	pub access_token: SecretString,
	/// The token type (always "bearer").
	pub token_type: String,
	/// Granted OAuth scopes (comma or space-separated).
	pub scope: String,
}

fn deserialize_secret_string<'de, D>(deserializer: D) -> Result<SecretString, D::Error>
where
	D: serde::Deserializer<'de>,
{
	let s = String::deserialize(deserializer)?;
	Ok(SecretString::new(s))
}

/// User profile information from GitHub's `/user` API endpoint.
///
/// # Fields
///
/// - `id`: GitHub's unique numeric user ID (stable across username changes).
/// - `login`: The GitHub username (may change over time).
/// - `name`: The user's display name, if set. Many users leave this blank.
/// - `email`: The user's public email, if set. Use [`GitHubOAuthClient::get_emails`]
///   to get all verified emails including private ones.
/// - `avatar_url`: URL to the user's avatar image, if available.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitHubUser {
	/// GitHub's unique numeric user ID.
	pub id: i64,
	/// The GitHub username.
	pub login: String,
	/// Display name (optional, may be null).
	pub name: Option<String>,
	/// Public email address (optional, may be null).
	pub email: Option<String>,
	/// Avatar image URL (optional, may be null).
	pub avatar_url: Option<String>,
}

/// Email address information from GitHub's `/user/emails` API endpoint.
///
/// This represents a single email address associated with a GitHub account.
/// Users may have multiple email addresses; use the `primary` and `verified`
/// fields to select the appropriate one.
///
/// # Fields
///
/// - `email`: The email address string.
/// - `primary`: Whether this is the user's primary email address.
/// - `verified`: Whether GitHub has verified this email address. Always prefer
///   verified emails for security.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitHubEmail {
	/// The email address.
	pub email: String,
	/// Whether this is the primary email.
	pub primary: bool,
	/// Whether this email has been verified by GitHub.
	pub verified: bool,
}

#[derive(Debug, Deserialize)]
struct GitHubErrorResponse {
	error: String,
	error_description: Option<String>,
}

// =============================================================================
// Client
// =============================================================================

/// OAuth client for authenticating users via GitHub.
///
/// This client handles the OAuth 2.0 authorization code flow with GitHub,
/// including generating authorization URLs, exchanging codes for tokens,
/// and fetching user information.
///
/// # Example
///
/// ```rust,no_run
/// use loom_server_auth_github::{GitHubOAuthClient, GitHubOAuthConfig};
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let config = GitHubOAuthConfig::from_env()?;
/// let client = GitHubOAuthClient::new(config);
///
/// let auth_url = client.authorization_url("csrf-state");
/// // Redirect user to auth_url...
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Clone)]
pub struct GitHubOAuthClient {
	config: GitHubOAuthConfig,
	http_client: reqwest::Client,
}

impl GitHubOAuthClient {
	/// Create a new GitHub OAuth client with the given configuration.
	///
	/// # Panics
	///
	/// Panics if the HTTP client cannot be built (should never happen in practice).
	#[tracing::instrument(skip_all, name = "GitHubOAuthClient::new")]
	pub fn new(config: GitHubOAuthConfig) -> Self {
		let http_client = loom_common_http::builder()
			.build()
			.expect("failed to build HTTP client");

		Self {
			config,
			http_client,
		}
	}

	/// Generate the GitHub authorization URL for the OAuth flow.
	///
	/// The returned URL should be used to redirect the user to GitHub for
	/// authorization. After authorization, GitHub will redirect back to the
	/// configured `redirect_uri` with `code` and `state` query parameters.
	///
	/// # Arguments
	///
	/// - `state`: A random, unguessable string to prevent CSRF attacks.
	///   This value should be stored server-side and verified when the
	///   user is redirected back.
	///
	/// # Returns
	///
	/// A URL string that includes:
	/// - `client_id`: The application's OAuth client ID
	/// - `redirect_uri`: Where GitHub will redirect after authorization
	/// - `scope`: The requested OAuth scopes (space-separated)
	/// - `state`: The CSRF protection token
	#[tracing::instrument(skip(self), fields(client_id = %self.config.client_id))]
	pub fn authorization_url(&self, state: &str) -> String {
		let mut url = Url::parse(GITHUB_AUTHORIZE_URL).expect("invalid authorize URL");

		url
			.query_pairs_mut()
			.append_pair("client_id", &self.config.client_id)
			.append_pair("redirect_uri", &self.config.redirect_uri)
			.append_pair("scope", &self.config.scopes_string())
			.append_pair("state", state);

		url.to_string()
	}

	/// Exchange an authorization code for an access token.
	///
	/// After the user authorizes the application, GitHub redirects back with
	/// an authorization code. This method exchanges that code for an access
	/// token that can be used to make API requests.
	///
	/// # Arguments
	///
	/// - `code`: The authorization code from the OAuth callback.
	///
	/// # Returns
	///
	/// A [`GitHubTokenResponse`] containing the access token and granted scopes.
	///
	/// # Errors
	///
	/// - [`OAuthError::HttpRequest`]: Network error or timeout.
	/// - [`OAuthError::GitHubError`]: GitHub rejected the code (expired, invalid, etc.).
	/// - [`OAuthError::ParseError`]: Unexpected response format.
	#[tracing::instrument(skip(self, code), name = "GitHubOAuthClient::exchange_code")]
	pub async fn exchange_code(&self, code: &str) -> Result<GitHubTokenResponse, OAuthError> {
		tracing::debug!("exchanging authorization code for access token");

		let response = self
			.http_client
			.post(GITHUB_TOKEN_URL)
			.header("Accept", "application/json")
			.form(&[
				("client_id", self.config.client_id.as_str()),
				("client_secret", self.config.client_secret.expose().as_str()),
				("code", code),
				("redirect_uri", self.config.redirect_uri.as_str()),
			])
			.send()
			.await?;

		let body = response.text().await?;

		if let Ok(error_response) = serde_json::from_str::<GitHubErrorResponse>(&body) {
			if !error_response.error.is_empty() {
				let message = error_response
					.error_description
					.unwrap_or(error_response.error);
				return Err(OAuthError::GitHubError(message));
			}
		}

		serde_json::from_str(&body)
			.map_err(|e| OAuthError::ParseError(format!("failed to parse token response: {e}")))
	}

	/// Fetch the authenticated user's profile from GitHub.
	///
	/// # Arguments
	///
	/// - `access_token`: The OAuth access token from [`exchange_code`].
	///
	/// # Returns
	///
	/// A [`GitHubUser`] containing the user's profile information.
	///
	/// # Errors
	///
	/// - [`OAuthError::HttpRequest`]: Network error or timeout.
	/// - [`OAuthError::GitHubError`]: Token is invalid or expired.
	/// - [`OAuthError::ParseError`]: Unexpected response format.
	#[tracing::instrument(skip(self, access_token), name = "GitHubOAuthClient::get_user")]
	pub async fn get_user(&self, access_token: &str) -> Result<GitHubUser, OAuthError> {
		tracing::debug!("fetching GitHub user info");

		let response = self
			.http_client
			.get(GITHUB_USER_API_URL)
			.header("Accept", "application/vnd.github+json")
			.header("Authorization", format!("Bearer {access_token}"))
			.header("X-GitHub-Api-Version", "2022-11-28")
			.send()
			.await?;

		if !response.status().is_success() {
			let body = response.text().await.unwrap_or_default();
			return Err(OAuthError::GitHubError(format!(
				"failed to get user: {body}"
			)));
		}

		response
			.json()
			.await
			.map_err(|e| OAuthError::ParseError(format!("failed to parse user response: {e}")))
	}

	/// Fetch all email addresses associated with the authenticated user.
	///
	/// This returns all emails including private ones, unlike the `email` field
	/// on [`GitHubUser`] which only includes the public email.
	///
	/// # Arguments
	///
	/// - `access_token`: The OAuth access token from [`exchange_code`].
	///
	/// # Returns
	///
	/// A list of [`GitHubEmail`] entries. Look for `primary: true` and
	/// `verified: true` to find the best email to use.
	///
	/// # Errors
	///
	/// - [`OAuthError::HttpRequest`]: Network error or timeout.
	/// - [`OAuthError::GitHubError`]: Token is invalid or expired.
	/// - [`OAuthError::ParseError`]: Unexpected response format.
	#[tracing::instrument(skip(self, access_token), name = "GitHubOAuthClient::get_emails")]
	pub async fn get_emails(&self, access_token: &str) -> Result<Vec<GitHubEmail>, OAuthError> {
		tracing::debug!("fetching GitHub user emails");

		let response = self
			.http_client
			.get(GITHUB_EMAILS_API_URL)
			.header("Accept", "application/vnd.github+json")
			.header("Authorization", format!("Bearer {access_token}"))
			.header("X-GitHub-Api-Version", "2022-11-28")
			.send()
			.await?;

		if !response.status().is_success() {
			let body = response.text().await.unwrap_or_default();
			return Err(OAuthError::GitHubError(format!(
				"failed to get emails: {body}"
			)));
		}

		response
			.json()
			.await
			.map_err(|e| OAuthError::ParseError(format!("failed to parse emails response: {e}")))
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn config_default_scopes() {
		let config = GitHubOAuthConfig {
			client_id: "test_id".to_string(),
			client_secret: SecretString::new("test_secret".to_string()),
			redirect_uri: "https://example.com/callback".to_string(),
			scopes: vec!["user:email".to_string(), "read:user".to_string()],
		};

		assert_eq!(config.scopes.len(), 2);
		assert!(config.scopes.contains(&"user:email".to_string()));
		assert!(config.scopes.contains(&"read:user".to_string()));
	}

	#[test]
	fn authorization_url_contains_required_params() {
		let config = GitHubOAuthConfig {
			client_id: "test_client_id".to_string(),
			client_secret: SecretString::new("test_secret".to_string()),
			redirect_uri: "https://example.com/callback".to_string(),
			scopes: vec!["user:email".to_string(), "read:user".to_string()],
		};

		let client = GitHubOAuthClient::new(config);
		let url = client.authorization_url("test_state_123");

		assert!(url.starts_with("https://github.com/login/oauth/authorize"));
		assert!(url.contains("client_id=test_client_id"));
		assert!(url.contains("redirect_uri=https%3A%2F%2Fexample.com%2Fcallback"));
		assert!(url.contains("state=test_state_123"));
		assert!(url.contains("scope=user%3Aemail+read%3Auser"));
	}

	#[test]
	fn github_user_deserializes() {
		let json = r#"{
            "id": 12345,
            "login": "testuser",
            "name": "Test User",
            "email": "test@example.com",
            "avatar_url": "https://avatars.githubusercontent.com/u/12345"
        }"#;

		let user: GitHubUser = serde_json::from_str(json).unwrap();
		assert_eq!(user.id, 12345);
		assert_eq!(user.login, "testuser");
		assert_eq!(user.name, Some("Test User".to_string()));
		assert_eq!(user.email, Some("test@example.com".to_string()));
	}

	#[test]
	fn github_user_deserializes_with_null_fields() {
		let json = r#"{
            "id": 12345,
            "login": "testuser",
            "name": null,
            "email": null,
            "avatar_url": null
        }"#;

		let user: GitHubUser = serde_json::from_str(json).unwrap();
		assert_eq!(user.id, 12345);
		assert_eq!(user.login, "testuser");
		assert!(user.name.is_none());
		assert!(user.email.is_none());
	}

	#[test]
	fn github_email_deserializes() {
		let json = r#"{
            "email": "test@example.com",
            "primary": true,
            "verified": true
        }"#;

		let email: GitHubEmail = serde_json::from_str(json).unwrap();
		assert_eq!(email.email, "test@example.com");
		assert!(email.primary);
		assert!(email.verified);
	}

	#[test]
	fn github_token_response_deserializes() {
		let json = r#"{
            "access_token": "gho_xxxxxxxxxxxx",
            "token_type": "bearer",
            "scope": "user:email,read:user"
        }"#;

		let token: GitHubTokenResponse = serde_json::from_str(json).unwrap();
		assert_eq!(token.access_token.expose(), "gho_xxxxxxxxxxxx");
		assert_eq!(token.token_type, "bearer");
		assert_eq!(token.scope, "user:email,read:user");
	}

	#[test]
	fn config_validation_rejects_empty_fields() {
		let config = GitHubOAuthConfig {
			client_id: "".to_string(),
			client_secret: SecretString::new("secret".to_string()),
			redirect_uri: "https://example.com".to_string(),
			scopes: vec![],
		};
		assert!(config.validate().is_err());

		let config = GitHubOAuthConfig {
			client_id: "id".to_string(),
			client_secret: SecretString::new("".to_string()),
			redirect_uri: "https://example.com".to_string(),
			scopes: vec![],
		};
		assert!(config.validate().is_err());

		let config = GitHubOAuthConfig {
			client_id: "id".to_string(),
			client_secret: SecretString::new("secret".to_string()),
			redirect_uri: "".to_string(),
			scopes: vec![],
		};
		assert!(config.validate().is_err());
	}

	#[test]
	fn config_validation_accepts_valid_config() {
		let config = GitHubOAuthConfig {
			client_id: "id".to_string(),
			client_secret: SecretString::new("secret".to_string()),
			redirect_uri: "https://example.com".to_string(),
			scopes: vec!["user:email".to_string()],
		};
		assert!(config.validate().is_ok());
	}

	#[test]
	fn scopes_string_joins_with_space() {
		let config = GitHubOAuthConfig {
			client_id: "id".to_string(),
			client_secret: SecretString::new("secret".to_string()),
			redirect_uri: "https://example.com".to_string(),
			scopes: vec![
				"user:email".to_string(),
				"read:user".to_string(),
				"repo".to_string(),
			],
		};
		assert_eq!(config.scopes_string(), "user:email read:user repo");
	}

	#[test]
	fn parse_scopes_handles_various_formats() {
		assert_eq!(
			GitHubOAuthConfig::parse_scopes("user:email read:user"),
			vec!["user:email", "read:user"]
		);
		assert_eq!(
			GitHubOAuthConfig::parse_scopes("user:email,read:user"),
			vec!["user:email", "read:user"]
		);
		assert_eq!(
			GitHubOAuthConfig::parse_scopes("  user:email  ,  read:user  "),
			vec!["user:email", "read:user"]
		);
		assert!(GitHubOAuthConfig::parse_scopes("").is_empty());
		assert!(GitHubOAuthConfig::parse_scopes("   ").is_empty());
	}

	#[test]
	fn access_token_is_not_logged() {
		let json = r#"{
            "access_token": "gho_supersecrettoken",
            "token_type": "bearer",
            "scope": "user:email"
        }"#;

		let token: GitHubTokenResponse = serde_json::from_str(json).unwrap();
		let debug_output = format!("{token:?}");

		assert!(!debug_output.contains("gho_supersecrettoken"));
		assert!(debug_output.contains("[REDACTED]"));
	}

	#[test]
	fn client_secret_is_not_logged() {
		let config = GitHubOAuthConfig {
			client_id: "test_id".to_string(),
			client_secret: SecretString::new("super_secret_value".to_string()),
			redirect_uri: "https://example.com".to_string(),
			scopes: vec![],
		};
		let debug_output = format!("{config:?}");

		assert!(!debug_output.contains("super_secret_value"));
		assert!(debug_output.contains("[REDACTED]"));
	}
}

#[cfg(test)]
mod proptests {
	use super::*;
	use proptest::prelude::*;

	proptest! {
		/// Authorization URLs must always contain required OAuth parameters
		/// regardless of the input values.
		#[test]
		fn authorization_url_always_has_required_params(
			client_id in "[a-zA-Z0-9]{1,40}",
			redirect_uri in "https://[a-z]{1,20}\\.[a-z]{2,5}/[a-z]{1,20}",
			state in "[a-zA-Z0-9]{1,64}",
		) {
			let config = GitHubOAuthConfig {
				client_id: client_id.clone(),
				client_secret: SecretString::new("secret".to_string()),
				redirect_uri: redirect_uri.clone(),
				scopes: vec!["user:email".to_string()],
			};

			let client = GitHubOAuthClient::new(config);
			let url = client.authorization_url(&state);

			prop_assert!(url.starts_with(GITHUB_AUTHORIZE_URL));
			prop_assert!(url.contains("client_id="));
			prop_assert!(url.contains("redirect_uri="));
			prop_assert!(url.contains("scope="));
			prop_assert!(url.contains("state="));
		}

		/// Scope joining and parsing should roundtrip correctly.
		#[test]
		fn scope_join_and_parse_roundtrips(
			scopes in proptest::collection::vec("[a-z]{1,10}:[a-z]{1,10}", 1..5)
		) {
			let config = GitHubOAuthConfig {
				client_id: "id".to_string(),
				client_secret: SecretString::new("secret".to_string()),
				redirect_uri: "https://example.com".to_string(),
				scopes: scopes.clone(),
			};

			let joined = config.scopes_string();
			let parsed = GitHubOAuthConfig::parse_scopes(&joined);

			prop_assert_eq!(parsed, scopes);
		}

		/// Valid configurations should always pass validation.
		#[test]
		fn valid_config_passes_validation(
			client_id in "[a-zA-Z0-9]{1,40}",
			client_secret in "[a-zA-Z0-9]{1,40}",
			redirect_uri in "https://[a-z]{1,20}\\.[a-z]{2,5}/[a-z]{1,20}",
		) {
			let config = GitHubOAuthConfig {
				client_id,
				client_secret: SecretString::new(client_secret),
				redirect_uri,
				scopes: vec!["user:email".to_string()],
			};

			prop_assert!(config.validate().is_ok());
		}

		/// Empty client_id should always fail validation.
		#[test]
		fn empty_client_id_fails_validation(
			client_secret in "[a-zA-Z0-9]{1,40}",
			redirect_uri in "https://[a-z]{1,20}\\.[a-z]{2,5}",
		) {
			let config = GitHubOAuthConfig {
				client_id: "".to_string(),
				client_secret: SecretString::new(client_secret),
				redirect_uri,
				scopes: vec![],
			};

			prop_assert!(config.validate().is_err());
		}

		/// Empty client_secret should always fail validation.
		#[test]
		fn empty_client_secret_fails_validation(
			client_id in "[a-zA-Z0-9]{1,40}",
			redirect_uri in "https://[a-z]{1,20}\\.[a-z]{2,5}",
		) {
			let config = GitHubOAuthConfig {
				client_id,
				client_secret: SecretString::new("".to_string()),
				redirect_uri,
				scopes: vec![],
			};

			prop_assert!(config.validate().is_err());
		}

		/// Empty redirect_uri should always fail validation.
		#[test]
		fn empty_redirect_uri_fails_validation(
			client_id in "[a-zA-Z0-9]{1,40}",
			client_secret in "[a-zA-Z0-9]{1,40}",
		) {
			let config = GitHubOAuthConfig {
				client_id,
				client_secret: SecretString::new(client_secret),
				redirect_uri: "".to_string(),
				scopes: vec![],
			};

			prop_assert!(config.validate().is_err());
		}

		/// Client secret should never appear in debug output.
		#[test]
		fn client_secret_never_in_debug(
			secret in "[a-zA-Z0-9]{10,40}"
		) {
			prop_assume!(!secret.contains("REDACTED"));
			prop_assume!(!secret.contains("Secret"));

			let config = GitHubOAuthConfig {
				client_id: "id".to_string(),
				client_secret: SecretString::new(secret.clone()),
				redirect_uri: "https://example.com".to_string(),
				scopes: vec![],
			};

			let debug = format!("{config:?}");
			prop_assert!(!debug.contains(&secret));
		}

		/// Access token should never appear in debug output.
		#[test]
		fn access_token_never_in_debug(
			token in "gho_[a-zA-Z0-9]{10,40}"
		) {
			prop_assume!(!token.contains("REDACTED"));

			let json = format!(
				r#"{{"access_token": "{token}", "token_type": "bearer", "scope": "user:email"}}"#
			);
			let response: GitHubTokenResponse = serde_json::from_str(&json).unwrap();

			let debug = format!("{response:?}");
			prop_assert!(!debug.contains(&token));
		}
	}
}
