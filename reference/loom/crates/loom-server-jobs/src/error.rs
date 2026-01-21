// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use loom_server_db::DbError;

#[derive(Debug, thiserror::Error)]
pub enum JobError {
	#[error("Job failed: {message}")]
	Failed { message: String, retryable: bool },

	#[error("Job cancelled")]
	Cancelled,

	#[error("Database error: {0}")]
	Database(#[from] sqlx::Error),

	#[error("Repository error: {0}")]
	Repository(#[from] DbError),

	#[error("Job not found: {0}")]
	NotFound(String),
}

pub type Result<T> = std::result::Result<T, JobError>;
