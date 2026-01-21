// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! Google OAuth 2.0 / OpenID Connect authentication for Loom.
//!
//! This module implements the Google OAuth 2.0 authorization code flow with OpenID Connect
//! (OIDC) for authenticating users via their Google accounts.
//!
//! # OAuth/OIDC Flow
//!
//! The Google OAuth/OIDC flow consists of four steps:
//!
//! 1. **Authorization URL Generation**: Generate a URL with `state` and `nonce` parameters.
//!    The `state` parameter provides CSRF protection, while the `nonce` is included in the
//!    ID token to prevent replay attacks. The user is redirected to Google to authorize.
//!
//! 2. **User Authorization**: The user authorizes in their browser and is redirected back
//!    to the configured `redirect_uri` with an authorization `code` and `state` parameter.
//!
//! 3. **Code Exchange**: Exchange the authorization code for tokens by calling Google's
//!    token endpoint. This returns both an `access_token` (for API access) and an `id_token`
//!    (a signed JWT containing user identity claims).
//!
//! 4. **Identity Verification**: Either decode the ID token to extract user claims directly,
//!    or use the access token to fetch user info from Google's userinfo endpoint.
//!
//! # ID Token vs UserInfo
//!
//! Google provides two ways to get user identity:
//!
//! - **ID Token**: A signed JWT containing identity claims (sub, email, name, etc.).
//!   This is the preferred method as it doesn't require an additional API call and
//!   the signature can be verified for authenticity.
//!
//! - **UserInfo Endpoint**: An API endpoint that returns user profile information.
//!   Requires an additional HTTP request but may contain more fields.
//!
//! # Example
//!
//! ```rust,no_run
//! use loom_server_auth_google::{GoogleOAuthClient, GoogleOAuthConfig};
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let config = GoogleOAuthConfig::from_env()?;
//! let client = GoogleOAuthClient::new(config);
//!
//! // Step 1: Generate authorization URL with CSRF state and replay-protection nonce
//! let auth_url = client.authorization_url("random-state", "random-nonce");
//!
//! // Step 2: User authorizes and is redirected back with code
//! // (handled by your web server)
//!
//! // Step 3: Exchange code for tokens
//! let tokens = client.exchange_code("authorization-code-from-callback").await?;
//!
//! // Step 4a: Decode ID token for user claims (no extra API call)
//! let claims = client.decode_id_token(tokens.id_token.expose())?;
//! println!("User email: {:?}", claims.email);
//!
//! // Step 4b: Or fetch from userinfo endpoint
//! let user_info = client.get_user_info(tokens.access_token.expose()).await?;
//! # Ok(())
//! # }
//! ```
//!
//! # Security Considerations
//!
//! - The `client_secret` is wrapped in [`SecretString`] to prevent accidental logging.
//! - Access tokens and ID tokens in [`GoogleTokenResponse`] are also wrapped to prevent exposure.
//! - All tracing instrumentation skips sensitive parameters.
//! - Always validate the `state` parameter in callbacks to prevent CSRF attacks.
//! - Validate the `nonce` claim in the ID token matches what was sent to prevent replay attacks.

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use loom_common_secret::SecretString;
use serde::{Deserialize, Serialize};
use std::env;
use url::Url;

const GOOGLE_AUTHORIZE_URL: &str = "https://accounts.google.com/o/oauth2/v2/auth";
const GOOGLE_TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
const GOOGLE_USERINFO_URL: &str = "https://openidconnect.googleapis.com/v1/userinfo";

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
	/// The HTTP request to Google failed (network error, timeout, etc.).
	#[error("HTTP request failed: {0}")]
	HttpRequest(#[from] reqwest::Error),

	/// The response from Google could not be parsed as expected.
	#[error("failed to parse response: {0}")]
	ParseError(String),

	/// Google returned an error response (invalid code, expired token, etc.).
	#[error("Google API error: {0}")]
	GoogleError(String),

	/// The ID token could not be decoded or parsed.
	///
	/// This can occur if:
	/// - The token doesn't have the expected 3-part JWT structure
	/// - The payload is not valid base64
	/// - The payload doesn't contain valid JSON claims
	#[error("invalid ID token: {0}")]
	InvalidIdToken(String),
}

// =============================================================================
// Configuration
// =============================================================================

/// Configuration for the Google OAuth client.
///
/// This contains all the credentials and settings needed to authenticate users
/// via Google OAuth/OIDC. The `client_secret` is wrapped in [`SecretString`] to
/// prevent accidental logging or exposure.
///
/// # Fields
///
/// - `client_id`: The OAuth application's client ID from Google Cloud Console.
/// - `client_secret`: The OAuth application's client secret (never logged).
/// - `redirect_uri`: The callback URL registered with Google where users are
///   redirected after authorization.
/// - `scopes`: The OAuth/OIDC scopes to request. Must include `openid` for OIDC.
///   Common scopes are `email` and `profile`.
#[derive(Debug, Clone)]
pub struct GoogleOAuthConfig {
	/// The OAuth application client ID.
	pub client_id: String,
	/// The OAuth application client secret (wrapped to prevent logging).
	pub client_secret: SecretString,
	/// The callback URL where Google redirects after authorization.
	pub redirect_uri: String,
	/// OAuth scopes to request (must include "openid" for OIDC).
	pub scopes: Vec<String>,
}

impl GoogleOAuthConfig {
	/// Load configuration from environment variables.
	///
	/// # Required Environment Variables
	///
	/// - `LOOM_SERVER_GOOGLE_CLIENT_ID`: The OAuth application's client ID.
	/// - `LOOM_SERVER_GOOGLE_CLIENT_SECRET`: The OAuth application's client secret.
	/// - `LOOM_SERVER_GOOGLE_REDIRECT_URI`: The callback URL for OAuth redirects.
	///
	/// # Returns
	///
	/// Returns the configuration with default scopes (`openid`, `email`, `profile`).
	///
	/// # Errors
	///
	/// Returns [`ConfigError::MissingEnvVar`] if any required variable is not set.
	pub fn from_env() -> Result<Self, ConfigError> {
		let client_id = env::var("LOOM_SERVER_GOOGLE_CLIENT_ID")
			.map_err(|_| ConfigError::MissingEnvVar("LOOM_SERVER_GOOGLE_CLIENT_ID".to_string()))?;

		let client_secret = env::var("LOOM_SERVER_GOOGLE_CLIENT_SECRET")
			.map_err(|_| ConfigError::MissingEnvVar("LOOM_SERVER_GOOGLE_CLIENT_SECRET".to_string()))?;

		let redirect_uri = env::var("LOOM_SERVER_GOOGLE_REDIRECT_URI")
			.map_err(|_| ConfigError::MissingEnvVar("LOOM_SERVER_GOOGLE_REDIRECT_URI".to_string()))?;

		Ok(Self {
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
	///
	/// Handles both space-separated and comma-separated scope lists.
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

/// Response from Google's token endpoint after exchanging an authorization code.
///
/// This response contains both an OAuth access token and an OpenID Connect ID token.
///
/// # Fields
///
/// - `access_token`: The OAuth access token for making API requests to Google.
///   Use this to call the userinfo endpoint or other Google APIs. Wrapped in
///   [`SecretString`] to prevent accidental logging.
///
/// - `id_token`: A signed JWT containing identity claims about the user.
///   This can be decoded to get user information without an additional API call.
///   The token contains claims like `sub`, `email`, `name`, and importantly the
///   `nonce` claim which should be validated to prevent replay attacks.
///   Wrapped in [`SecretString`] to prevent accidental logging.
///
/// - `token_type`: Always "Bearer" for Google OAuth tokens.
///
/// - `expires_in`: Token lifetime in seconds (typically 3600 = 1 hour).
///
/// - `refresh_token`: Optional token for obtaining new access tokens without
///   user interaction. Only returned when `access_type=offline` is requested
///   and the user consents (usually on first authorization only).
///
/// - `scope`: Space-separated list of granted scopes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoogleTokenResponse {
	/// The access token for API requests (wrapped to prevent logging).
	#[serde(deserialize_with = "deserialize_secret_string")]
	pub access_token: SecretString,
	/// The OIDC ID token containing user identity claims (wrapped to prevent logging).
	#[serde(deserialize_with = "deserialize_secret_string")]
	pub id_token: SecretString,
	/// The token type (always "Bearer").
	pub token_type: String,
	/// Token lifetime in seconds.
	pub expires_in: u64,
	/// Refresh token for obtaining new access tokens (optional).
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
	let opt: Option<String> = Option::deserialize(deserializer)?;
	Ok(opt.map(SecretString::new))
}

/// User profile information from Google's userinfo API endpoint.
///
/// This is returned by the `/v1/userinfo` endpoint when called with a valid
/// access token. For most use cases, the claims in the ID token are sufficient
/// and avoid an extra API call.
///
/// # Fields
///
/// - `sub`: Google's unique user identifier (stable across all Google services).
/// - `email`: The user's email address.
/// - `email_verified`: Whether Google has verified this email address.
/// - `name`: The user's full display name (optional).
/// - `picture`: URL to the user's profile picture (optional).
/// - `given_name`: The user's first name (optional).
/// - `family_name`: The user's last name (optional).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoogleUserInfo {
	/// Google's unique user identifier.
	pub sub: String,
	/// The user's email address.
	pub email: String,
	/// Whether the email has been verified by Google.
	pub email_verified: bool,
	/// The user's full display name (optional).
	pub name: Option<String>,
	/// URL to the user's profile picture (optional).
	pub picture: Option<String>,
	/// The user's first name (optional).
	pub given_name: Option<String>,
	/// The user's last name (optional).
	pub family_name: Option<String>,
}

/// Claims from a decoded Google ID token (JWT payload).
///
/// The ID token is a signed JWT that contains identity claims about the user.
/// Unlike the userinfo endpoint, this doesn't require an additional API call
/// since the token is returned directly from the token exchange.
///
/// # Standard OIDC Claims
///
/// - `iss`: Token issuer, always "https://accounts.google.com".
/// - `sub`: Subject identifier - Google's unique user ID.
/// - `aud`: Audience - should match your client_id.
/// - `exp`: Expiration time (Unix timestamp).
/// - `iat`: Issued-at time (Unix timestamp).
///
/// # Google-Specific Claims
///
/// - `email`: The user's email address (requires `email` scope).
/// - `email_verified`: Whether Google has verified the email.
/// - `name`: The user's display name (requires `profile` scope).
/// - `picture`: URL to profile picture (requires `profile` scope).
///
/// # Nonce Validation
///
/// The `nonce` claim (not included in this struct but present in the raw token)
/// should be validated against the nonce sent in the authorization request to
/// prevent replay attacks. This is critical for security.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoogleIdTokenClaims {
	/// Token issuer (always "https://accounts.google.com").
	pub iss: String,
	/// Subject identifier - Google's unique user ID.
	pub sub: String,
	/// Audience - should match your client_id.
	pub aud: String,
	/// Expiration time (Unix timestamp).
	pub exp: u64,
	/// Issued-at time (Unix timestamp).
	pub iat: u64,
	/// The user's email address (optional, requires email scope).
	pub email: Option<String>,
	/// Whether the email has been verified (optional).
	pub email_verified: Option<bool>,
	/// The user's display name (optional, requires profile scope).
	pub name: Option<String>,
	/// URL to the user's profile picture (optional).
	pub picture: Option<String>,
	/// The nonce value sent in the authorization request (for replay protection).
	pub nonce: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GoogleErrorResponse {
	error: String,
	error_description: Option<String>,
}

// =============================================================================
// Client
// =============================================================================

/// OAuth client for authenticating users via Google.
///
/// This client handles the OAuth 2.0 / OpenID Connect authorization code flow
/// with Google, including generating authorization URLs, exchanging codes for
/// tokens, decoding ID tokens, and fetching user information.
///
/// # Example
///
/// ```rust,no_run
/// use loom_server_auth_google::{GoogleOAuthClient, GoogleOAuthConfig};
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let config = GoogleOAuthConfig::from_env()?;
/// let client = GoogleOAuthClient::new(config);
///
/// let auth_url = client.authorization_url("csrf-state", "replay-nonce");
/// // Redirect user to auth_url...
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Clone)]
pub struct GoogleOAuthClient {
	config: GoogleOAuthConfig,
	http_client: reqwest::Client,
}

impl GoogleOAuthClient {
	/// Create a new Google OAuth client with the given configuration.
	///
	/// # Panics
	///
	/// Panics if the HTTP client cannot be built (should never happen in practice).
	#[tracing::instrument(skip_all, name = "GoogleOAuthClient::new")]
	pub fn new(config: GoogleOAuthConfig) -> Self {
		let http_client = loom_common_http::builder()
			.build()
			.expect("failed to build HTTP client");

		Self {
			config,
			http_client,
		}
	}

	/// Generate the Google authorization URL for the OAuth/OIDC flow.
	///
	/// The returned URL should be used to redirect the user to Google for
	/// authorization. After authorization, Google will redirect back to the
	/// configured `redirect_uri` with `code` and `state` query parameters.
	///
	/// # Arguments
	///
	/// - `state`: A random, unguessable string to prevent CSRF attacks.
	///   This value should be stored server-side and verified when the
	///   user is redirected back.
	///
	/// - `nonce`: A random, unguessable string for replay protection.
	///   This value is included in the ID token and should be validated
	///   after token exchange to ensure the token was issued for this
	///   specific authorization request.
	///
	/// # Returns
	///
	/// A URL string that includes:
	/// - `client_id`: The application's OAuth client ID
	/// - `redirect_uri`: Where Google will redirect after authorization
	/// - `response_type`: Always "code" for authorization code flow
	/// - `scope`: The requested OAuth scopes (space-separated)
	/// - `state`: The CSRF protection token
	/// - `nonce`: The replay protection token
	/// - `access_type`: Set to "offline" to request a refresh token
	/// - `prompt`: Set to "consent" to always show consent screen
	#[tracing::instrument(skip(self), fields(client_id = %self.config.client_id))]
	pub fn authorization_url(&self, state: &str, nonce: &str) -> String {
		let mut url = Url::parse(GOOGLE_AUTHORIZE_URL).expect("invalid authorize URL");

		url
			.query_pairs_mut()
			.append_pair("client_id", &self.config.client_id)
			.append_pair("redirect_uri", &self.config.redirect_uri)
			.append_pair("response_type", "code")
			.append_pair("scope", &self.config.scopes_string())
			.append_pair("state", state)
			.append_pair("nonce", nonce)
			.append_pair("access_type", "offline")
			.append_pair("prompt", "consent");

		url.to_string()
	}

	/// Exchange an authorization code for tokens.
	///
	/// After the user authorizes the application, Google redirects back with
	/// an authorization code. This method exchanges that code for an access
	/// token and ID token.
	///
	/// # Arguments
	///
	/// - `code`: The authorization code from the OAuth callback.
	///
	/// # Returns
	///
	/// A [`GoogleTokenResponse`] containing the access token, ID token, and metadata.
	///
	/// # Errors
	///
	/// - [`OAuthError::HttpRequest`]: Network error or timeout.
	/// - [`OAuthError::GoogleError`]: Google rejected the code (expired, invalid, etc.).
	/// - [`OAuthError::ParseError`]: Unexpected response format.
	#[tracing::instrument(skip(self, code), name = "GoogleOAuthClient::exchange_code")]
	pub async fn exchange_code(&self, code: &str) -> Result<GoogleTokenResponse, OAuthError> {
		tracing::debug!("exchanging authorization code for tokens");

		let response = self
			.http_client
			.post(GOOGLE_TOKEN_URL)
			.header("Content-Type", "application/x-www-form-urlencoded")
			.form(&[
				("client_id", self.config.client_id.as_str()),
				("client_secret", self.config.client_secret.expose().as_str()),
				("code", code),
				("redirect_uri", self.config.redirect_uri.as_str()),
				("grant_type", "authorization_code"),
			])
			.send()
			.await?;

		let body = response.text().await?;

		if let Ok(error_response) = serde_json::from_str::<GoogleErrorResponse>(&body) {
			if !error_response.error.is_empty() {
				let message = error_response
					.error_description
					.unwrap_or(error_response.error);
				return Err(OAuthError::GoogleError(message));
			}
		}

		serde_json::from_str(&body)
			.map_err(|e| OAuthError::ParseError(format!("failed to parse token response: {e}")))
	}

	/// Fetch user information from Google's userinfo endpoint.
	///
	/// This makes an API call to Google to retrieve the user's profile.
	/// For most use cases, decoding the ID token with [`decode_id_token`]
	/// is preferred as it avoids an extra network request.
	///
	/// # Arguments
	///
	/// - `access_token`: The access token from [`exchange_code`].
	///
	/// # Returns
	///
	/// A [`GoogleUserInfo`] containing the user's profile information.
	///
	/// # Errors
	///
	/// - [`OAuthError::HttpRequest`]: Network error or timeout.
	/// - [`OAuthError::GoogleError`]: Google rejected the token (expired, invalid, etc.).
	/// - [`OAuthError::ParseError`]: Unexpected response format.
	#[tracing::instrument(skip(self, access_token), name = "GoogleOAuthClient::get_user_info")]
	pub async fn get_user_info(&self, access_token: &str) -> Result<GoogleUserInfo, OAuthError> {
		tracing::debug!("fetching Google user info");

		let response = self
			.http_client
			.get(GOOGLE_USERINFO_URL)
			.header("Authorization", format!("Bearer {access_token}"))
			.send()
			.await?;

		if !response.status().is_success() {
			let body = response.text().await.unwrap_or_default();
			return Err(OAuthError::GoogleError(format!(
				"failed to get user info: {body}"
			)));
		}

		response
			.json()
			.await
			.map_err(|e| OAuthError::ParseError(format!("failed to parse user info response: {e}")))
	}

	/// Decode an ID token to extract user claims.
	///
	/// This decodes the JWT payload to extract user identity claims without
	/// making an additional API call. Note that this does NOT verify the
	/// token signature - for production use, you should verify the signature
	/// using Google's public keys.
	///
	/// # Arguments
	///
	/// - `id_token`: The ID token from [`exchange_code`].
	///
	/// # Returns
	///
	/// The [`GoogleIdTokenClaims`] extracted from the token payload.
	///
	/// # Errors
	///
	/// - [`OAuthError::InvalidIdToken`]: The token format is invalid (not 3 parts),
	///   the payload is not valid base64, or the claims are not valid JSON.
	///
	/// # Security Note
	///
	/// After decoding, you should validate:
	/// 1. The `nonce` matches what was sent in the authorization request
	/// 2. The `aud` matches your client_id
	/// 3. The `exp` is in the future
	/// 4. The `iss` is "https://accounts.google.com"
	#[tracing::instrument(skip(self, id_token), name = "GoogleOAuthClient::decode_id_token")]
	pub fn decode_id_token(&self, id_token: &str) -> Result<GoogleIdTokenClaims, OAuthError> {
		let parts: Vec<&str> = id_token.split('.').collect();
		if parts.len() != 3 {
			return Err(OAuthError::InvalidIdToken(
				"ID token must have 3 parts".to_string(),
			));
		}

		let payload = parts[1];
		let decoded = URL_SAFE_NO_PAD
			.decode(payload)
			.map_err(|e| OAuthError::InvalidIdToken(format!("failed to decode payload: {e}")))?;

		let claims: GoogleIdTokenClaims = serde_json::from_slice(&decoded)
			.map_err(|e| OAuthError::InvalidIdToken(format!("failed to parse claims: {e}")))?;

		Ok(claims)
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn config_default_scopes() {
		let config = GoogleOAuthConfig {
			client_id: "test_id".to_string(),
			client_secret: SecretString::new("test_secret".to_string()),
			redirect_uri: "https://example.com/callback".to_string(),
			scopes: vec![
				"openid".to_string(),
				"email".to_string(),
				"profile".to_string(),
			],
		};

		assert_eq!(config.scopes.len(), 3);
		assert!(config.scopes.contains(&"openid".to_string()));
		assert!(config.scopes.contains(&"email".to_string()));
		assert!(config.scopes.contains(&"profile".to_string()));
	}

	#[test]
	fn authorization_url_contains_required_params() {
		let config = GoogleOAuthConfig {
			client_id: "test_client_id".to_string(),
			client_secret: SecretString::new("test_secret".to_string()),
			redirect_uri: "https://example.com/callback".to_string(),
			scopes: vec![
				"openid".to_string(),
				"email".to_string(),
				"profile".to_string(),
			],
		};

		let client = GoogleOAuthClient::new(config);
		let url = client.authorization_url("test_state_123", "test_nonce_456");

		assert!(url.starts_with("https://accounts.google.com/o/oauth2/v2/auth"));
		assert!(url.contains("client_id=test_client_id"));
		assert!(url.contains("redirect_uri=https%3A%2F%2Fexample.com%2Fcallback"));
		assert!(url.contains("response_type=code"));
		assert!(url.contains("state=test_state_123"));
		assert!(url.contains("nonce=test_nonce_456"));
		assert!(url.contains("scope=openid+email+profile"));
		assert!(url.contains("access_type=offline"));
		assert!(url.contains("prompt=consent"));
	}

	#[test]
	fn google_user_info_deserializes() {
		let json = r#"{
			"sub": "123456789",
			"email": "test@example.com",
			"email_verified": true,
			"name": "Test User",
			"picture": "https://example.com/photo.jpg",
			"given_name": "Test",
			"family_name": "User"
		}"#;

		let user: GoogleUserInfo = serde_json::from_str(json).unwrap();
		assert_eq!(user.sub, "123456789");
		assert_eq!(user.email, "test@example.com");
		assert!(user.email_verified);
		assert_eq!(user.name, Some("Test User".to_string()));
		assert_eq!(user.given_name, Some("Test".to_string()));
		assert_eq!(user.family_name, Some("User".to_string()));
	}

	#[test]
	fn google_user_info_deserializes_with_null_fields() {
		let json = r#"{
			"sub": "123456789",
			"email": "test@example.com",
			"email_verified": false,
			"name": null,
			"picture": null,
			"given_name": null,
			"family_name": null
		}"#;

		let user: GoogleUserInfo = serde_json::from_str(json).unwrap();
		assert_eq!(user.sub, "123456789");
		assert_eq!(user.email, "test@example.com");
		assert!(!user.email_verified);
		assert!(user.name.is_none());
		assert!(user.picture.is_none());
	}

	#[test]
	fn google_token_response_deserializes() {
		let json = r#"{
			"access_token": "ya29.xxxxxxxxxxxx",
			"id_token": "eyJhbGciOiJSUzI1NiJ9.eyJpc3MiOiJodHRwczovL2FjY291bnRzLmdvb2dsZS5jb20ifQ.signature",
			"token_type": "Bearer",
			"expires_in": 3600,
			"refresh_token": "1//xxxxxxxxxxxx",
			"scope": "openid email profile"
		}"#;

		let token: GoogleTokenResponse = serde_json::from_str(json).unwrap();
		assert_eq!(token.access_token.expose(), "ya29.xxxxxxxxxxxx");
		assert!(token.id_token.expose().starts_with("eyJ"));
		assert_eq!(token.token_type, "Bearer");
		assert_eq!(token.expires_in, 3600);
		assert_eq!(
			token.refresh_token.as_ref().map(|s| s.expose().as_str()),
			Some("1//xxxxxxxxxxxx")
		);
		assert_eq!(token.scope, "openid email profile");
	}

	#[test]
	fn google_token_response_deserializes_without_refresh_token() {
		let json = r#"{
			"access_token": "ya29.xxxxxxxxxxxx",
			"id_token": "eyJhbGciOiJSUzI1NiJ9.eyJpc3MiOiJodHRwczovL2FjY291bnRzLmdvb2dsZS5jb20ifQ.signature",
			"token_type": "Bearer",
			"expires_in": 3600,
			"scope": "openid email profile"
		}"#;

		let token: GoogleTokenResponse = serde_json::from_str(json).unwrap();
		assert!(token.refresh_token.is_none());
	}

	#[test]
	fn decode_id_token_success() {
		let config = GoogleOAuthConfig {
			client_id: "test_client_id".to_string(),
			client_secret: SecretString::new("test_secret".to_string()),
			redirect_uri: "https://example.com/callback".to_string(),
			scopes: vec!["openid".to_string()],
		};
		let client = GoogleOAuthClient::new(config);

		let payload = r#"{"iss":"https://accounts.google.com","sub":"123456789","aud":"test_client_id","exp":1735600000,"iat":1735596400,"email":"test@example.com","email_verified":true,"name":"Test User","picture":"https://example.com/photo.jpg","nonce":"test-nonce"}"#;
		let encoded_payload = URL_SAFE_NO_PAD.encode(payload);
		let id_token = format!("header.{encoded_payload}.signature");

		let claims = client.decode_id_token(&id_token).unwrap();
		assert_eq!(claims.iss, "https://accounts.google.com");
		assert_eq!(claims.sub, "123456789");
		assert_eq!(claims.aud, "test_client_id");
		assert_eq!(claims.email, Some("test@example.com".to_string()));
		assert_eq!(claims.email_verified, Some(true));
		assert_eq!(claims.name, Some("Test User".to_string()));
		assert_eq!(claims.nonce, Some("test-nonce".to_string()));
	}

	#[test]
	fn decode_id_token_invalid_format() {
		let config = GoogleOAuthConfig {
			client_id: "test_client_id".to_string(),
			client_secret: SecretString::new("test_secret".to_string()),
			redirect_uri: "https://example.com/callback".to_string(),
			scopes: vec!["openid".to_string()],
		};
		let client = GoogleOAuthClient::new(config);

		let result = client.decode_id_token("invalid_token");
		assert!(matches!(result, Err(OAuthError::InvalidIdToken(_))));
	}

	#[test]
	fn decode_id_token_invalid_base64() {
		let config = GoogleOAuthConfig {
			client_id: "test_client_id".to_string(),
			client_secret: SecretString::new("test_secret".to_string()),
			redirect_uri: "https://example.com/callback".to_string(),
			scopes: vec!["openid".to_string()],
		};
		let client = GoogleOAuthClient::new(config);

		let result = client.decode_id_token("header.!!!invalid-base64!!!.signature");
		assert!(matches!(result, Err(OAuthError::InvalidIdToken(_))));
	}

	#[test]
	fn decode_id_token_invalid_json() {
		let config = GoogleOAuthConfig {
			client_id: "test_client_id".to_string(),
			client_secret: SecretString::new("test_secret".to_string()),
			redirect_uri: "https://example.com/callback".to_string(),
			scopes: vec!["openid".to_string()],
		};
		let client = GoogleOAuthClient::new(config);

		let encoded_payload = URL_SAFE_NO_PAD.encode("not valid json");
		let id_token = format!("header.{encoded_payload}.signature");

		let result = client.decode_id_token(&id_token);
		assert!(matches!(result, Err(OAuthError::InvalidIdToken(_))));
	}

	#[test]
	fn google_id_token_claims_deserializes() {
		let json = r#"{
			"iss": "https://accounts.google.com",
			"sub": "123456789",
			"aud": "test_client_id",
			"exp": 1735600000,
			"iat": 1735596400,
			"email": "test@example.com",
			"email_verified": true,
			"name": "Test User",
			"picture": "https://example.com/photo.jpg",
			"nonce": "test-nonce-123"
		}"#;

		let claims: GoogleIdTokenClaims = serde_json::from_str(json).unwrap();
		assert_eq!(claims.iss, "https://accounts.google.com");
		assert_eq!(claims.sub, "123456789");
		assert_eq!(claims.aud, "test_client_id");
		assert_eq!(claims.exp, 1735600000);
		assert_eq!(claims.iat, 1735596400);
		assert_eq!(claims.email, Some("test@example.com".to_string()));
		assert_eq!(claims.email_verified, Some(true));
		assert_eq!(claims.nonce, Some("test-nonce-123".to_string()));
	}

	#[test]
	fn client_secret_not_in_debug() {
		let config = GoogleOAuthConfig {
			client_id: "test_id".to_string(),
			client_secret: SecretString::new("super_secret_value".to_string()),
			redirect_uri: "https://example.com".to_string(),
			scopes: vec![],
		};

		let debug_output = format!("{config:?}");

		assert!(!debug_output.contains("super_secret_value"));
		assert!(debug_output.contains("[REDACTED]"));
	}

	#[test]
	fn config_validation_passes_for_valid_config() {
		let config = GoogleOAuthConfig {
			client_id: "test_id".to_string(),
			client_secret: SecretString::new("test_secret".to_string()),
			redirect_uri: "https://example.com/callback".to_string(),
			scopes: vec!["openid".to_string()],
		};

		assert!(config.validate().is_ok());
	}

	#[test]
	fn config_validation_fails_for_empty_client_id() {
		let config = GoogleOAuthConfig {
			client_id: "".to_string(),
			client_secret: SecretString::new("test_secret".to_string()),
			redirect_uri: "https://example.com/callback".to_string(),
			scopes: vec![],
		};

		assert!(config.validate().is_err());
	}

	#[test]
	fn config_validation_fails_for_empty_client_secret() {
		let config = GoogleOAuthConfig {
			client_id: "test_id".to_string(),
			client_secret: SecretString::new("".to_string()),
			redirect_uri: "https://example.com/callback".to_string(),
			scopes: vec![],
		};

		assert!(config.validate().is_err());
	}

	#[test]
	fn config_validation_fails_for_empty_redirect_uri() {
		let config = GoogleOAuthConfig {
			client_id: "test_id".to_string(),
			client_secret: SecretString::new("test_secret".to_string()),
			redirect_uri: "".to_string(),
			scopes: vec![],
		};

		assert!(config.validate().is_err());
	}

	#[test]
	fn scope_parsing_handles_spaces() {
		let scopes = GoogleOAuthConfig::parse_scopes("openid email profile");
		assert_eq!(
			scopes,
			vec![
				"openid".to_string(),
				"email".to_string(),
				"profile".to_string()
			]
		);
	}

	#[test]
	fn scope_parsing_handles_commas() {
		let scopes = GoogleOAuthConfig::parse_scopes("openid,email,profile");
		assert_eq!(
			scopes,
			vec![
				"openid".to_string(),
				"email".to_string(),
				"profile".to_string()
			]
		);
	}

	#[test]
	fn scope_parsing_handles_mixed_separators() {
		let scopes = GoogleOAuthConfig::parse_scopes("openid, email profile");
		assert_eq!(
			scopes,
			vec![
				"openid".to_string(),
				"email".to_string(),
				"profile".to_string()
			]
		);
	}

	#[test]
	fn scope_parsing_handles_empty_string() {
		let scopes = GoogleOAuthConfig::parse_scopes("");
		assert!(scopes.is_empty());
	}
}

#[cfg(test)]
mod proptests {
	use super::*;
	use proptest::prelude::*;

	proptest! {
		/// Authorization URLs must always contain required OAuth/OIDC parameters
		/// regardless of the input values.
		#[test]
		fn authorization_url_always_has_required_params(
			client_id in "[a-zA-Z0-9]{1,40}",
			redirect_uri in "https://[a-z]{1,20}\\.[a-z]{2,5}/[a-z]{1,20}",
			state in "[a-zA-Z0-9]{1,64}",
			nonce in "[a-zA-Z0-9]{1,64}",
		) {
			let config = GoogleOAuthConfig {
				client_id: client_id.clone(),
				client_secret: SecretString::new("secret".to_string()),
				redirect_uri: redirect_uri.clone(),
				scopes: vec!["openid".to_string()],
			};

			let client = GoogleOAuthClient::new(config);
			let url = client.authorization_url(&state, &nonce);

			prop_assert!(url.starts_with(GOOGLE_AUTHORIZE_URL));
			prop_assert!(url.contains("response_type=code"));
			prop_assert!(url.contains("client_id="));
			prop_assert!(url.contains("redirect_uri="));
			prop_assert!(url.contains("scope="));
			prop_assert!(url.contains("state="));
			prop_assert!(url.contains("nonce="));
		}

		/// Scope joining and parsing should roundtrip correctly.
		#[test]
		fn scope_join_and_parse_roundtrips(
			scopes in proptest::collection::vec("[a-z]{1,10}", 1..5)
		) {
			let config = GoogleOAuthConfig {
				client_id: "id".to_string(),
				client_secret: SecretString::new("secret".to_string()),
				redirect_uri: "https://example.com".to_string(),
				scopes: scopes.clone(),
			};

			let joined = config.scopes_string();
			let parsed = GoogleOAuthConfig::parse_scopes(&joined);

			prop_assert_eq!(parsed, scopes);
		}

		/// Valid configurations should always pass validation.
		#[test]
		fn valid_config_passes_validation(
			client_id in "[a-zA-Z0-9]{1,40}",
			client_secret in "[a-zA-Z0-9]{1,40}",
			redirect_uri in "https://[a-z]{1,20}\\.[a-z]{2,5}/[a-z]{1,20}",
		) {
			let config = GoogleOAuthConfig {
				client_id,
				client_secret: SecretString::new(client_secret),
				redirect_uri,
				scopes: vec!["openid".to_string()],
			};

			prop_assert!(config.validate().is_ok());
		}

		/// Empty client_id should always fail validation.
		#[test]
		fn empty_client_id_fails_validation(
			client_secret in "[a-zA-Z0-9]{1,40}",
			redirect_uri in "https://[a-z]{1,20}\\.[a-z]{2,5}",
		) {
			let config = GoogleOAuthConfig {
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
			let config = GoogleOAuthConfig {
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
			let config = GoogleOAuthConfig {
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

			let config = GoogleOAuthConfig {
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
			token in "ya29\\.[a-zA-Z0-9_-]{10,40}"
		) {
			prop_assume!(!token.contains("REDACTED"));

			let json = format!(
				r#"{{"access_token": "{token}", "id_token": "eyJ.eyJ.sig", "token_type": "Bearer", "expires_in": 3600, "scope": "openid"}}"#
			);
			let response: GoogleTokenResponse = serde_json::from_str(&json).unwrap();

			let debug = format!("{response:?}");
			prop_assert!(!debug.contains(&token));
		}

		/// ID token should never appear in debug output.
		#[test]
		fn id_token_never_in_debug(
			payload in "[a-zA-Z0-9]{20,50}"
		) {
			prop_assume!(!payload.contains("REDACTED"));
			prop_assume!(!payload.contains("Secret"));

			let id_token = format!("eyJhbGciOiJSUzI1NiJ9.{payload}.signature");
			let json = format!(
				r#"{{"access_token": "ya29.token", "id_token": "{id_token}", "token_type": "Bearer", "expires_in": 3600, "scope": "openid"}}"#
			);
			let response: GoogleTokenResponse = serde_json::from_str(&json).unwrap();

			let debug = format!("{response:?}");
			prop_assert!(!debug.contains(&payload));
		}

		/// ID tokens without 3 parts should always fail decoding.
		#[test]
		fn id_token_wrong_part_count_fails(
			parts in proptest::collection::vec("[a-zA-Z0-9]{1,20}", 0..10usize)
		) {
			prop_assume!(parts.len() != 3);

			let config = GoogleOAuthConfig {
				client_id: "id".to_string(),
				client_secret: SecretString::new("secret".to_string()),
				redirect_uri: "https://example.com".to_string(),
				scopes: vec![],
			};
			let client = GoogleOAuthClient::new(config);

			let token = parts.join(".");
			let result = client.decode_id_token(&token);

			if parts.len() != 3 {
				prop_assert!(matches!(result, Err(OAuthError::InvalidIdToken(_))));
			}
		}

		/// Valid base64-encoded JSON claims should decode successfully.
		#[test]
		fn valid_claims_decode_successfully(
			sub in "[0-9]{5,20}",
			email in "[a-z]{3,10}@[a-z]{3,10}\\.[a-z]{2,3}",
		) {
			let config = GoogleOAuthConfig {
				client_id: "test_client".to_string(),
				client_secret: SecretString::new("secret".to_string()),
				redirect_uri: "https://example.com".to_string(),
				scopes: vec![],
			};
			let client = GoogleOAuthClient::new(config);

			let claims_json = format!(
				r#"{{"iss":"https://accounts.google.com","sub":"{sub}","aud":"test_client","exp":9999999999,"iat":1000000000,"email":"{email}"}}"#
			);
			let encoded = URL_SAFE_NO_PAD.encode(&claims_json);
			let token = format!("header.{encoded}.signature");

			let result = client.decode_id_token(&token);
			prop_assert!(result.is_ok());

			let claims = result.unwrap();
			prop_assert_eq!(claims.sub, sub);
			prop_assert_eq!(claims.email, Some(email));
		}
	}
}
