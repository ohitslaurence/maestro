// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights
// reserved. SPDX-License-Identifier: Proprietary

use loom_common_http::RetryableError;
use reqwest::StatusCode;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ThreadIdError {
	#[error("invalid thread ID prefix: expected 'T-', got '{0}'")]
	InvalidPrefix(String),

	#[error("invalid UUID in thread ID: {0}")]
	InvalidUuid(#[from] uuid::Error),
}

#[derive(Debug, Error)]
pub enum ThreadStoreError {
	#[error("I/O error: {0}")]
	Io(#[from] std::io::Error),

	#[error("serialization error: {0}")]
	Serialization(#[from] serde_json::Error),

	#[error("thread not found: {0}")]
	NotFound(String),

	#[error("sync error: {0}")]
	Sync(#[from] ThreadSyncError),
}

#[derive(Debug, Error)]
pub enum ThreadSyncError {
	#[error("network error: {0}")]
	Network(#[from] reqwest::Error),

	#[error("server error: {status} - {message}")]
	Server { status: StatusCode, message: String },

	#[error("conflict: thread was modified on server")]
	Conflict,

	#[error("invalid URL: {0}")]
	InvalidUrl(String),

	#[error("request timeout")]
	Timeout,

	#[error("unexpected status: {0}")]
	UnexpectedStatus(StatusCode),
}

impl RetryableError for ThreadSyncError {
	fn is_retryable(&self) -> bool {
		match self {
			Self::Network(e) => e.is_retryable(),
			Self::Server { status, .. } => matches!(
				*status,
				StatusCode::TOO_MANY_REQUESTS
					| StatusCode::REQUEST_TIMEOUT
					| StatusCode::BAD_GATEWAY
					| StatusCode::SERVICE_UNAVAILABLE
					| StatusCode::GATEWAY_TIMEOUT
			),
			Self::Conflict => false,
			Self::InvalidUrl(_) => false,
			Self::Timeout => true,
			Self::UnexpectedStatus(status) => matches!(
				*status,
				StatusCode::TOO_MANY_REQUESTS
					| StatusCode::REQUEST_TIMEOUT
					| StatusCode::BAD_GATEWAY
					| StatusCode::SERVICE_UNAVAILABLE
					| StatusCode::GATEWAY_TIMEOUT
			),
		}
	}
}
