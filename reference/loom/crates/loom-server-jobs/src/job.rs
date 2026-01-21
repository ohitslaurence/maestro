// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use crate::context::JobContext;
use crate::error::JobError;
use crate::types::JobOutput;
use async_trait::async_trait;

#[async_trait]
pub trait Job: Send + Sync {
	fn id(&self) -> &str;
	fn name(&self) -> &str;
	fn description(&self) -> &str;
	async fn run(&self, ctx: &JobContext) -> Result<JobOutput, JobError>;
}
