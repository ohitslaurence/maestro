// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! Error types for the feature flags SDK.

use thiserror::Error;

/// Result type alias for the flags SDK.
pub type Result<T> = std::result::Result<T, FlagsError>;

/// Errors that can occur in the feature flags SDK.
#[derive(Error, Debug)]
pub enum FlagsError {
	/// SDK key is missing or invalid.
	#[error("Invalid or missing SDK key")]
	InvalidSdkKey,

	/// Base URL is missing or invalid.
	#[error("Invalid or missing base URL")]
	InvalidBaseUrl,

	/// Failed to connect to the server.
	#[error("Failed to connect to server: {0}")]
	ConnectionFailed(#[source] reqwest::Error),

	/// HTTP request failed.
	#[error("HTTP request failed: {0}")]
	RequestFailed(#[source] reqwest::Error),

	/// Failed to parse server response.
	#[error("Failed to parse server response: {0}")]
	ParseFailed(String),

	/// Server returned an error response.
	#[error("Server returned an error: {status} - {message}")]
	ServerError {
		/// HTTP status code.
		status: u16,
		/// Error message from server.
		message: String,
	},

	/// Flag not found.
	#[error("Flag not found: {flag_key}")]
	FlagNotFound {
		/// The flag key that was not found.
		flag_key: String,
	},

	/// SSE connection error.
	#[error("SSE connection failed: {0}")]
	SseConnectionFailed(String),

	/// SSE stream error.
	#[error("SSE stream error: {0}")]
	SseStreamError(String),

	/// Client is offline and no cached data available.
	#[error("Client is offline and no cached data is available")]
	OfflineNoCache,

	/// Authentication failed.
	#[error("SDK key authentication failed")]
	AuthenticationFailed,

	/// Rate limited.
	#[error("Rate limited. Retry after {retry_after_secs:?} seconds")]
	RateLimited {
		/// Seconds until retry is allowed.
		retry_after_secs: Option<u64>,
	},

	/// Initialization timeout.
	#[error("Client initialization timed out")]
	InitializationTimeout,

	/// Client already closed.
	#[error("Client has been closed")]
	ClientClosed,

	/// Invalid flag value type.
	#[error("Invalid flag value type: expected {expected}, got {actual}")]
	InvalidValueType {
		/// Expected type.
		expected: String,
		/// Actual type.
		actual: String,
	},
}

impl FlagsError {
	/// Returns true if this error is retryable.
	pub fn is_retryable(&self) -> bool {
		matches!(
			self,
			FlagsError::ConnectionFailed(_)
				| FlagsError::SseConnectionFailed(_)
				| FlagsError::SseStreamError(_)
				| FlagsError::RateLimited { .. }
		)
	}

	/// Returns true if the client should use cached values for this error.
	pub fn should_use_cache(&self) -> bool {
		matches!(
			self,
			FlagsError::ConnectionFailed(_)
				| FlagsError::RequestFailed(_)
				| FlagsError::ServerError {
					status: 500..=599,
					..
				} | FlagsError::RateLimited { .. }
		)
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_retryable_errors() {
		assert!(FlagsError::SseConnectionFailed("test".to_string()).is_retryable());
		assert!(FlagsError::SseStreamError("test".to_string()).is_retryable());
		assert!(FlagsError::RateLimited {
			retry_after_secs: Some(60)
		}
		.is_retryable());
		assert!(!FlagsError::InvalidSdkKey.is_retryable());
		assert!(!FlagsError::FlagNotFound {
			flag_key: "test".to_string()
		}
		.is_retryable());
	}

	#[test]
	fn test_should_use_cache() {
		assert!(FlagsError::ServerError {
			status: 503,
			message: "unavailable".to_string()
		}
		.should_use_cache());
		assert!(FlagsError::RateLimited {
			retry_after_secs: None
		}
		.should_use_cache());
		assert!(!FlagsError::InvalidSdkKey.should_use_cache());
		assert!(!FlagsError::AuthenticationFailed.should_use_cache());
	}
}
