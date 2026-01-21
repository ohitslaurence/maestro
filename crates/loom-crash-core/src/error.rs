// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! Error types for crash analytics.

use thiserror::Error;

/// Errors that can occur in the crash analytics system.
#[derive(Debug, Error)]
pub enum CrashError {
	#[error("project not found: {0}")]
	ProjectNotFound(String),

	#[error("issue not found: {0}")]
	IssueNotFound(String),

	#[error("event not found: {0}")]
	EventNotFound(String),

	#[error("artifact not found: {0}")]
	ArtifactNotFound(String),

	#[error("release not found: {0}")]
	ReleaseNotFound(String),

	#[error("invalid API key")]
	InvalidApiKey,

	#[error("API key revoked")]
	ApiKeyRevoked,

	#[error("rate limit exceeded")]
	RateLimitExceeded,

	#[error("invalid fingerprint: {0}")]
	InvalidFingerprint(String),

	#[error("invalid source map: {0}")]
	InvalidSourceMap(String),

	#[error("invalid VLQ character: {0}")]
	InvalidVlqChar(char),

	#[error("invalid source index in source map")]
	InvalidSourceIndex,

	#[error("invalid platform: {0}")]
	InvalidPlatform(String),

	#[error("invalid issue status: {0}")]
	InvalidIssueStatus(String),

	#[error("invalid issue level: {0}")]
	InvalidIssueLevel(String),

	#[error("invalid issue priority: {0}")]
	InvalidIssuePriority(String),

	#[error("invalid artifact type: {0}")]
	InvalidArtifactType(String),

	#[error("invalid key type: {0}")]
	InvalidKeyType(String),

	#[error("invalid breadcrumb level: {0}")]
	InvalidBreadcrumbLevel(String),

	#[error("event payload too large: {size} bytes (max: {max})")]
	PayloadTooLarge { size: usize, max: usize },

	#[error("too many stacktrace frames: {count} (max: {max})")]
	TooManyFrames { count: usize, max: usize },

	#[error("serialization error: {0}")]
	Serialization(#[from] serde_json::Error),

	#[error("unauthorized: {0}")]
	Unauthorized(String),

	#[error("forbidden: {0}")]
	Forbidden(String),

	#[error("internal error: {0}")]
	Internal(String),
}

/// Result type for crash analytics operations.
pub type Result<T> = std::result::Result<T, CrashError>;
