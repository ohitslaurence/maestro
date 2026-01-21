// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights
// reserved. SPDX-License-Identifier: Proprietary

//! Error types for GitHub App client.

use loom_common_http::RetryableError;
use thiserror::Error;

/// Errors that can occur when interacting with the GitHub App API.
#[derive(Debug, Error)]
pub enum GithubAppError {
	/// Network-level error during HTTP communication.
	#[error("Network error: {0}")]
	Network(#[from] reqwest::Error),

	/// Request timed out.
	#[error("Request timed out")]
	Timeout,

	/// Invalid API key or app configuration.
	#[error("Unauthorized or invalid app configuration")]
	Unauthorized,

	/// Forbidden - insufficient permissions.
	#[error("Forbidden or insufficient permissions")]
	Forbidden,

	/// Rate limit exceeded.
	#[error("Rate limit exceeded")]
	RateLimited,

	/// GitHub API returned an error.
	#[error("GitHub API error: {status} - {message}")]
	ApiError { status: u16, message: String },

	/// Invalid or unparseable response.
	#[error("Invalid response from GitHub: {0}")]
	InvalidResponse(String),

	/// Configuration error.
	#[error("Configuration error: {0}")]
	Config(String),

	/// JWT signing/encoding error.
	#[error("JWT error: {0}")]
	Jwt(String),

	/// Installation not found for repository.
	#[error("GitHub App not installed for {owner}/{repo}")]
	InstallationNotFound { owner: String, repo: String },

	/// Webhook signature verification failed.
	#[error("Invalid webhook signature")]
	InvalidWebhookSignature,
}

impl RetryableError for GithubAppError {
	fn is_retryable(&self) -> bool {
		match self {
			GithubAppError::Network(e) => e.is_retryable(),
			GithubAppError::Timeout => true,
			GithubAppError::RateLimited => true,
			GithubAppError::ApiError { status, .. } => *status >= 500,
			_ => false,
		}
	}
}

impl GithubAppError {
	/// Create an API error from status code and message.
	pub fn api_error(status: u16, message: impl Into<String>) -> Self {
		Self::ApiError {
			status,
			message: message.into(),
		}
	}

	/// Create an installation not found error.
	pub fn installation_not_found(owner: impl Into<String>, repo: impl Into<String>) -> Self {
		Self::InstallationNotFound {
			owner: owner.into(),
			repo: repo.into(),
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_retryable_timeout() {
		assert!(GithubAppError::Timeout.is_retryable());
	}

	#[test]
	fn test_retryable_rate_limited() {
		assert!(GithubAppError::RateLimited.is_retryable());
	}

	#[test]
	fn test_retryable_5xx() {
		assert!(GithubAppError::api_error(500, "Internal Server Error").is_retryable());
		assert!(GithubAppError::api_error(502, "Bad Gateway").is_retryable());
		assert!(GithubAppError::api_error(503, "Service Unavailable").is_retryable());
	}

	#[test]
	fn test_not_retryable_4xx() {
		assert!(!GithubAppError::api_error(400, "Bad Request").is_retryable());
		assert!(!GithubAppError::api_error(404, "Not Found").is_retryable());
	}

	#[test]
	fn test_not_retryable_config() {
		assert!(!GithubAppError::Config("missing key".to_string()).is_retryable());
	}

	#[test]
	fn test_not_retryable_jwt() {
		assert!(!GithubAppError::Jwt("invalid key".to_string()).is_retryable());
	}

	#[test]
	fn test_not_retryable_webhook_signature() {
		assert!(!GithubAppError::InvalidWebhookSignature.is_retryable());
	}

	#[test]
	fn test_error_display() {
		let err = GithubAppError::installation_not_found("my-org", "my-repo");
		assert_eq!(
			err.to_string(),
			"GitHub App not installed for my-org/my-repo"
		);
	}
}
