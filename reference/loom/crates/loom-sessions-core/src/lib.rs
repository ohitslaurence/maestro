// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! Core types for the Loom session analytics system.
//!
//! This crate provides shared types for session tracking, aggregation,
//! and release health metrics.
//!
//! ## Key Types
//!
//! - [`Session`] - A single user engagement period
//! - [`SessionAggregate`] - Hourly rollup of session data
//! - [`ReleaseHealth`] - Computed metrics for a release
//!
//! ## Example
//!
//! ```rust
//! use loom_sessions_core::{Session, SessionStatus, Platform};
//!
//! let status = SessionStatus::Active;
//! let platform = Platform::JavaScript;
//! ```

pub mod aggregate;
pub mod error;
pub mod release_health;
pub mod session;

pub use aggregate::{SessionAggregate, SessionAggregateId};
pub use error::SessionsError;
pub use release_health::{AdoptionStage, ReleaseHealth};
pub use session::{Platform, Session, SessionId, SessionStatus};

/// Result type for sessions operations.
pub type Result<T> = std::result::Result<T, SessionsError>;
