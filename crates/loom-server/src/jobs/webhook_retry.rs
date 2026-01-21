// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use std::sync::Arc;

use async_trait::async_trait;
use chrono::Utc;
use loom_server_jobs::{Job, JobContext, JobError, JobOutput};
use loom_server_scm::{delivery, DeliveryStatus, WebhookStore};
use tracing::instrument;

const MAX_RETRY_ATTEMPTS: i32 = 3;
const RETRY_BACKOFF_BASE_SECS: i64 = 60;

pub struct WebhookRetryJob<S: WebhookStore> {
	store: Arc<S>,
	client: reqwest::Client,
}

impl<S: WebhookStore + 'static> WebhookRetryJob<S> {
	pub fn new(store: Arc<S>, client: reqwest::Client) -> Self {
		Self { store, client }
	}
}

#[async_trait]
impl<S: WebhookStore + 'static> Job for WebhookRetryJob<S> {
	fn id(&self) -> &str {
		"webhook-retry"
	}

	fn name(&self) -> &str {
		"Webhook Retry"
	}

	fn description(&self) -> &str {
		"Process pending webhook deliveries with retry logic"
	}

	#[instrument(skip(self, ctx), fields(job_id = "webhook-retry"))]
	async fn run(&self, ctx: &JobContext) -> Result<JobOutput, JobError> {
		if ctx.cancellation_token.is_cancelled() {
			return Err(JobError::Cancelled);
		}

		let pending = self
			.store
			.get_pending_deliveries()
			.await
			.map_err(|e| JobError::Failed {
				message: e.to_string(),
				retryable: true,
			})?;

		if pending.is_empty() {
			return Ok(JobOutput {
				message: "No pending webhook deliveries".to_string(),
				metadata: Some(serde_json::json!({
					"processed": 0,
					"succeeded": 0,
					"failed": 0,
					"retrying": 0,
				})),
			});
		}

		let mut processed = 0;
		let mut succeeded = 0;
		let mut failed = 0;
		let mut retrying = 0;

		for mut delivery_record in pending {
			if ctx.cancellation_token.is_cancelled() {
				return Err(JobError::Cancelled);
			}

			let now = Utc::now();
			if let Some(next_retry_at) = delivery_record.next_retry_at {
				if now < next_retry_at {
					continue;
				}
			}

			let webhook = match self
				.store
				.get_webhook_for_delivery(delivery_record.id)
				.await
			{
				Ok(Some(w)) => w,
				Ok(None) => {
					delivery_record.status = DeliveryStatus::Failed;
					delivery_record.response_body = Some("Webhook not found".to_string());
					let _ = self.store.update_delivery(&delivery_record).await;
					failed += 1;
					processed += 1;
					continue;
				}
				Err(e) => {
					tracing::warn!(
						delivery_id = %delivery_record.id,
						error = %e,
						"Failed to get webhook for delivery"
					);
					continue;
				}
			};

			delivery_record.attempts += 1;

			let result = delivery::deliver(
				&webhook,
				&delivery_record.event,
				delivery_record.payload.clone(),
				&self.client,
			)
			.await;

			match result {
				Ok(delivery_result) => {
					delivery_record.response_code = delivery_result.status_code.map(|c| c as i32);
					delivery_record.response_body = delivery_result.body;
					delivery_record.delivered_at = Some(Utc::now());

					if delivery_result.success {
						delivery_record.status = DeliveryStatus::Success;
						delivery_record.next_retry_at = None;
						succeeded += 1;
						tracing::info!(
							delivery_id = %delivery_record.id,
							webhook_id = %webhook.id,
							attempts = delivery_record.attempts,
							"Webhook delivery succeeded"
						);
					} else if delivery_record.attempts >= MAX_RETRY_ATTEMPTS {
						delivery_record.status = DeliveryStatus::Failed;
						delivery_record.next_retry_at = None;
						failed += 1;
						tracing::warn!(
							delivery_id = %delivery_record.id,
							webhook_id = %webhook.id,
							attempts = delivery_record.attempts,
							response_code = ?delivery_record.response_code,
							"Webhook delivery failed after max retries"
						);
					} else {
						let backoff_secs =
							RETRY_BACKOFF_BASE_SECS * (2_i64.pow(delivery_record.attempts as u32 - 1));
						delivery_record.next_retry_at =
							Some(Utc::now() + chrono::Duration::seconds(backoff_secs));
						retrying += 1;
						tracing::info!(
							delivery_id = %delivery_record.id,
							webhook_id = %webhook.id,
							attempts = delivery_record.attempts,
							next_retry_secs = backoff_secs,
							"Webhook delivery will retry"
						);
					}
				}
				Err(e) => {
					delivery_record.response_body = Some(e.to_string());

					if delivery_record.attempts >= MAX_RETRY_ATTEMPTS {
						delivery_record.status = DeliveryStatus::Failed;
						delivery_record.next_retry_at = None;
						failed += 1;
						tracing::warn!(
							delivery_id = %delivery_record.id,
							webhook_id = %webhook.id,
							attempts = delivery_record.attempts,
							error = %e,
							"Webhook delivery failed after max retries"
						);
					} else {
						let backoff_secs =
							RETRY_BACKOFF_BASE_SECS * (2_i64.pow(delivery_record.attempts as u32 - 1));
						delivery_record.next_retry_at =
							Some(Utc::now() + chrono::Duration::seconds(backoff_secs));
						retrying += 1;
						tracing::info!(
							delivery_id = %delivery_record.id,
							webhook_id = %webhook.id,
							attempts = delivery_record.attempts,
							next_retry_secs = backoff_secs,
							error = %e,
							"Webhook delivery will retry after error"
						);
					}
				}
			}

			if let Err(e) = self.store.update_delivery(&delivery_record).await {
				tracing::error!(
					delivery_id = %delivery_record.id,
					error = %e,
					"Failed to update delivery record"
				);
			}

			processed += 1;
		}

		tracing::info!(
			processed,
			succeeded,
			failed,
			retrying,
			"Webhook retry job completed"
		);

		Ok(JobOutput {
			message: format!(
				"Processed {} deliveries: {} succeeded, {} failed, {} retrying",
				processed, succeeded, failed, retrying
			),
			metadata: Some(serde_json::json!({
				"processed": processed,
				"succeeded": succeeded,
				"failed": failed,
				"retrying": retrying,
			})),
		})
	}
}
