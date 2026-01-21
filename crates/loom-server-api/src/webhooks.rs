// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use chrono::{DateTime, Utc};
use loom_server_scm::{PayloadFormat, Webhook};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

#[derive(Debug, Clone, Default, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "kebab-case")]
pub enum PayloadFormatApi {
	/// GitHub-compatible webhook payload format
	#[serde(rename = "github-compat")]
	GitHubCompat,
	#[default]
	LoomV1,
}

impl From<PayloadFormat> for PayloadFormatApi {
	fn from(v: PayloadFormat) -> Self {
		match v {
			PayloadFormat::GitHubCompat => PayloadFormatApi::GitHubCompat,
			PayloadFormat::LoomV1 => PayloadFormatApi::LoomV1,
		}
	}
}

impl From<PayloadFormatApi> for PayloadFormat {
	fn from(v: PayloadFormatApi) -> Self {
		match v {
			PayloadFormatApi::GitHubCompat => PayloadFormat::GitHubCompat,
			PayloadFormatApi::LoomV1 => PayloadFormat::LoomV1,
		}
	}
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateWebhookRequest {
	pub url: String,
	pub secret: String,
	#[serde(default)]
	pub payload_format: PayloadFormatApi,
	pub events: Vec<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct WebhookResponse {
	pub id: Uuid,
	pub url: String,
	pub payload_format: PayloadFormatApi,
	pub events: Vec<String>,
	pub enabled: bool,
	pub created_at: DateTime<Utc>,
}

impl From<Webhook> for WebhookResponse {
	fn from(w: Webhook) -> Self {
		Self {
			id: w.id,
			url: w.url,
			payload_format: w.payload_format.into(),
			events: w.events,
			enabled: w.enabled,
			created_at: w.created_at,
		}
	}
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ListWebhooksResponse {
	pub webhooks: Vec<WebhookResponse>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct WebhookSuccessResponse {
	pub message: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct WebhookErrorResponse {
	pub error: String,
	pub message: String,
}
