// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! Authentication scheme abstraction for Anthropic API.
//!
//! Provides a unified interface for both API key and OAuth authentication.

use loom_cli_credentials::{CredentialError, CredentialStore};
use loom_common_secret::SecretString;

use super::oauth_client::OAuthClient;

/// OAuth beta header required for subscription-based authentication.
pub const OAUTH_BETA_HEADER: &str = "oauth-2025-04-20";

/// Interleaved thinking beta header.
pub const INTERLEAVED_THINKING_BETA_HEADER: &str = "interleaved-thinking-2025-05-14";

/// Context management beta header.
pub const CONTEXT_MANAGEMENT_BETA_HEADER: &str = "context-management-2025-06-27";

/// Combined beta headers for OAuth requests (matching Claude CLI).
/// Note: Claude CLI does NOT include claude-code-20250219 for /v1/messages requests.
pub const OAUTH_COMBINED_BETA_HEADERS: &str =
	"oauth-2025-04-20,interleaved-thinking-2025-05-14,context-management-2025-06-27";

/// Combined beta headers for API key requests (non-OAuth).
pub const API_KEY_BETA_HEADERS: &str =
	"interleaved-thinking-2025-05-14,context-management-2025-06-27";

/// User-Agent to use when talking to Anthropic API.
/// Must match Claude CLI format exactly.
pub const ANTHROPIC_USER_AGENT: &str = "claude-cli/2.0.76 (external, sdk-cli)";

/// Required system prompt prefix for OAuth authentication with Opus/Sonnet models.
///
/// Anthropic validates OAuth requests to ensure they come from legitimate coding
/// assistant tools. The system prompt MUST start with this exact phrase (case-sensitive,
/// punctuation-sensitive) for OAuth tokens to work with premium models like Opus and Sonnet.
///
/// Without this prefix, OAuth requests to Opus/Sonnet will fail with:
/// "This credential is only authorized for use with Claude Code"
///
/// Haiku models work without this prefix, but Opus/Sonnet require it.
///
/// Reference: https://github.com/nsxdavid/anthropic-max-router
pub const OAUTH_REQUIRED_SYSTEM_PROMPT_PREFIX: &str =
	"You are Claude Code, Anthropic's official CLI for Claude.";

/// Authentication errors.
#[derive(Debug, thiserror::Error)]
pub enum AuthError {
	#[error("Missing credentials")]
	MissingCredentials,

	#[error("Credential error: {0}")]
	Credential(#[from] CredentialError),

	#[error("HTTP error: {0}")]
	Http(String),
}

/// Unified authentication for Anthropic API.
///
/// Supports both static API keys and OAuth-based subscription authentication.
#[derive(Debug)]
pub enum AnthropicAuth<S: CredentialStore> {
	/// Static pay-per-use API key.
	ApiKey { key: SecretString },

	/// OAuth-based subscription (Claude Pro/Max) or console OAuth.
	OAuth { client: OAuthClient<S> },
}

impl<S: CredentialStore> Clone for AnthropicAuth<S> {
	fn clone(&self) -> Self {
		match self {
			AnthropicAuth::ApiKey { key } => AnthropicAuth::ApiKey { key: key.clone() },
			AnthropicAuth::OAuth { client } => AnthropicAuth::OAuth {
				client: client.clone(),
			},
		}
	}
}

impl<S: CredentialStore> AnthropicAuth<S> {
	/// Create API key authentication.
	pub fn api_key(key: impl Into<String>) -> Self {
		AnthropicAuth::ApiKey {
			key: SecretString::new(key.into()),
		}
	}

	/// Check if this is API key authentication.
	pub fn is_api_key(&self) -> bool {
		matches!(self, AnthropicAuth::ApiKey { .. })
	}

	/// Check if this is OAuth authentication.
	pub fn is_oauth(&self) -> bool {
		matches!(self, AnthropicAuth::OAuth { .. })
	}

	/// Apply authentication to a request builder.
	///
	/// For API keys, sets `x-api-key` header and `anthropic-beta` headers.
	/// For OAuth, sets `Authorization: Bearer` header and combined `anthropic-beta` headers.
	/// Both cases also set the User-Agent and other required headers to match Claude CLI.
	pub async fn apply_to_request(
		&self,
		request: reqwest::RequestBuilder,
	) -> Result<reqwest::RequestBuilder, AuthError> {
		match self {
			AnthropicAuth::ApiKey { key } => Ok(
				request
					.header("x-api-key", key.expose())
					.header("anthropic-beta", API_KEY_BETA_HEADERS)
					.header("anthropic-dangerous-direct-browser-access", "true")
					.header("user-agent", ANTHROPIC_USER_AGENT),
			),
			AnthropicAuth::OAuth { client } => {
				let token = client.get_access_token().await?;
				Ok(
					request
						.bearer_auth(token)
						.header("anthropic-beta", OAUTH_COMBINED_BETA_HEADERS)
						.header("anthropic-dangerous-direct-browser-access", "true")
						.header("user-agent", ANTHROPIC_USER_AGENT),
				)
			}
		}
	}
}

/// Build HTTP headers for OAuth requests.
/// Includes all required headers to match Claude CLI behavior.
pub fn build_oauth_headers(
	access_token: &str,
	additional_beta: Option<&str>,
) -> reqwest::header::HeaderMap {
	let mut headers = reqwest::header::HeaderMap::new();

	headers.insert(
		reqwest::header::AUTHORIZATION,
		format!("Bearer {access_token}").parse().unwrap(),
	);

	let beta_value = if let Some(additional) = additional_beta {
		format!("{OAUTH_COMBINED_BETA_HEADERS},{additional}")
	} else {
		OAUTH_COMBINED_BETA_HEADERS.to_string()
	};

	headers.insert(
		reqwest::header::HeaderName::from_static("anthropic-beta"),
		beta_value.parse().unwrap(),
	);

	headers.insert(
		reqwest::header::HeaderName::from_static("anthropic-dangerous-direct-browser-access"),
		"true".parse().unwrap(),
	);

	headers.insert(
		reqwest::header::USER_AGENT,
		ANTHROPIC_USER_AGENT.parse().unwrap(),
	);

	headers
}

/// Build HTTP headers for API key requests.
/// Includes all required headers to match Claude CLI behavior.
pub fn build_api_key_headers(api_key: &str) -> reqwest::header::HeaderMap {
	let mut headers = reqwest::header::HeaderMap::new();

	headers.insert(
		reqwest::header::HeaderName::from_static("x-api-key"),
		api_key.parse().unwrap(),
	);

	headers.insert(
		reqwest::header::HeaderName::from_static("anthropic-beta"),
		API_KEY_BETA_HEADERS.parse().unwrap(),
	);

	headers.insert(
		reqwest::header::HeaderName::from_static("anthropic-dangerous-direct-browser-access"),
		"true".parse().unwrap(),
	);

	headers.insert(
		reqwest::header::USER_AGENT,
		ANTHROPIC_USER_AGENT.parse().unwrap(),
	);

	headers
}

#[cfg(test)]
mod tests {
	use super::*;
	use loom_cli_credentials::MemoryCredentialStore;
	use std::sync::Arc;

	use super::super::oauth_client::OAuthCredentials;

	#[test]
	fn test_anthropic_auth_api_key() {
		let auth: AnthropicAuth<MemoryCredentialStore> = AnthropicAuth::api_key("sk-test");
		assert!(auth.is_api_key());
		assert!(!auth.is_oauth());
	}

	#[test]
	fn test_anthropic_auth_oauth() {
		let store = Arc::new(MemoryCredentialStore::new());
		let now_ms = std::time::SystemTime::now()
			.duration_since(std::time::UNIX_EPOCH)
			.unwrap()
			.as_millis() as u64;

		let creds = OAuthCredentials::new(
			SecretString::new("rt_test".to_string()),
			SecretString::new("at_test".to_string()),
			now_ms + 120_000,
		);

		let auth: AnthropicAuth<MemoryCredentialStore> = AnthropicAuth::OAuth {
			client: OAuthClient::new("anthropic", creds, store),
		};

		assert!(auth.is_oauth());
		assert!(!auth.is_api_key());
	}

	#[test]
	fn test_build_oauth_headers() {
		let headers = build_oauth_headers("at_test", None);

		assert_eq!(
			headers.get(reqwest::header::AUTHORIZATION).unwrap(),
			"Bearer at_test"
		);
		// Should include all required beta headers (matching Claude CLI)
		let beta = headers.get("anthropic-beta").unwrap().to_str().unwrap();
		assert!(beta.contains(OAUTH_BETA_HEADER));
		assert!(beta.contains(INTERLEAVED_THINKING_BETA_HEADER));
		assert!(beta.contains(CONTEXT_MANAGEMENT_BETA_HEADER));
		// Should include dangerous direct browser access header
		assert_eq!(
			headers
				.get("anthropic-dangerous-direct-browser-access")
				.unwrap(),
			"true"
		);
		// Should have Claude CLI user-agent
		assert_eq!(
			headers.get(reqwest::header::USER_AGENT).unwrap(),
			ANTHROPIC_USER_AGENT
		);
	}

	#[test]
	fn test_build_oauth_headers_with_additional_beta() {
		let headers = build_oauth_headers("at_test", Some("max-tokens-3-5-sonnet-2024-07-15"));

		let beta = headers.get("anthropic-beta").unwrap().to_str().unwrap();
		assert!(beta.contains(OAUTH_BETA_HEADER));
		assert!(beta.contains("max-tokens"));
	}

	#[test]
	fn test_build_api_key_headers() {
		let headers = build_api_key_headers("sk-test-key");

		assert_eq!(headers.get("x-api-key").unwrap(), "sk-test-key");
		// Should include beta headers
		let beta = headers.get("anthropic-beta").unwrap().to_str().unwrap();
		assert!(beta.contains(INTERLEAVED_THINKING_BETA_HEADER));
		assert!(beta.contains(CONTEXT_MANAGEMENT_BETA_HEADER));
		// Should NOT include OAuth header for API key auth
		assert!(!beta.contains(OAUTH_BETA_HEADER));
		// Should include dangerous direct browser access header
		assert_eq!(
			headers
				.get("anthropic-dangerous-direct-browser-access")
				.unwrap(),
			"true"
		);
		// Should have Claude CLI user-agent
		assert_eq!(
			headers.get(reqwest::header::USER_AGENT).unwrap(),
			ANTHROPIC_USER_AGENT
		);
	}
}
