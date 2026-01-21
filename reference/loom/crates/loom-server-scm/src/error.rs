// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights
// reserved. SPDX-License-Identifier: Proprietary

use thiserror::Error;

pub type Result<T> = std::result::Result<T, ScmError>;

#[derive(Error, Debug)]
pub enum ScmError {
	#[error("repository not found")]
	NotFound,

	#[error("repository already exists")]
	AlreadyExists,

	#[error("permission denied")]
	PermissionDenied,

	#[error("invalid repository name: {0}")]
	InvalidName(String),

	#[error("database error: {0}")]
	Database(#[from] sqlx::Error),

	#[error("git error: {0}")]
	GitError(String),

	#[error("ref not found: {0}")]
	RefNotFound(String),

	#[error("object not found: {0}")]
	ObjectNotFound(String),

	#[error("io error: {0}")]
	Io(#[from] std::io::Error),

	#[error("branch protection violation: {0}")]
	ProtectionViolation(String),
}
