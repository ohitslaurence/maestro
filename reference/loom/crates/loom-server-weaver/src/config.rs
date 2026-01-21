// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! Weaver provisioner configuration.

use serde::{Deserialize, Serialize};

/// Configuration for the weaver provisioner.
#[derive(Debug, Clone)]
pub struct WeaverConfig {
	/// Kubernetes namespace for weaver pods
	pub namespace: String,
	/// Cleanup task interval in seconds
	pub cleanup_interval_secs: u64,
	/// Default weaver TTL in hours
	pub default_ttl_hours: u32,
	/// Maximum weaver TTL in hours
	pub max_ttl_hours: u32,
	/// Maximum concurrent running weavers
	pub max_concurrent: u32,
	/// Timeout waiting for weaver ready state in seconds
	pub ready_timeout_secs: u64,
	/// Webhook configurations
	pub webhooks: Vec<WebhookConfig>,
	/// Image pull secret names for private registries (e.g., ghcr.io)
	pub image_pull_secrets: Vec<String>,
	/// URL to loom-server for secrets API (in-cluster: http://loom-server.loom.svc.cluster.local:8080)
	pub secrets_server_url: Option<String>,
	/// Allow insecure (HTTP) connections to secrets server (for in-cluster use)
	pub secrets_allow_insecure: bool,
	/// Enable WireGuard tunnel for weaver pods (for SSH access via wgtunnel)
	pub wg_enabled: bool,
	/// Enable audit sidecar
	pub audit_enabled: bool,
	/// Audit sidecar image
	pub audit_image: String,
	/// Audit batch interval in milliseconds
	pub audit_batch_interval_ms: u32,
	/// Audit buffer max size in bytes
	pub audit_buffer_max_bytes: u64,
	/// Loom server URL (injected as LOOM_SERVER_URL for CLI to connect to LLM proxy)
	pub server_url: String,
}

impl Default for WeaverConfig {
	fn default() -> Self {
		Self {
			namespace: "loom-weavers".to_string(),
			cleanup_interval_secs: 1800, // 30 minutes
			default_ttl_hours: 4,
			max_ttl_hours: 48,
			max_concurrent: 64,
			ready_timeout_secs: 60,
			webhooks: Vec::new(),
			image_pull_secrets: Vec::new(),
			secrets_server_url: None,
			secrets_allow_insecure: false,
			wg_enabled: false,
			audit_enabled: false,
			audit_image: "ghcr.io/ghuntley/loom-audit-sidecar:latest".to_string(),
			audit_batch_interval_ms: 100,
			audit_buffer_max_bytes: 256 * 1024 * 1024, // 256 MB
			server_url: String::new(),
		}
	}
}

/// Configuration for a webhook endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookConfig {
	/// Webhook URL
	pub url: String,
	/// Events to trigger this webhook
	pub events: Vec<WebhookEvent>,
	/// HMAC secret for signing payloads
	pub secret: Option<String>,
}

/// Events that can trigger webhooks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WebhookEvent {
	/// Weaver successfully created
	#[serde(rename = "weaver.created")]
	WeaverCreated,
	/// Weaver deleted (manual or cleanup)
	#[serde(rename = "weaver.deleted")]
	WeaverDeleted,
	/// Weaver entered failed state
	#[serde(rename = "weaver.failed")]
	WeaverFailed,
	/// Cleanup task completed
	#[serde(rename = "weavers.cleanup")]
	WeaversCleanup,
}
