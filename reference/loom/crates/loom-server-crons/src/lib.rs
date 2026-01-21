// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! Cron monitoring server implementation for Loom.
//!
//! This crate provides the server-side implementation for the cron monitoring
//! system, including:
//!
//! - Repository layer for database operations
//! - Schedule parsing and next run calculation
//! - SSE broadcasting for real-time updates

pub mod error;
pub mod repository;
pub mod schedule;
pub mod sse;

pub use error::{CronsServerError, Result};
pub use repository::{CronsRepository, SqliteCronsRepository};
pub use schedule::{calculate_next_expected, validate_cron_expression, validate_timezone};
pub use sse::{BroadcasterStats, CronsBroadcaster, CronsBroadcasterConfig};
