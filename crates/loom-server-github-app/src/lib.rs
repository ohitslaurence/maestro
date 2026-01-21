// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights
// reserved. SPDX-License-Identifier: Proprietary

//! GitHub App client for Loom.
//!
//! This crate provides a typed Rust client for GitHub App authentication
//! and GitHub API operations, including code search and repository
//! introspection.

pub mod client;
pub mod config;
pub mod error;
pub mod jwt;
pub mod types;
pub mod webhook;

pub use client::GithubAppClient;
pub use config::GithubAppConfig;
pub use error::GithubAppError;
pub use loom_common_http::RetryConfig;
pub use types::{
	AppInfoResponse, CodeSearchItem, CodeSearchRequest, CodeSearchResponse, FileContents,
	FileContentsRequest, Installation, InstallationAccount, InstallationStatusResponse,
	RepoInfoRequest, Repository,
};
pub use webhook::verify_webhook_signature;
