// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use serde::{Deserialize, Serialize};
use std::time::Duration;

pub use loom_server_db::{JobDefinition, JobRun, JobStatus, TriggerSource};

#[derive(Debug, Clone)]
pub enum JobType {
	Periodic { interval: Duration },
	OneShot,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobOutput {
	pub message: String,
	pub metadata: Option<serde_json::Value>,
}
