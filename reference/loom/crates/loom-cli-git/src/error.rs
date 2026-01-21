// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights
// reserved. SPDX-License-Identifier: Proprietary

use std::io;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum GitError {
	#[error("not a git repository: {0}")]
	NotAGitRepo(String),

	#[error("git command failed: {cmd} {args:?}: {stderr}")]
	CommandFailed {
		cmd: &'static str,
		args: Vec<String>,
		stderr: String,
	},

	#[error("git is not installed or not in PATH")]
	GitNotInstalled,

	#[error("I/O error: {0}")]
	Io(#[from] io::Error),
}
