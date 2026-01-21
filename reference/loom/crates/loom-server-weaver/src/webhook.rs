// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! Webhook dispatch system for weaver lifecycle events.

use crate::config::{WebhookConfig, WebhookEvent};
use crate::types::Weaver;
use chrono::{DateTime, Utc};
use serde::Serialize;
use std::collections::HashMap;
use tracing::{debug, error, warn};

/// Payload sent to webhook endpoints.
#[derive(Debug, Clone, Serialize)]
pub struct WebhookPayload {
	/// Event type (e.g., "weaver.created")
	pub event: String,
	/// When the event occurred
	pub timestamp: DateTime<Utc>,
	/// Weaver data for single-weaver events
	#[serde(skip_serializing_if = "Option::is_none")]
	pub weaver: Option<WebhookWeaverPayload>,
	/// Weaver data for multi-weaver events (cleanup)
	#[serde(skip_serializing_if = "Option::is_none")]
	pub weavers: Option<Vec<WebhookWeaverPayload>>,
	/// Count of affected weavers
	#[serde(skip_serializing_if = "Option::is_none")]
	pub count: Option<u32>,
	/// Error reason for failed events
	#[serde(skip_serializing_if = "Option::is_none")]
	pub reason: Option<String>,
}

/// Weaver data included in webhook payloads.
#[derive(Debug, Clone, Serialize)]
pub struct WebhookWeaverPayload {
	/// Weaver ID
	pub id: String,
	/// Container image
	#[serde(skip_serializing_if = "Option::is_none")]
	pub image: Option<String>,
	/// User-defined metadata tags
	pub tags: HashMap<String, String>,
}

impl WebhookWeaverPayload {
	fn from_weaver(weaver: &Weaver) -> Self {
		Self {
			id: weaver.id.to_string(),
			image: Some(weaver.image.clone()),
			tags: weaver.tags.clone(),
		}
	}
}

impl WebhookPayload {
	/// Create payload for weaver.created event.
	pub fn weaver_created(weaver: &Weaver) -> Self {
		Self {
			event: "weaver.created".to_string(),
			timestamp: Utc::now(),
			weaver: Some(WebhookWeaverPayload::from_weaver(weaver)),
			weavers: None,
			count: None,
			reason: None,
		}
	}

	/// Create payload for weaver.deleted event.
	pub fn weaver_deleted(weaver: &Weaver) -> Self {
		Self {
			event: "weaver.deleted".to_string(),
			timestamp: Utc::now(),
			weaver: Some(WebhookWeaverPayload::from_weaver(weaver)),
			weavers: None,
			count: None,
			reason: None,
		}
	}

	/// Create payload for weaver.failed event.
	pub fn weaver_failed(weaver: &Weaver, reason: &str) -> Self {
		Self {
			event: "weaver.failed".to_string(),
			timestamp: Utc::now(),
			weaver: Some(WebhookWeaverPayload::from_weaver(weaver)),
			weavers: None,
			count: None,
			reason: Some(reason.to_string()),
		}
	}

	/// Create payload for weavers.cleanup event.
	pub fn weavers_cleanup(weavers: &[Weaver]) -> Self {
		Self {
			event: "weavers.cleanup".to_string(),
			timestamp: Utc::now(),
			weaver: None,
			weavers: Some(
				weavers
					.iter()
					.map(WebhookWeaverPayload::from_weaver)
					.collect(),
			),
			count: Some(weavers.len() as u32),
			reason: None,
		}
	}
}

fn compute_signature(secret: &str, body: &str) -> String {
	loom_common_webhook::compute_hmac_sha256(secret.as_bytes(), body.as_bytes())
}

/// Dispatches webhook notifications for weaver lifecycle events.
#[derive(Clone)]
pub struct WebhookDispatcher {
	webhooks: Vec<WebhookConfig>,
	http_client: reqwest::Client,
}

impl WebhookDispatcher {
	/// Create a new webhook dispatcher.
	pub fn new(webhooks: Vec<WebhookConfig>) -> Self {
		let http_client = reqwest::Client::builder()
			.timeout(std::time::Duration::from_secs(30))
			.build()
			.expect("Failed to create HTTP client");

		Self {
			webhooks,
			http_client,
		}
	}

	/// Dispatch a webhook event to all matching endpoints.
	pub fn dispatch(&self, event: WebhookEvent, payload: WebhookPayload) {
		let matching_webhooks: Vec<_> = self
			.webhooks
			.iter()
			.filter(|w| w.events.contains(&event))
			.cloned()
			.collect();

		if matching_webhooks.is_empty() {
			debug!(?event, "No webhooks configured for event");
			return;
		}

		let body = match serde_json::to_string(&payload) {
			Ok(b) => b,
			Err(e) => {
				error!(?event, error = %e, "Failed to serialize webhook payload");
				return;
			}
		};

		for webhook in matching_webhooks {
			let client = self.http_client.clone();
			let body = body.clone();
			let url = webhook.url.clone();
			let secret = webhook.secret.clone();

			tokio::spawn(async move {
				let mut request = client
					.post(&url)
					.header("Content-Type", "application/json")
					.body(body.clone());

				if let Some(ref secret) = secret {
					let signature = compute_signature(secret, &body);
					request = request.header("X-Webhook-Signature", signature);
				}

				match request.send().await {
					Ok(response) => {
						if response.status().is_success() {
							debug!(url = %url, "Webhook delivered successfully");
						} else {
							warn!(
									url = %url,
									status = %response.status(),
									"Webhook returned non-success status"
							);
						}
					}
					Err(e) => {
						error!(url = %url, error = %e, "Failed to deliver webhook");
					}
				}
			});
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_compute_signature() {
		let secret = "test-secret";
		let body = r#"{"event":"weaver.created"}"#;
		let sig = compute_signature(secret, body);
		assert!(!sig.is_empty());
		assert_eq!(sig.len(), 64); // SHA256 hex = 64 chars
	}

	#[test]
	fn test_payload_serialization() {
		let payload = WebhookPayload {
			event: "weaver.created".to_string(),
			timestamp: Utc::now(),
			weaver: Some(WebhookWeaverPayload {
				id: "test-id".to_string(),
				image: Some("python:3.12".to_string()),
				tags: HashMap::new(),
			}),
			weavers: None,
			count: None,
			reason: None,
		};

		let json = serde_json::to_string(&payload).unwrap();
		assert!(json.contains("weaver.created"));
		assert!(json.contains("test-id"));
		assert!(!json.contains("weavers")); // None fields should be skipped
	}
}
