// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights
// reserved. SPDX-License-Identifier: Proprietary

//! Error types for Serper.dev API client.

use loom_common_http::RetryableError;
use thiserror::Error;

/// Errors that can occur when interacting with the Serper API.
#[derive(Debug, Error)]
pub enum SerperError {
	/// Network-level error during HTTP communication.
	#[error("Network error: {0}")]
	Network(#[from] reqwest::Error),

	/// Request timed out.
	#[error("Request timed out")]
	Timeout,

	/// Rate limit exceeded.
	#[error("Rate limit exceeded")]
	RateLimited,

	/// Invalid API key.
	#[error("Invalid API key")]
	Unauthorized,

	/// Invalid or unparseable response from Serper.
	#[error("Invalid response from Serper: {0}")]
	InvalidResponse(String),

	/// Serper API returned an error status.
	#[error("Serper API error: {status} - {message}")]
	ApiError { status: u16, message: String },
}

impl RetryableError for SerperError {
	fn is_retryable(&self) -> bool {
		match self {
			SerperError::Network(e) => e.is_retryable(),
			SerperError::Timeout => true,
			SerperError::RateLimited => true,
			SerperError::Unauthorized => false,
			SerperError::InvalidResponse(_) => false,
			SerperError::ApiError { status, .. } => *status >= 500,
		}
	}
}
