// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights
// reserved. SPDX-License-Identifier: Proprietary

mod config;
mod error;
mod generator;
mod service;

pub use config::AutoCommitConfig;
pub use error::AutoCommitError;
pub use generator::CommitMessageGenerator;
// Re-export loom-git types for convenience
pub use loom_cli_git::{CommandGitClient, GitClient, GitDiff, GitError, MockGitClient};
pub use service::{AutoCommitResult, AutoCommitService, CompletedToolInfo};
