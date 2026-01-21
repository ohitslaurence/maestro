// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! Feature flags server implementation for Loom.
//!
//! This crate provides the server-side implementation for the feature flags system,
//! including database operations, flag evaluation, and SDK key management.
//!
//! # Architecture
//!
//! - `repository` - Database operations for flags, strategies, kill switches, etc.
//! - `evaluation` - Server-side flag evaluation engine
//! - `sdk_auth` - SDK key authentication and hashing
//!
//! # Example
//!
//! ```ignore
//! use loom_server_flags::{SqliteFlagsRepository, FlagsRepository, evaluate_flag};
//! use loom_flags_core::EvaluationContext;
//!
//! // Create repository
//! let repo = SqliteFlagsRepository::new(pool);
//!
//! // Get flag and config
//! let flag = repo.get_flag_by_key(Some(org_id), "feature.new_flow").await?;
//! let config = repo.get_flag_config(flag.id, env_id).await?;
//!
//! // Evaluate
//! let context = EvaluationContext::new("prod").with_user_id("user123");
//! let result = evaluate_flag(&flag, Some(&config), None, &[], &[], &context);
//! ```

pub mod error;
pub mod evaluation;
pub mod repository;
pub mod sdk_auth;
pub mod sse;

pub use error::{FlagsServerError, Result};
pub use evaluation::evaluate_flag;
pub use repository::{FlagsRepository, SqliteFlagsRepository};
pub use sdk_auth::{hash_sdk_key, verify_sdk_key};
pub use sse::{BroadcasterConfig, BroadcasterStats, ChannelKey, ChannelStats, FlagsBroadcaster};

// Re-export core types for convenience
pub use loom_flags_core::*;
