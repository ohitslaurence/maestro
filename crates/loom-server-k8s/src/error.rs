// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights
// reserved. SPDX-License-Identifier: Proprietary

use thiserror::Error;

/// Result type alias for K8s operations.
pub type K8sResult<T> = Result<T, K8sError>;

/// Errors that can occur during K8s operations.
#[derive(Error, Debug)]
pub enum K8sError {
	#[error("K8s API error: {message}")]
	ApiError { message: String },

	#[error("Pod not found: {name}")]
	PodNotFound { name: String },

	#[error("Namespace not found: {name}")]
	NamespaceNotFound { name: String },

	#[error("Operation timed out")]
	Timeout,

	#[error("Log stream error: {message}")]
	StreamError { message: String },

	#[error("Attach error: {message}")]
	AttachError { message: String },

	#[error("TokenReview failed: {message}")]
	TokenReviewFailed { message: String },

	#[error("Token validation failed: {message}")]
	TokenValidationFailed { message: String },

	#[error("TokenReview API error: {message}")]
	TokenReviewError { message: String },
}

impl From<kube::Error> for K8sError {
	fn from(err: kube::Error) -> Self {
		K8sError::ApiError {
			message: err.to_string(),
		}
	}
}
