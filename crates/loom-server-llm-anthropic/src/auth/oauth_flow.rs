// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! OAuth 2.0 flow implementation for Anthropic Claude.
//!
//! Supports both Claude Pro/Max subscription OAuth and Console OAuth for API key creation.

use loom_cli_credentials::CredentialError;
use serde::{Deserialize, Serialize};
use tracing::{debug, error, info};
use url::Url;

use super::pkce::Pkce;

/// Anthropic's public OAuth client ID for CLI tools.
pub const CLIENT_ID: &str = "9d1c250a-e61b-44d9-88ed-5944d1962f5e";

/// OAuth redirect URI.
pub const REDIRECT_URI: &str = "https://console.anthropic.com/oauth/code/callback";

/// Token endpoint for exchange and refresh.
pub const TOKEN_ENDPOINT: &str = "https://console.anthropic.com/v1/oauth/token";

/// OAuth scopes required for Claude Pro/Max subscription inference.
/// Must include user:sessions:claude_code to access Sonnet/Opus models.
pub const SCOPES: &str = "user:inference user:profile user:sessions:claude_code";

/// Authorization mode determines which OAuth endpoint to use.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthMode {
	/// Use claude.ai for Pro/Max subscription OAuth.
	Max,
	/// Use console.anthropic.com for API key creation.
	Console,
}

impl AuthMode {
	/// Get the authorization URL for this mode.
	pub fn authorize_url(&self) -> &'static str {
		match self {
			AuthMode::Max => "https://claude.ai/oauth/authorize",
			AuthMode::Console => "https://console.anthropic.com/oauth/authorize",
		}
	}
}

/// Result of initiating an authorization request.
#[derive(Debug, Clone)]
pub struct AuthorizationRequest {
	/// URL to open in browser.
	pub url: String,
	/// PKCE verifier to store for token exchange.
	pub verifier: String,
}

/// Build the OAuth authorization URL.
pub fn authorize(mode: AuthMode) -> AuthorizationRequest {
	let pkce = Pkce::generate();

	let mut url = Url::parse(mode.authorize_url()).expect("Invalid authorize URL");

	{
		let mut params = url.query_pairs_mut();
		params.append_pair("code", "true");
		params.append_pair("client_id", CLIENT_ID);
		params.append_pair("response_type", "code");
		params.append_pair("redirect_uri", REDIRECT_URI);
		params.append_pair("scope", SCOPES);
		params.append_pair("code_challenge", &pkce.challenge);
		params.append_pair("code_challenge_method", "S256");
		params.append_pair("state", &pkce.verifier);
	}

	AuthorizationRequest {
		url: url.to_string(),
		verifier: pkce.verifier,
	}
}

/// Token exchange request body.
#[derive(Debug, Serialize)]
struct TokenExchangeRequest {
	code: String,
	state: String,
	grant_type: String,
	client_id: String,
	redirect_uri: String,
	code_verifier: String,
}

/// Token refresh request body.
#[derive(Debug, Serialize)]
struct TokenRefreshRequest {
	refresh_token: String,
	grant_type: String,
	client_id: String,
}

/// Successful token response from Anthropic.
#[derive(Debug, Deserialize)]
struct TokenResponse {
	access_token: String,
	refresh_token: String,
	expires_in: u64,
}

/// Error response from token endpoint.
#[derive(Debug, Deserialize)]
struct TokenErrorResponse {
	error: String,
	#[serde(default)]
	error_description: Option<String>,
}

/// Result of a token exchange or refresh operation.
#[derive(Debug, Clone)]
pub enum ExchangeResult {
	/// Successfully obtained tokens.
	Success {
		access: String,
		refresh: String,
		/// Expiration timestamp in milliseconds since epoch.
		expires: u64,
	},
	/// Token exchange failed.
	Failed { error: String },
}

/// Exchange an authorization code for tokens.
///
/// # Arguments
/// * `code` - The authorization code from the callback
/// * `state` - The state parameter from the callback (must match what was sent in authorization)
/// * `verifier` - The PKCE code verifier (secret that proves we initiated the request)
pub async fn exchange_code(
	code: &str,
	state: &str,
	verifier: &str,
) -> Result<ExchangeResult, CredentialError> {
	let client = loom_common_http::new_client();

	let request = TokenExchangeRequest {
		code: code.to_string(),
		state: state.to_string(),
		grant_type: "authorization_code".to_string(),
		client_id: CLIENT_ID.to_string(),
		redirect_uri: REDIRECT_URI.to_string(),
		code_verifier: verifier.to_string(),
	};

	debug!("Exchanging authorization code for tokens");

	let response = client
		.post(TOKEN_ENDPOINT)
		.header("Content-Type", "application/json")
		.json(&request)
		.send()
		.await
		.map_err(|e| CredentialError::Other(format!("HTTP error: {e}")))?;

	let status = response.status();

	if status.is_success() {
		let token_response: TokenResponse = response
			.json()
			.await
			.map_err(|e| CredentialError::Serde(e.to_string()))?;

		let now_ms = std::time::SystemTime::now()
			.duration_since(std::time::UNIX_EPOCH)
			.unwrap()
			.as_millis() as u64;

		let expires = now_ms + (token_response.expires_in * 1000);

		info!("Successfully exchanged code for tokens");

		Ok(ExchangeResult::Success {
			access: token_response.access_token,
			refresh: token_response.refresh_token,
			expires,
		})
	} else {
		let error_body = response.text().await.unwrap_or_default();

		if let Ok(error_response) = serde_json::from_str::<TokenErrorResponse>(&error_body) {
			let error_msg = error_response
				.error_description
				.unwrap_or(error_response.error);
			error!(error = %error_msg, "Token exchange failed");
			Ok(ExchangeResult::Failed { error: error_msg })
		} else {
			error!(status = %status, body = %error_body, "Token exchange failed");
			Ok(ExchangeResult::Failed { error: error_body })
		}
	}
}

/// Refresh an access token using a refresh token.
pub async fn refresh_token(refresh: &str) -> Result<ExchangeResult, CredentialError> {
	let client = loom_common_http::new_client();

	let request = TokenRefreshRequest {
		refresh_token: refresh.to_string(),
		grant_type: "refresh_token".to_string(),
		client_id: CLIENT_ID.to_string(),
	};

	debug!("Refreshing access token");

	let response = client
		.post(TOKEN_ENDPOINT)
		.header("Content-Type", "application/json")
		.json(&request)
		.send()
		.await
		.map_err(|e| CredentialError::Other(format!("HTTP error: {e}")))?;

	let status = response.status();

	if status.is_success() {
		let token_response: TokenResponse = response
			.json()
			.await
			.map_err(|e| CredentialError::Serde(e.to_string()))?;

		let now_ms = std::time::SystemTime::now()
			.duration_since(std::time::UNIX_EPOCH)
			.unwrap()
			.as_millis() as u64;

		let expires = now_ms + (token_response.expires_in * 1000);

		info!("Successfully refreshed access token");

		Ok(ExchangeResult::Success {
			access: token_response.access_token,
			refresh: token_response.refresh_token,
			expires,
		})
	} else {
		let error_body = response.text().await.unwrap_or_default();

		if let Ok(error_response) = serde_json::from_str::<TokenErrorResponse>(&error_body) {
			let error_msg = error_response
				.error_description
				.unwrap_or(error_response.error);
			error!(error = %error_msg, "Token refresh failed");
			Ok(ExchangeResult::Failed { error: error_msg })
		} else {
			error!(status = %status, body = %error_body, "Token refresh failed");
			Ok(ExchangeResult::Failed { error: error_body })
		}
	}
}

/// Response from creating an API key.
#[derive(Debug, Deserialize)]
struct CreateApiKeyResponse {
	raw_key: String,
}

/// Create an API key using OAuth credentials.
///
/// This is used when the user selects "Create API Key" option via Console OAuth.
pub async fn create_api_key(access_token: &str) -> Result<String, CredentialError> {
	let client = loom_common_http::new_client();

	let response = client
		.post("https://api.anthropic.com/api/oauth/claude_cli/create_api_key")
		.header("Content-Type", "application/json")
		.header("Authorization", format!("Bearer {access_token}"))
		.send()
		.await
		.map_err(|e| CredentialError::Other(format!("HTTP error: {e}")))?;

	if !response.status().is_success() {
		let status = response.status();
		let body = response.text().await.unwrap_or_default();
		error!(status = %status, body = %body, "Failed to create API key");
		return Err(CredentialError::Other(format!(
			"Failed to create API key: {status} - {body}"
		)));
	}

	let result: CreateApiKeyResponse = response
		.json()
		.await
		.map_err(|e| CredentialError::Serde(e.to_string()))?;

	info!("Successfully created API key");
	Ok(result.raw_key)
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_auth_mode_urls() {
		assert_eq!(
			AuthMode::Max.authorize_url(),
			"https://claude.ai/oauth/authorize"
		);
		assert_eq!(
			AuthMode::Console.authorize_url(),
			"https://console.anthropic.com/oauth/authorize"
		);
	}

	#[test]
	fn test_authorize_generates_url() {
		let request = authorize(AuthMode::Max);

		assert!(request.url.contains("claude.ai/oauth/authorize"));
		assert!(request.url.contains("client_id="));
		assert!(request.url.contains("code_challenge="));
		assert!(request.url.contains("code_challenge_method=S256"));
		assert!(!request.verifier.is_empty());
	}

	#[test]
	fn test_authorize_console_mode() {
		let request = authorize(AuthMode::Console);

		assert!(request
			.url
			.contains("console.anthropic.com/oauth/authorize"));
	}

	#[test]
	fn test_authorize_includes_all_params() {
		let request = authorize(AuthMode::Max);
		let url = Url::parse(&request.url).unwrap();

		let params: std::collections::HashMap<_, _> = url.query_pairs().collect();

		assert_eq!(params.get("client_id").map(|s| s.as_ref()), Some(CLIENT_ID));
		assert_eq!(
			params.get("response_type").map(|s| s.as_ref()),
			Some("code")
		);
		assert_eq!(
			params.get("redirect_uri").map(|s| s.as_ref()),
			Some(REDIRECT_URI)
		);
		assert_eq!(params.get("scope").map(|s| s.as_ref()), Some(SCOPES));
		assert_eq!(
			params.get("code_challenge_method").map(|s| s.as_ref()),
			Some("S256")
		);
		assert!(params.contains_key("code_challenge"));
		assert!(params.contains_key("state"));
	}

	#[test]
	fn test_exchange_result_variants() {
		let success = ExchangeResult::Success {
			access: "at_test".to_string(),
			refresh: "rt_test".to_string(),
			expires: 12345,
		};

		if let ExchangeResult::Success {
			access,
			refresh,
			expires,
		} = success
		{
			assert_eq!(access, "at_test");
			assert_eq!(refresh, "rt_test");
			assert_eq!(expires, 12345);
		}

		let failed = ExchangeResult::Failed {
			error: "invalid_grant".to_string(),
		};

		if let ExchangeResult::Failed { error } = failed {
			assert_eq!(error, "invalid_grant");
		}
	}
}
