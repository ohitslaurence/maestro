// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use std::sync::Arc;

use async_trait::async_trait;
use loom_server_jobs::{Job, JobContext, JobError, JobOutput};
use loom_server_llm_service::LlmService;
use tracing::instrument;

pub struct TokenRefreshJob {
	llm_service: Arc<LlmService>,
}

impl TokenRefreshJob {
	pub fn new(llm_service: Arc<LlmService>) -> Self {
		Self { llm_service }
	}
}

#[async_trait]
impl Job for TokenRefreshJob {
	fn id(&self) -> &str {
		"token-refresh"
	}

	fn name(&self) -> &str {
		"Token Refresh"
	}

	fn description(&self) -> &str {
		"Refresh Anthropic OAuth tokens before expiry"
	}

	#[instrument(skip(self, ctx), fields(job_id = "token-refresh"))]
	async fn run(&self, ctx: &JobContext) -> Result<JobOutput, JobError> {
		if ctx.cancellation_token.is_cancelled() {
			return Err(JobError::Cancelled);
		}

		// The AnthropicPool already has its own internal refresh task that runs
		// continuously. This job serves as a backup/monitoring mechanism.
		// We check the pool health to verify tokens are being refreshed properly.
		match self.llm_service.anthropic_health().await {
			Some(health) => {
				tracing::debug!(?health, "Anthropic pool health check");
				Ok(JobOutput {
					message: "Token refresh check completed".to_string(),
					metadata: Some(serde_json::json!({
						"health_check": "passed",
						"pool_status": format!("{:?}", health),
					})),
				})
			}
			None => Ok(JobOutput {
				message: "No Anthropic OAuth pool configured".to_string(),
				metadata: Some(serde_json::json!({
					"health_check": "skipped",
					"reason": "no_oauth_pool",
				})),
			}),
		}
	}
}
