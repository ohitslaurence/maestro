// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! Core types for the Loom cron monitoring system.
//!
//! This crate provides shared types for cron/job monitoring including monitors,
//! check-ins, and statistics. It is used by both the server-side implementation
//! (`loom-server-crons`) and client SDKs.
//!
//! # Overview
//!
//! The cron monitoring system supports:
//! - Ping-based monitoring for shell scripts and cron jobs
//! - SDK-based check-ins for application code
//! - Missed run detection with configurable grace periods
//! - Timeout detection for long-running jobs
//! - Duration tracking and statistics

pub mod checkin;
pub mod error;
pub mod monitor;
pub mod sse;
pub mod stats;

pub use checkin::{
	truncate_output, CheckIn, CheckInId, CheckInSource, CheckInStatus, MAX_OUTPUT_BYTES,
};
pub use error::{CronsError, Result};
pub use monitor::{Monitor, MonitorHealth, MonitorId, MonitorSchedule, MonitorStatus, OrgId};
pub use sse::{CronStreamEvent, MonitorState};
pub use stats::{MonitorStats, StatsPeriod};
