// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights
// reserved. SPDX-License-Identifier: Proprietary

//! Anthropic LLM client implementation for Loom.
//!
//! This crate provides an implementation of the `LlmClient` trait for
//! Anthropic's Claude models via the Messages API.
//!
//! # Authentication
//!
//! Supports both API key and OAuth authentication.
//!
//! ```rust,no_run
//! use loom_server_llm_anthropic::{AnthropicClient, AnthropicConfig};
//!
//! // API key authentication (default)
//! let config = AnthropicConfig::new("sk-ant-api03-...");
//! let client = AnthropicClient::new(config).unwrap();
//! ```

pub mod auth;
mod client;
pub mod pool;
mod stream;
mod types;

pub use client::{is_permanent_auth_message, is_quota_message, AnthropicClient};
pub use pool::{
	AccountDetails, AccountHealthInfo, AccountHealthStatus, AccountSelectionStrategy, AnthropicPool,
	AnthropicPoolConfig, PoolStatus,
};
pub use types::*;

// Re-export auth types for convenience
pub use auth::{
	authorize, build_api_key_headers, build_oauth_headers, create_api_key, exchange_code,
	refresh_token, AnthropicAuth, AuthError, AuthMode, AuthorizationRequest, ExchangeResult,
	OAuthClient, Pkce, CLIENT_ID, OAUTH_BETA_HEADER, REDIRECT_URI, SCOPES, TOKEN_ENDPOINT,
};

// Re-export OAuthCredentials from auth module
pub use auth::oauth_client::OAuthCredentials;

// Re-export credential types from loom-cli-credentials
pub use loom_cli_credentials::{
	CredentialError, CredentialStore, CredentialValue, FileCredentialStore, MemoryCredentialStore,
};
