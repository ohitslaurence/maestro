// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights
// reserved. SPDX-License-Identifier: Proprietary

use loom_cli_git::GitError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AutoCommitError {
	#[error("git error: {0}")]
	Git(#[from] GitError),

	#[error("LLM error: {0}")]
	Llm(String),

	#[error("auto-commit disabled")]
	Disabled,

	#[error("no changes to commit")]
	NoChanges,

	#[error("not a git repository")]
	NotARepository,
}
