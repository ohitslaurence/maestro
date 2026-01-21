// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! Okta OAuth 2.0 / OpenID Connect authentication for Loom.
//!
//! This module provides enterprise Single Sign-On (SSO) authentication via Okta,
//! implementing the OAuth 2.0 / OIDC authorization code flow.
//!
//! # Overview
//!
//! Okta is an enterprise identity provider commonly used for corporate SSO. Unlike
//! consumer OAuth providers (GitHub, Google), Okta is typically configured per-organization
//! with a custom domain like `dev-123456.okta.com` or `yourcompany.okta.com`.
//!
//! # OAuth/OIDC Flow
//!
//! The authentication flow consists of four steps:
//!
//! 1. **Authorization URL Generation**: Generate a URL with `state` (CSRF protection)
//!    and `nonce` (replay protection) parameters. Redirect the user to Okta.
//!
//! 2. **User Authorization**: The user authenticates with Okta (password, MFA, etc.)
//!    and is redirected back to the configured `redirect_uri` with an authorization `code`.
//!
//! 3. **Code Exchange**: Exchange the authorization code for tokens by calling
//!    Okta's token endpoint with client credentials. Returns an `access_token`
//!    (for API calls) and an `id_token` (contains user identity claims).
//!
//! 4. **User Info**: Use the access token to fetch user information from the
//!    userinfo endpoint, including enterprise-specific claims like group memberships.
//!
//! # Domain Configuration
//!
//! Okta domains follow the pattern:
//! - Development: `dev-123456.okta.com`
//! - Preview: `yourcompany.oktapreview.com`
//! - Production: `yourcompany.okta.com`
//!
//! The domain is used to construct the issuer URL: `https://{domain}/oauth2/default`
//! where `default` refers to the default authorization server.
//!
//! # Groups Claim
//!
//! Okta can include group memberships in tokens via the `groups` claim. This is
//! useful for enterprise permissions (e.g., checking if a user belongs to "Admins"
//! or "Developers" groups). Group claims must be explicitly configured in the Okta
//! admin console under API > Authorization Servers > Claims.
//!
//! # Token Introspection
//!
//! The [`OktaOAuthClient::introspect_token`] method validates tokens server-side
//! by calling Okta's introspection endpoint. This is useful for:
//! - Checking if a token has been revoked
//! - Validating tokens from untrusted sources
//! - Getting token metadata without decoding the JWT
//!
//! # Example
//!
//! ```rust,no_run
//! use loom_server_auth_okta::{OktaOAuthClient, OktaOAuthConfig};
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let config = OktaOAuthConfig::from_env()?;
//! let client = OktaOAuthClient::new(config);
//!
//! // Step 1: Generate authorization URL with CSRF state and nonce
//! let auth_url = client.authorization_url("random-state", "random-nonce");
//!
//! // Step 2: User authorizes and is redirected back with code
//! // (handled by your web server)
//!
//! // Step 3: Exchange code for tokens
//! let tokens = client.exchange_code("authorization-code-from-callback").await?;
//!
//! // Step 4: Fetch user info including groups
//! let user = client.get_user_info(tokens.access_token.expose()).await?;
//! println!("User: {} (groups: {:?})", user.email, user.groups);
//! # Ok(())
//! # }
//! ```
//!
//! # Security Considerations
//!
//! - The `client_secret` is wrapped in [`SecretString`] to prevent accidental logging.
//! - Access tokens and ID tokens in [`OktaTokenResponse`] are also wrapped.
//! - All tracing instrumentation skips sensitive parameters.
//! - Always validate the `state` parameter in callbacks to prevent CSRF attacks.
//! - Validate the `nonce` in the ID token to prevent replay attacks.

use loom_common_secret::SecretString;
use serde::{Deserialize, Serialize};
use std::env;
use url::Url;

// =============================================================================
// Errors
// =============================================================================

/// Errors that can occur when loading Okta configuration.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
	/// A required environment variable was not set.
	#[error("missing environment variable: {0}")]
	MissingEnvVar(String),

	/// A configuration value was empty or invalid.
	#[error("invalid configuration: {0}")]
	InvalidConfig(String),
}

/// Errors that can occur during OAuth operations with Okta.
#[derive(Debug, thiserror::Error)]
pub enum OAuthError {
	/// The HTTP request to Okta failed (network error, timeout, etc.).
	#[error("HTTP request failed: {0}")]
	HttpRequest(#[from] reqwest::Error),

	/// The response from Okta could not be parsed as expected.
	#[error("failed to parse response: {0}")]
	ParseError(String),

	/// Okta returned an error response (invalid code, expired token, etc.).
	#[error("Okta API error: {0}")]
	OktaError(String),
}

// =============================================================================
// Configuration
// =============================================================================

/// Configuration for the Okta OAuth client.
///
/// This contains all the credentials and settings needed to authenticate users
/// via Okta OAuth/OIDC. The `client_secret` is wrapped in [`SecretString`] to
/// prevent accidental logging or exposure.
///
/// # Fields
///
/// - `domain`: The Okta domain (e.g., `dev-123456.okta.com`). Do not include
///   the `https://` prefix.
/// - `client_id`: The OAuth application's client ID from the Okta admin console.
/// - `client_secret`: The OAuth application's client secret (never logged).
/// - `redirect_uri`: The callback URL registered in Okta where users are
///   redirected after authorization.
/// - `scopes`: The OIDC scopes to request. Default scopes are `openid` (required
///   for OIDC), `email` (user's email), and `profile` (user's name, etc.).
#[derive(Debug, Clone)]
pub struct OktaOAuthConfig {
	/// The Okta domain (e.g., "dev-123456.okta.com").
	pub domain: String,
	/// The OAuth application client ID.
	pub client_id: String,
	/// The OAuth application client secret (wrapped to prevent logging).
	pub client_secret: SecretString,
	/// The callback URL where Okta redirects after authorization.
	pub redirect_uri: String,
	/// OIDC scopes to request (e.g., "openid", "email", "profile", "groups").
	pub scopes: Vec<String>,
}

impl OktaOAuthConfig {
	/// Load configuration from environment variables.
	///
	/// # Required Environment Variables
	///
	/// - `LOOM_SERVER_OKTA_DOMAIN`: The Okta domain (e.g., `dev-123456.okta.com`).
	/// - `LOOM_SERVER_OKTA_CLIENT_ID`: The OAuth application's client ID.
	/// - `LOOM_SERVER_OKTA_CLIENT_SECRET`: The OAuth application's client secret.
	/// - `LOOM_SERVER_OKTA_REDIRECT_URI`: The callback URL for OAuth redirects.
	///
	/// # Returns
	///
	/// Returns the configuration with default scopes (`openid`, `email`, `profile`).
	///
	/// # Errors
	///
	/// Returns [`ConfigError::MissingEnvVar`] if any required variable is not set.
	pub fn from_env() -> Result<Self, ConfigError> {
		let domain = env::var("LOOM_SERVER_OKTA_DOMAIN")
			.map_err(|_| ConfigError::MissingEnvVar("LOOM_SERVER_OKTA_DOMAIN".to_string()))?;

		let client_id = env::var("LOOM_SERVER_OKTA_CLIENT_ID")
			.map_err(|_| ConfigError::MissingEnvVar("LOOM_SERVER_OKTA_CLIENT_ID".to_string()))?;

		let client_secret = env::var("LOOM_SERVER_OKTA_CLIENT_SECRET")
			.map_err(|_| ConfigError::MissingEnvVar("LOOM_SERVER_OKTA_CLIENT_SECRET".to_string()))?;

		let redirect_uri = env::var("LOOM_SERVER_OKTA_REDIRECT_URI")
			.map_err(|_| ConfigError::MissingEnvVar("LOOM_SERVER_OKTA_REDIRECT_URI".to_string()))?;

		Ok(Self {
			domain,
			client_id,
			client_secret: SecretString::new(client_secret),
			redirect_uri,
			scopes: vec![
				"openid".to_string(),
				"email".to_string(),
				"profile".to_string(),
			],
		})
	}

	/// Construct the issuer URL from the domain.
	///
	/// The issuer URL follows the pattern `https://{domain}/oauth2/default`
	/// where `default` refers to Okta's default authorization server.
	///
	/// # Returns
	///
	/// The full issuer URL (e.g., `https://dev-123456.okta.com/oauth2/default`).
	pub fn issuer_url(&self) -> String {
		format!("https://{}/oauth2/default", self.domain)
	}

	/// Validate that all configuration fields are non-empty.
	///
	/// # Errors
	///
	/// Returns [`ConfigError::InvalidConfig`] if any field is empty.
	pub fn validate(&self) -> Result<(), ConfigError> {
		if self.domain.is_empty() {
			return Err(ConfigError::InvalidConfig(
				"domain cannot be empty".to_string(),
			));
		}
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
}

// =============================================================================
// Response types
// =============================================================================

/// Response from Okta's token endpoint after exchanging an authorization code.
///
/// # Fields
///
/// - `access_token`: The OAuth access token for making API requests. This is
///   wrapped in [`SecretString`] to prevent accidental logging. Use `.expose()`
///   to access the token value when making API calls.
/// - `id_token`: The OIDC ID token containing user identity claims as a JWT.
///   Also wrapped in [`SecretString`] as it contains sensitive user data.
/// - `token_type`: Always "Bearer" for Okta OAuth tokens.
/// - `expires_in`: Token lifetime in seconds (typically 3600).
/// - `refresh_token`: Optional refresh token if `offline_access` scope was granted.
/// - `scope`: Space-separated list of granted scopes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OktaTokenResponse {
	/// The access token for API requests (wrapped to prevent logging).
	#[serde(deserialize_with = "deserialize_secret_string")]
	pub access_token: SecretString,
	/// The OIDC ID token (wrapped to prevent logging).
	#[serde(deserialize_with = "deserialize_secret_string")]
	pub id_token: SecretString,
	/// The token type (always "Bearer").
	pub token_type: String,
	/// Token lifetime in seconds.
	pub expires_in: u64,
	/// Refresh token, if `offline_access` scope was granted.
	#[serde(default, deserialize_with = "deserialize_optional_secret_string")]
	pub refresh_token: Option<SecretString>,
	/// Granted OAuth scopes (space-separated).
	pub scope: String,
}

fn deserialize_secret_string<'de, D>(deserializer: D) -> Result<SecretString, D::Error>
where
	D: serde::Deserializer<'de>,
{
	let s = String::deserialize(deserializer)?;
	Ok(SecretString::new(s))
}

fn deserialize_optional_secret_string<'de, D>(
	deserializer: D,
) -> Result<Option<SecretString>, D::Error>
where
	D: serde::Deserializer<'de>,
{
	let opt = Option::<String>::deserialize(deserializer)?;
	Ok(opt.map(SecretString::new))
}

/// User information from Okta's userinfo endpoint.
///
/// This contains the authenticated user's profile information from Okta.
/// The available fields depend on the requested scopes and Okta configuration.
///
/// # Fields
///
/// - `sub`: The unique subject identifier for this user within Okta.
/// - `email`: The user's email address (requires `email` scope).
/// - `email_verified`: Whether the email has been verified.
/// - `name`: The user's full display name (requires `profile` scope).
/// - `preferred_username`: The user's preferred username (often their email).
/// - `given_name`: First name (requires `profile` scope).
/// - `family_name`: Last name (requires `profile` scope).
/// - `groups`: Group memberships for enterprise permissions (requires `groups`
///   scope and claims configuration in Okta admin console).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OktaUserInfo {
	/// Unique subject identifier.
	pub sub: String,
	/// User's email address.
	pub email: String,
	/// Whether the email is verified.
	pub email_verified: Option<bool>,
	/// User's full display name.
	pub name: Option<String>,
	/// User's preferred username.
	pub preferred_username: Option<String>,
	/// User's first name.
	pub given_name: Option<String>,
	/// User's last name.
	pub family_name: Option<String>,
	/// Group memberships for enterprise permissions.
	pub groups: Option<Vec<String>>,
}

/// Response from Okta's token introspection endpoint.
///
/// Token introspection allows server-side validation of tokens without
/// decoding JWTs. This is useful for checking if tokens have been revoked
/// or getting metadata about tokens from untrusted sources.
///
/// # Fields
///
/// - `active`: Whether the token is currently valid. If `false`, all other
///   fields may be absent.
/// - `sub`: The subject (user ID) the token was issued for.
/// - `exp`: Token expiration time as Unix timestamp.
/// - `iat`: Token issue time as Unix timestamp.
/// - `client_id`: The OAuth client ID the token was issued to.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OktaIntrospectResponse {
	/// Whether the token is currently active/valid.
	pub active: bool,
	/// Subject (user ID) the token was issued for.
	pub sub: Option<String>,
	/// Expiration time as Unix timestamp.
	pub exp: Option<u64>,
	/// Issue time as Unix timestamp.
	pub iat: Option<u64>,
	/// Client ID the token was issued to.
	pub client_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OktaErrorResponse {
	error: String,
	error_description: Option<String>,
}

// =============================================================================
// Client
// =============================================================================

/// OAuth client for authenticating users via Okta.
///
/// This client handles the OAuth 2.0 / OIDC authorization code flow with Okta,
/// including generating authorization URLs, exchanging codes for tokens,
/// fetching user information, and token introspection.
///
/// # Example
///
/// ```rust,no_run
/// use loom_server_auth_okta::{OktaOAuthClient, OktaOAuthConfig};
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let config = OktaOAuthConfig::from_env()?;
/// let client = OktaOAuthClient::new(config);
///
/// let auth_url = client.authorization_url("csrf-state", "replay-nonce");
/// // Redirect user to auth_url...
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Clone)]
pub struct OktaOAuthClient {
	config: OktaOAuthConfig,
	http_client: reqwest::Client,
}

impl OktaOAuthClient {
	/// Create a new Okta OAuth client with the given configuration.
	///
	/// # Panics
	///
	/// Panics if the HTTP client cannot be built (should never happen in practice).
	#[tracing::instrument(skip_all, name = "OktaOAuthClient::new")]
	pub fn new(config: OktaOAuthConfig) -> Self {
		let http_client = loom_common_http::builder()
			.build()
			.expect("failed to build HTTP client");

		Self {
			config,
			http_client,
		}
	}

	fn authorization_endpoint(&self) -> String {
		format!("{}/v1/authorize", self.config.issuer_url())
	}

	fn token_endpoint(&self) -> String {
		format!("{}/v1/token", self.config.issuer_url())
	}

	fn userinfo_endpoint(&self) -> String {
		format!("{}/v1/userinfo", self.config.issuer_url())
	}

	fn introspect_endpoint(&self) -> String {
		format!("{}/v1/introspect", self.config.issuer_url())
	}

	/// Generate the Okta authorization URL for the OAuth/OIDC flow.
	///
	/// The returned URL should be used to redirect the user to Okta for
	/// authentication. After authorization, Okta will redirect back to the
	/// configured `redirect_uri` with `code` and `state` query parameters.
	///
	/// # Arguments
	///
	/// - `state`: A random, unguessable string to prevent CSRF attacks.
	///   This value should be stored server-side and verified when the
	///   user is redirected back.
	/// - `nonce`: A random string to prevent replay attacks. This value
	///   should be validated in the returned ID token's `nonce` claim.
	///
	/// # Returns
	///
	/// A URL string that includes:
	/// - `client_id`: The application's OAuth client ID
	/// - `redirect_uri`: Where Okta will redirect after authorization
	/// - `response_type`: Set to "code" for authorization code flow
	/// - `scope`: The requested OIDC scopes (space-separated)
	/// - `state`: The CSRF protection token
	/// - `nonce`: The replay protection token
	#[tracing::instrument(skip(self), fields(client_id = %self.config.client_id))]
	pub fn authorization_url(&self, state: &str, nonce: &str) -> String {
		let mut url =
			Url::parse(&self.authorization_endpoint()).expect("invalid authorization endpoint");

		url
			.query_pairs_mut()
			.append_pair("client_id", &self.config.client_id)
			.append_pair("redirect_uri", &self.config.redirect_uri)
			.append_pair("response_type", "code")
			.append_pair("scope", &self.config.scopes.join(" "))
			.append_pair("state", state)
			.append_pair("nonce", nonce);

		url.to_string()
	}

	fn basic_auth_header(&self) -> String {
		let credentials = format!(
			"{}:{}",
			self.config.client_id,
			self.config.client_secret.expose()
		);
		let encoded = base64::Engine::encode(
			&base64::engine::general_purpose::STANDARD,
			credentials.as_bytes(),
		);
		format!("Basic {encoded}")
	}

	/// Exchange an authorization code for access and ID tokens.
	///
	/// After the user authorizes the application, Okta redirects back with
	/// an authorization code. This method exchanges that code for tokens
	/// that can be used to access user information and APIs.
	///
	/// # Arguments
	///
	/// - `code`: The authorization code from the OAuth callback.
	///
	/// # Returns
	///
	/// An [`OktaTokenResponse`] containing the access token, ID token, and
	/// optional refresh token.
	///
	/// # Errors
	///
	/// - [`OAuthError::HttpRequest`]: Network error or timeout.
	/// - [`OAuthError::OktaError`]: Okta rejected the code (expired, invalid, etc.).
	/// - [`OAuthError::ParseError`]: Unexpected response format.
	#[tracing::instrument(skip(self, code), name = "OktaOAuthClient::exchange_code")]
	pub async fn exchange_code(&self, code: &str) -> Result<OktaTokenResponse, OAuthError> {
		tracing::debug!("exchanging authorization code for tokens");

		let response = self
			.http_client
			.post(self.token_endpoint())
			.header("Accept", "application/json")
			.header("Content-Type", "application/x-www-form-urlencoded")
			.header("Authorization", self.basic_auth_header())
			.form(&[
				("grant_type", "authorization_code"),
				("code", code),
				("redirect_uri", &self.config.redirect_uri),
			])
			.send()
			.await?;

		let body = response.text().await?;

		if let Ok(error_response) = serde_json::from_str::<OktaErrorResponse>(&body) {
			if !error_response.error.is_empty() {
				let message = error_response
					.error_description
					.unwrap_or(error_response.error);
				return Err(OAuthError::OktaError(message));
			}
		}

		serde_json::from_str(&body)
			.map_err(|e| OAuthError::ParseError(format!("failed to parse token response: {e}")))
	}

	/// Fetch the authenticated user's profile from Okta's userinfo endpoint.
	///
	/// # Arguments
	///
	/// - `access_token`: The OAuth access token from [`exchange_code`].
	///
	/// # Returns
	///
	/// An [`OktaUserInfo`] containing the user's profile information,
	/// including group memberships if the `groups` scope was requested
	/// and configured in Okta.
	///
	/// # Errors
	///
	/// - [`OAuthError::HttpRequest`]: Network error or timeout.
	/// - [`OAuthError::OktaError`]: Token is invalid or expired.
	/// - [`OAuthError::ParseError`]: Unexpected response format.
	#[tracing::instrument(skip(self, access_token), name = "OktaOAuthClient::get_user_info")]
	pub async fn get_user_info(&self, access_token: &str) -> Result<OktaUserInfo, OAuthError> {
		tracing::debug!("fetching Okta user info");

		let response = self
			.http_client
			.get(self.userinfo_endpoint())
			.header("Accept", "application/json")
			.header("Authorization", format!("Bearer {access_token}"))
			.send()
			.await?;

		if !response.status().is_success() {
			let body = response.text().await.unwrap_or_default();
			return Err(OAuthError::OktaError(format!(
				"failed to get user info: {body}"
			)));
		}

		response
			.json()
			.await
			.map_err(|e| OAuthError::ParseError(format!("failed to parse user info response: {e}")))
	}

	/// Introspect a token to check its validity server-side.
	///
	/// Token introspection calls Okta's introspection endpoint to validate
	/// a token without decoding the JWT locally. This is useful for:
	///
	/// - Checking if a token has been revoked
	/// - Validating tokens from untrusted sources
	/// - Getting token metadata (expiration, subject, etc.)
	///
	/// # Arguments
	///
	/// - `token`: The access token to introspect.
	///
	/// # Returns
	///
	/// An [`OktaIntrospectResponse`] with `active: true` if the token is valid,
	/// or `active: false` if the token is expired, revoked, or invalid.
	///
	/// # Errors
	///
	/// - [`OAuthError::HttpRequest`]: Network error or timeout.
	/// - [`OAuthError::OktaError`]: Introspection endpoint error.
	/// - [`OAuthError::ParseError`]: Unexpected response format.
	#[tracing::instrument(skip(self, token), name = "OktaOAuthClient::introspect_token")]
	pub async fn introspect_token(&self, token: &str) -> Result<OktaIntrospectResponse, OAuthError> {
		tracing::debug!("introspecting token");

		let response = self
			.http_client
			.post(self.introspect_endpoint())
			.header("Accept", "application/json")
			.header("Content-Type", "application/x-www-form-urlencoded")
			.header("Authorization", self.basic_auth_header())
			.form(&[("token", token), ("token_type_hint", "access_token")])
			.send()
			.await?;

		if !response.status().is_success() {
			let body = response.text().await.unwrap_or_default();
			return Err(OAuthError::OktaError(format!(
				"failed to introspect token: {body}"
			)));
		}

		response
			.json()
			.await
			.map_err(|e| OAuthError::ParseError(format!("failed to parse introspect response: {e}")))
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	fn test_config() -> OktaOAuthConfig {
		OktaOAuthConfig {
			domain: "dev-123456.okta.com".to_string(),
			client_id: "test_id".to_string(),
			client_secret: SecretString::new("test_secret".to_string()),
			redirect_uri: "https://example.com/callback".to_string(),
			scopes: vec![
				"openid".to_string(),
				"email".to_string(),
				"profile".to_string(),
			],
		}
	}

	#[test]
	fn config_default_scopes() {
		let config = test_config();
		assert_eq!(config.scopes.len(), 3);
		assert!(config.scopes.contains(&"openid".to_string()));
		assert!(config.scopes.contains(&"email".to_string()));
		assert!(config.scopes.contains(&"profile".to_string()));
	}

	#[test]
	fn issuer_url_is_correct() {
		let config = test_config();
		assert_eq!(
			config.issuer_url(),
			"https://dev-123456.okta.com/oauth2/default"
		);
	}

	#[test]
	fn authorization_url_contains_required_params() {
		let mut config = test_config();
		config.client_id = "test_client_id".to_string();
		let client = OktaOAuthClient::new(config);
		let url = client.authorization_url("test_state_123", "test_nonce_456");

		assert!(url.starts_with("https://dev-123456.okta.com/oauth2/default/v1/authorize"));
		assert!(url.contains("client_id=test_client_id"));
		assert!(url.contains("redirect_uri=https%3A%2F%2Fexample.com%2Fcallback"));
		assert!(url.contains("response_type=code"));
		assert!(url.contains("state=test_state_123"));
		assert!(url.contains("nonce=test_nonce_456"));
		assert!(url.contains("scope=openid+email+profile"));
	}

	#[test]
	fn okta_token_response_deserializes() {
		let json = r#"{
            "access_token": "eyJhbGciOiJSUzI1NiJ9...",
            "id_token": "eyJhbGciOiJSUzI1NiJ9...",
            "token_type": "Bearer",
            "expires_in": 3600,
            "refresh_token": "refresh_xxx",
            "scope": "openid email profile"
        }"#;

		let token: OktaTokenResponse = serde_json::from_str(json).unwrap();
		assert_eq!(token.access_token.expose(), "eyJhbGciOiJSUzI1NiJ9...");
		assert_eq!(token.id_token.expose(), "eyJhbGciOiJSUzI1NiJ9...");
		assert_eq!(token.token_type, "Bearer");
		assert_eq!(token.expires_in, 3600);
		assert_eq!(
			token.refresh_token.as_ref().map(|s| s.expose().as_str()),
			Some("refresh_xxx")
		);
		assert_eq!(token.scope, "openid email profile");
	}

	#[test]
	fn okta_token_response_deserializes_without_refresh_token() {
		let json = r#"{
            "access_token": "eyJhbGciOiJSUzI1NiJ9...",
            "id_token": "eyJhbGciOiJSUzI1NiJ9...",
            "token_type": "Bearer",
            "expires_in": 3600,
            "scope": "openid email profile"
        }"#;

		let token: OktaTokenResponse = serde_json::from_str(json).unwrap();
		assert!(token.refresh_token.is_none());
	}

	#[test]
	fn okta_user_info_deserializes() {
		let json = r#"{
            "sub": "00uid4BxXw6I6TV4m0g3",
            "email": "test@example.com",
            "email_verified": true,
            "name": "Test User",
            "preferred_username": "testuser",
            "given_name": "Test",
            "family_name": "User",
            "groups": ["Everyone", "Admins"]
        }"#;

		let user: OktaUserInfo = serde_json::from_str(json).unwrap();
		assert_eq!(user.sub, "00uid4BxXw6I6TV4m0g3");
		assert_eq!(user.email, "test@example.com");
		assert_eq!(user.email_verified, Some(true));
		assert_eq!(user.name, Some("Test User".to_string()));
		assert_eq!(user.preferred_username, Some("testuser".to_string()));
		assert_eq!(user.given_name, Some("Test".to_string()));
		assert_eq!(user.family_name, Some("User".to_string()));
		assert_eq!(
			user.groups,
			Some(vec!["Everyone".to_string(), "Admins".to_string()])
		);
	}

	#[test]
	fn okta_user_info_deserializes_minimal() {
		let json = r#"{
            "sub": "00uid4BxXw6I6TV4m0g3",
            "email": "test@example.com"
        }"#;

		let user: OktaUserInfo = serde_json::from_str(json).unwrap();
		assert_eq!(user.sub, "00uid4BxXw6I6TV4m0g3");
		assert_eq!(user.email, "test@example.com");
		assert!(user.email_verified.is_none());
		assert!(user.name.is_none());
		assert!(user.groups.is_none());
	}

	#[test]
	fn okta_introspect_response_deserializes_active() {
		let json = r#"{
            "active": true,
            "sub": "00uid4BxXw6I6TV4m0g3",
            "exp": 1735590000,
            "iat": 1735586400,
            "client_id": "0oa1hladp7lnKGBXGT0g4"
        }"#;

		let response: OktaIntrospectResponse = serde_json::from_str(json).unwrap();
		assert!(response.active);
		assert_eq!(response.sub, Some("00uid4BxXw6I6TV4m0g3".to_string()));
		assert_eq!(response.exp, Some(1735590000));
		assert_eq!(response.iat, Some(1735586400));
		assert_eq!(
			response.client_id,
			Some("0oa1hladp7lnKGBXGT0g4".to_string())
		);
	}

	#[test]
	fn okta_introspect_response_deserializes_inactive() {
		let json = r#"{
            "active": false
        }"#;

		let response: OktaIntrospectResponse = serde_json::from_str(json).unwrap();
		assert!(!response.active);
		assert!(response.sub.is_none());
		assert!(response.exp.is_none());
	}

	#[test]
	fn config_validation_passes_for_valid_config() {
		let config = test_config();
		assert!(config.validate().is_ok());
	}

	#[test]
	fn config_validation_fails_for_empty_domain() {
		let mut config = test_config();
		config.domain = "".to_string();
		assert!(config.validate().is_err());
	}

	#[test]
	fn config_validation_fails_for_empty_client_id() {
		let mut config = test_config();
		config.client_id = "".to_string();
		assert!(config.validate().is_err());
	}

	#[test]
	fn config_validation_fails_for_empty_client_secret() {
		let config = OktaOAuthConfig {
			domain: "dev-123456.okta.com".to_string(),
			client_id: "test_id".to_string(),
			client_secret: SecretString::new("".to_string()),
			redirect_uri: "https://example.com/callback".to_string(),
			scopes: vec!["openid".to_string()],
		};
		assert!(config.validate().is_err());
	}

	#[test]
	fn config_validation_fails_for_empty_redirect_uri() {
		let mut config = test_config();
		config.redirect_uri = "".to_string();
		assert!(config.validate().is_err());
	}

	#[test]
	fn client_secret_not_in_debug_output() {
		let config = OktaOAuthConfig {
			domain: "dev-123456.okta.com".to_string(),
			client_id: "test_id".to_string(),
			client_secret: SecretString::new("super_secret_value".to_string()),
			redirect_uri: "https://example.com/callback".to_string(),
			scopes: vec!["openid".to_string()],
		};

		let debug_output = format!("{config:?}");
		assert!(!debug_output.contains("super_secret_value"));
		assert!(debug_output.contains("[REDACTED]"));
	}

	#[test]
	fn access_token_not_in_debug_output() {
		let json = r#"{
            "access_token": "eyJhbGciOiJSUzI1NiJ9_secret_token",
            "id_token": "eyJhbGciOiJSUzI1NiJ9_secret_id",
            "token_type": "Bearer",
            "expires_in": 3600,
            "scope": "openid"
        }"#;

		let token: OktaTokenResponse = serde_json::from_str(json).unwrap();
		let debug_output = format!("{token:?}");

		assert!(!debug_output.contains("_secret_token"));
		assert!(!debug_output.contains("_secret_id"));
		assert!(debug_output.contains("[REDACTED]"));
	}
}

#[cfg(test)]
mod proptests {
	use super::*;
	use proptest::prelude::*;

	proptest! {
		/// Issuer URLs must follow the pattern https://{domain}/oauth2/default
		#[test]
		fn issuer_url_construction_is_correct(
			domain in "[a-z]{3,10}-[0-9]{1,6}\\.okta\\.com"
		) {
			let config = OktaOAuthConfig {
				domain: domain.clone(),
				client_id: "test".to_string(),
				client_secret: SecretString::new("secret".to_string()),
				redirect_uri: "https://example.com".to_string(),
				scopes: vec!["openid".to_string()],
			};

			let issuer = config.issuer_url();
			prop_assert!(issuer.starts_with("https://"));
			prop_assert!(issuer.contains(&domain));
			prop_assert!(issuer.ends_with("/oauth2/default"));
		}

		/// Authorization URLs must always contain required OAuth/OIDC parameters.
		#[test]
		fn authorization_url_always_has_required_params(
			domain in "[a-z]{3,10}\\.okta\\.com",
			client_id in "[a-zA-Z0-9]{1,40}",
			redirect_uri in "https://[a-z]{1,20}\\.[a-z]{2,5}/[a-z]{1,20}",
			state in "[a-zA-Z0-9]{1,64}",
			nonce in "[a-zA-Z0-9]{1,64}",
		) {
			let config = OktaOAuthConfig {
				domain,
				client_id: client_id.clone(),
				client_secret: SecretString::new("secret".to_string()),
				redirect_uri: redirect_uri.clone(),
				scopes: vec!["openid".to_string()],
			};

			let client = OktaOAuthClient::new(config);
			let url = client.authorization_url(&state, &nonce);

			prop_assert!(url.contains("/v1/authorize"));
			prop_assert!(url.contains("client_id="));
			prop_assert!(url.contains("redirect_uri="));
			prop_assert!(url.contains("response_type=code"));
			prop_assert!(url.contains("scope="));
			prop_assert!(url.contains("state="));
			prop_assert!(url.contains("nonce="));
		}

		/// Valid configurations should always pass validation.
		#[test]
		fn valid_config_passes_validation(
			domain in "[a-z]{3,10}\\.okta\\.com",
			client_id in "[a-zA-Z0-9]{1,40}",
			client_secret in "[a-zA-Z0-9]{1,40}",
			redirect_uri in "https://[a-z]{1,20}\\.[a-z]{2,5}/[a-z]{1,20}",
		) {
			let config = OktaOAuthConfig {
				domain,
				client_id,
				client_secret: SecretString::new(client_secret),
				redirect_uri,
				scopes: vec!["openid".to_string()],
			};

			prop_assert!(config.validate().is_ok());
		}

		/// Empty domain should always fail validation.
		#[test]
		fn empty_domain_fails_validation(
			client_id in "[a-zA-Z0-9]{1,40}",
			client_secret in "[a-zA-Z0-9]{1,40}",
			redirect_uri in "https://[a-z]{1,20}\\.[a-z]{2,5}",
		) {
			let config = OktaOAuthConfig {
				domain: "".to_string(),
				client_id,
				client_secret: SecretString::new(client_secret),
				redirect_uri,
				scopes: vec![],
			};

			prop_assert!(config.validate().is_err());
		}

		/// Empty client_id should always fail validation.
		#[test]
		fn empty_client_id_fails_validation(
			domain in "[a-z]{3,10}\\.okta\\.com",
			client_secret in "[a-zA-Z0-9]{1,40}",
			redirect_uri in "https://[a-z]{1,20}\\.[a-z]{2,5}",
		) {
			let config = OktaOAuthConfig {
				domain,
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
			domain in "[a-z]{3,10}\\.okta\\.com",
			client_id in "[a-zA-Z0-9]{1,40}",
			redirect_uri in "https://[a-z]{1,20}\\.[a-z]{2,5}",
		) {
			let config = OktaOAuthConfig {
				domain,
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
			domain in "[a-z]{3,10}\\.okta\\.com",
			client_id in "[a-zA-Z0-9]{1,40}",
			client_secret in "[a-zA-Z0-9]{1,40}",
		) {
			let config = OktaOAuthConfig {
				domain,
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

			let config = OktaOAuthConfig {
				domain: "dev-123456.okta.com".to_string(),
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
			token in "eyJ[a-zA-Z0-9]{10,40}"
		) {
			prop_assume!(!token.contains("REDACTED"));

			let json = format!(
				r#"{{"access_token": "{token}", "id_token": "eyJtest", "token_type": "Bearer", "expires_in": 3600, "scope": "openid"}}"#
			);
			let response: OktaTokenResponse = serde_json::from_str(&json).unwrap();

			let debug = format!("{response:?}");
			prop_assert!(!debug.contains(&token));
		}

		/// ID token should never appear in debug output.
		#[test]
		fn id_token_never_in_debug(
			token in "eyJ[a-zA-Z0-9]{10,40}"
		) {
			prop_assume!(!token.contains("REDACTED"));

			let json = format!(
				r#"{{"access_token": "eyJtest", "id_token": "{token}", "token_type": "Bearer", "expires_in": 3600, "scope": "openid"}}"#
			);
			let response: OktaTokenResponse = serde_json::from_str(&json).unwrap();

			let debug = format!("{response:?}");
			prop_assert!(!debug.contains(&token));
		}
	}
}
