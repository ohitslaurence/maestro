// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use std::path::PathBuf;

use thiserror::Error;

use crate::types::StitchId;

/// Errors that can occur during spool operations.
#[derive(Debug, Error)]
pub enum SpoolError {
	#[error("not a spool repository")]
	NotASpoolRepo,

	#[error("stitch not found: {0:?}")]
	StitchNotFound(StitchId),

	#[error("tangled path: {path}")]
	Tangled { path: PathBuf },

	#[error("pin already exists: {0}")]
	PinExists(String),

	#[error("pin not found: {0}")]
	PinNotFound(String),

	#[error("nothing to unpick")]
	NothingToUnpick,

	#[error("revset error: {0}")]
	Revset(String),

	#[error("backend error: {0}")]
	Backend(String),

	#[error("workspace error: {0}")]
	Workspace(String),

	#[error("transaction error: {0}")]
	Transaction(String),

	#[error("git operation failed: {0}")]
	Git(String),

	#[error("rewrite error: {0}")]
	Rewrite(String),

	#[error("config error: {0}")]
	Config(String),

	#[error("invalid argument: {0}")]
	InvalidArgument(String),

	#[error("operation aborted: {0}")]
	Aborted(String),

	#[error("I/O error: {0}")]
	Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, SpoolError>;

impl SpoolError {
	/// Create a backend error from any error type.
	pub fn backend<E: std::fmt::Display>(e: E) -> Self {
		Self::Backend(e.to_string())
	}

	/// Create a workspace error from any error type.
	pub fn workspace<E: std::fmt::Display>(e: E) -> Self {
		Self::Workspace(e.to_string())
	}

	/// Create a transaction error from any error type.
	pub fn transaction<E: std::fmt::Display>(e: E) -> Self {
		Self::Transaction(e.to_string())
	}

	/// Create a git error from any error type.
	pub fn git<E: std::fmt::Display>(e: E) -> Self {
		Self::Git(e.to_string())
	}

	/// Create a rewrite error from any error type.
	pub fn rewrite<E: std::fmt::Display>(e: E) -> Self {
		Self::Rewrite(e.to_string())
	}
}
