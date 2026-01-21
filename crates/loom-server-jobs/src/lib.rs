// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! Background job scheduler for Loom server.
//!
//! This crate provides a job scheduling system for running periodic and one-shot
//! background tasks with retry support, health monitoring, and SQLite persistence.

pub mod context;
pub mod error;
pub mod health;
pub mod job;
pub mod repository;
pub mod scheduler;
pub mod types;

pub use context::{CancellationToken, JobContext};
pub use error::{JobError, Result};
pub use health::{HealthState, JobHealthStatus, JobsHealthStatus, LastRunInfo};
pub use job::Job;
pub use repository::JobRepository;
pub use scheduler::JobScheduler;
pub use types::{JobDefinition, JobOutput, JobRun, JobStatus, JobType, TriggerSource};
