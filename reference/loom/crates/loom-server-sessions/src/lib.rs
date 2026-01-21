// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! Server-side implementation for Loom session analytics.
//!
//! This crate provides the database repository and background jobs
//! for session tracking and release health metrics.

pub mod error;
pub mod repository;

pub use error::{Result, SessionsServerError};
pub use repository::{SessionsRepository, SqliteSessionsRepository};
