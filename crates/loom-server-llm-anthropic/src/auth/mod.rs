// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! Anthropic authentication module.
//!
//! Provides OAuth 2.0 + PKCE authentication for Claude Pro/Max subscriptions
//! and traditional API key authentication.

pub mod oauth_client;
mod oauth_flow;
mod pkce;
mod scheme;

pub use oauth_client::{OAuthClient, OAuthCredentials};
pub use oauth_flow::{
	authorize, create_api_key, exchange_code, refresh_token, AuthMode, AuthorizationRequest,
	ExchangeResult, CLIENT_ID, REDIRECT_URI, SCOPES, TOKEN_ENDPOINT,
};
pub use pkce::Pkce;
pub use scheme::{
	build_api_key_headers, build_oauth_headers, AnthropicAuth, AuthError, ANTHROPIC_USER_AGENT,
	API_KEY_BETA_HEADERS, CONTEXT_MANAGEMENT_BETA_HEADER, INTERLEAVED_THINKING_BETA_HEADER,
	OAUTH_BETA_HEADER, OAUTH_COMBINED_BETA_HEADERS, OAUTH_REQUIRED_SYSTEM_PROMPT_PREFIX,
};
