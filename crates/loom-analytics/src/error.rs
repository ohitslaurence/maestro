// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! Error types for the analytics SDK.

use loom_common_http::RetryableError;
use thiserror::Error;

/// Analytics SDK errors.
#[derive(Debug, Error)]
pub enum AnalyticsError {
	/// API key is missing or invalid.
	#[error("invalid API key: must start with 'loom_analytics_'")]
	InvalidApiKey,

	/// Base URL is missing or invalid.
	#[error("invalid base URL")]
	InvalidBaseUrl,

	/// HTTP request failed.
	#[error("HTTP request failed: {0}")]
	RequestFailed(#[from] reqwest::Error),

	/// Server returned an error response.
	#[error("server error ({status}): {message}")]
	ServerError { status: u16, message: String },

	/// Rate limited by the server.
	#[error("rate limited, retry after {retry_after_secs:?} seconds")]
	RateLimited { retry_after_secs: Option<u64> },

	/// Client has been shut down.
	#[error("client has been shut down")]
	ClientShutdown,

	/// Event validation failed.
	#[error("event validation failed: {0}")]
	ValidationFailed(String),

	/// Serialization error.
	#[error("serialization error: {0}")]
	SerializationError(String),
}

impl RetryableError for AnalyticsError {
	fn is_retryable(&self) -> bool {
		match self {
			AnalyticsError::RequestFailed(e) => e.is_retryable(),
			AnalyticsError::ServerError { status, .. } => {
				matches!(*status, 429 | 408 | 500 | 502 | 503 | 504)
			}
			AnalyticsError::RateLimited { .. } => true,
			_ => false,
		}
	}
}

/// Result type alias for analytics operations.
pub type Result<T> = std::result::Result<T, AnalyticsError>;

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_server_error_retryable_statuses() {
		let retryable_codes = [429, 408, 500, 502, 503, 504];
		for status in retryable_codes {
			let err = AnalyticsError::ServerError {
				status,
				message: "test".to_string(),
			};
			assert!(err.is_retryable(), "status {status} should be retryable");
		}
	}

	#[test]
	fn test_server_error_non_retryable_statuses() {
		let non_retryable_codes = [400, 401, 403, 404, 422];
		for status in non_retryable_codes {
			let err = AnalyticsError::ServerError {
				status,
				message: "test".to_string(),
			};
			assert!(
				!err.is_retryable(),
				"status {status} should not be retryable"
			);
		}
	}

	#[test]
	fn test_rate_limited_is_retryable() {
		let err = AnalyticsError::RateLimited {
			retry_after_secs: Some(30),
		};
		assert!(err.is_retryable());
	}

	#[test]
	fn test_validation_error_not_retryable() {
		let err = AnalyticsError::ValidationFailed("invalid event".to_string());
		assert!(!err.is_retryable());
	}

	#[test]
	fn test_invalid_api_key_not_retryable() {
		let err = AnalyticsError::InvalidApiKey;
		assert!(!err.is_retryable());
	}

	#[test]
	fn test_client_shutdown_not_retryable() {
		let err = AnalyticsError::ClientShutdown;
		assert!(!err.is_retryable());
	}
}
