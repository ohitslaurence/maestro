// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights
// reserved. SPDX-License-Identifier: Proprietary

//! Common configuration primitives for Loom.
//!
//! This crate provides shared types and helpers for configuration across
//! all Loom crates, including:
//!
//! - [`Secret<T>`]: A wrapper type that prevents accidental logging of
//!   sensitive values (re-exported from [`loom_common_secret`])
//! - [`load_secret_env`]: Helper for loading secrets from environment variables
//!   with `*_FILE` support

pub mod env;

// Re-export Secret types from loom-secret for convenience
pub use loom_common_secret::{Secret, SecretString, REDACTED};

pub use env::{load_secret_env, RequiredSecretError, SecretEnvError};
