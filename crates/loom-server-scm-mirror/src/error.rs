// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights
// reserved. SPDX-License-Identifier: Proprietary

use thiserror::Error;

pub type Result<T> = std::result::Result<T, MirrorError>;

#[derive(Error, Debug)]
pub enum MirrorError {
	#[error("mirror not found")]
	NotFound,

	#[error("mirror already exists")]
	AlreadyExists,

	#[error("credential not found: {0}")]
	CredentialNotFound(String),

	#[error("git operation failed: {0}")]
	GitError(String),

	#[error("platform API error: {0}")]
	PlatformError(String),

	#[error("invalid URL: {0}")]
	InvalidUrl(String),

	#[error("database error: {0}")]
	Database(#[from] sqlx::Error),

	#[error("db error: {0}")]
	Db(#[from] loom_server_db::DbError),

	#[error("io error: {0}")]
	Io(#[from] std::io::Error),

	#[error("http error: {0}")]
	Http(#[from] reqwest::Error),

	#[error("scm error: {0}")]
	Scm(#[from] loom_server_scm::ScmError),
}
