// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Utc};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::sync::{mpsc, RwLock};

use crate::config::Config;
use crate::events::WeaverAuditEvent;
use crate::health::HealthState;
use crate::metrics::Metrics;

#[derive(Error, Debug)]
pub enum ClientError {
	#[error("HTTP error: {0}")]
	Http(#[from] reqwest::Error),

	#[error("IO error: {0}")]
	Io(#[from] std::io::Error),

	#[error("server returned error: {status} - {body}")]
	ServerError { status: u16, body: String },

	#[error("authentication required but SA token could not be read: {0}")]
	AuthRequired(std::io::Error),

	#[error("SVID exchange failed: {0}")]
	SvidExchangeFailed(String),

	#[error("JSON parse error: {0}")]
	JsonParse(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, ClientError>;

/// Request body for SVID token exchange (matches server's TokenRequest)
#[derive(Debug, Clone, Serialize)]
struct SvidExchangeRequest {
	pod_name: String,
	pod_namespace: String,
}

/// Request body format expected by the server
#[derive(Debug, Clone, Serialize)]
struct WeaverAuditRequest {
	weaver_id: String,
	org_id: String,
	events: Vec<WeaverAuditEventPayload>,
}

/// Event payload format expected by the server
#[derive(Debug, Clone, Serialize)]
struct WeaverAuditEventPayload {
	timestamp_ns: u64,
	pid: u32,
	tid: u32,
	comm: String,
	event_type: String,
	details: serde_json::Value,
}

/// Response from SVID token exchange (matches server's SvidResponse)
#[derive(Debug, Clone, Deserialize)]
struct SvidExchangeResponse {
	token: String,
	#[allow(dead_code)]
	token_type: String,
	expires_at: DateTime<Utc>,
	#[allow(dead_code)]
	spiffe_id: String,
}

#[derive(Debug, Clone)]
struct CachedSvid {
	token: String,
	expires_at: DateTime<Utc>,
}

impl CachedSvid {
	fn is_valid(&self) -> bool {
		Utc::now() + chrono::Duration::seconds(30) < self.expires_at
	}
}

#[derive(Clone)]
pub struct AuditClient {
	client: Client,
	server_url: String,
	weaver_id: String,
	org_id: String,
	pod_name: String,
	pod_namespace: String,
	sa_token_path: String,
	allow_no_auth: bool,
	cached_svid: Arc<RwLock<Option<CachedSvid>>>,
}

impl AuditClient {
	pub fn new(config: &Config) -> Result<Self> {
		let client = Client::builder()
			.timeout(Duration::from_secs(30))
			.connect_timeout(Duration::from_secs(5))
			.build()?;

		Ok(AuditClient {
			client,
			server_url: config.server_url.clone(),
			weaver_id: config.weaver_id.clone(),
			org_id: config.org_id.clone(),
			pod_name: config.pod_name.clone(),
			pod_namespace: config.pod_namespace.clone(),
			sa_token_path: config.sa_token_path.clone(),
			allow_no_auth: config.allow_no_auth,
			cached_svid: Arc::new(RwLock::new(None)),
		})
	}

	/// Send events to the server, automatically chunking large batches.
	///
	/// If the batch exceeds MAX_BATCH_SIZE, it will be split into multiple
	/// requests. If any chunk fails, returns an error (partial success is
	/// still possible - successfully sent events won't be retried).
	pub async fn send_events(&self, events: &[WeaverAuditEvent]) -> Result<()> {
		if events.is_empty() {
			return Ok(());
		}

		// Chunk large batches to stay within server limits (Option 1)
		for chunk in events.chunks(MAX_BATCH_SIZE) {
			self.send_events_chunk(chunk).await?;
		}

		Ok(())
	}

	/// Send a single chunk of events (must be <= MAX_BATCH_SIZE).
	async fn send_events_chunk(&self, events: &[WeaverAuditEvent]) -> Result<()> {
		debug_assert!(
			events.len() <= MAX_BATCH_SIZE,
			"Chunk size {} exceeds MAX_BATCH_SIZE {}",
			events.len(),
			MAX_BATCH_SIZE
		);

		// Convert events to the payload format expected by the server
		let event_payloads: Vec<WeaverAuditEventPayload> = events
			.iter()
			.map(|e| WeaverAuditEventPayload {
				timestamp_ns: e.timestamp_ns,
				pid: e.pid,
				tid: e.tid,
				comm: e.comm.clone(),
				// Serialize event_type using serde to get snake_case format
				event_type: serde_json::to_value(e.event_type)
					.and_then(serde_json::from_value)
					.unwrap_or_else(|_| format!("{:?}", e.event_type).to_lowercase()),
				details: e.details.clone(),
			})
			.collect();

		let request_body = WeaverAuditRequest {
			weaver_id: self.weaver_id.clone(),
			org_id: self.org_id.clone(),
			events: event_payloads,
		};

		let url = format!("{}/internal/weaver-audit/events", self.server_url);
		let mut request = self.client.post(&url).json(&request_body);

		match self.get_or_refresh_svid().await {
			Ok(token) => {
				request = request.bearer_auth(token);
			}
			Err(e) => {
				if self.allow_no_auth {
					tracing::warn!(
						"Running without authentication (LOOM_AUDIT_ALLOW_NO_AUTH=true). \
						 This should only be used for development."
					);
				} else {
					return Err(e);
				}
			}
		}

		let response = request.send().await?;
		let status = response.status();

		if !status.is_success() {
			let body = response.text().await.unwrap_or_default();
			return Err(ClientError::ServerError {
				status: status.as_u16(),
				body,
			});
		}

		Ok(())
	}

	pub async fn is_server_reachable(&self) -> bool {
		let url = format!("{}/health", self.server_url);
		match self
			.client
			.get(&url)
			.timeout(Duration::from_secs(5))
			.send()
			.await
		{
			Ok(resp) => resp.status().is_success(),
			Err(_) => false,
		}
	}

	async fn get_or_refresh_svid(&self) -> Result<String> {
		{
			let cached = self.cached_svid.read().await;
			if let Some(ref svid) = *cached {
				if svid.is_valid() {
					return Ok(svid.token.clone());
				}
			}
		}

		let mut cached = self.cached_svid.write().await;
		if let Some(ref svid) = *cached {
			if svid.is_valid() {
				return Ok(svid.token.clone());
			}
		}

		let sa_token = self
			.read_sa_token()
			.await
			.map_err(ClientError::AuthRequired)?;

		match self.exchange_sa_token_for_svid(&sa_token).await {
			Ok(svid_response) => {
				let new_svid = CachedSvid {
					token: svid_response.token.clone(),
					expires_at: svid_response.expires_at,
				};
				*cached = Some(new_svid);
				tracing::debug!(
					expires_at = %svid_response.expires_at,
					spiffe_id = %svid_response.spiffe_id,
					"Successfully obtained SVID"
				);
				Ok(svid_response.token)
			}
			Err(e) => {
				tracing::warn!(
					error = %e,
					"SVID exchange failed, falling back to SA token. \
					 This may indicate the auth endpoint is not yet available."
				);
				Ok(sa_token)
			}
		}
	}

	async fn exchange_sa_token_for_svid(&self, sa_token: &str) -> Result<SvidExchangeResponse> {
		let url = format!("{}/internal/weaver-auth/token", self.server_url);

		let request_body = SvidExchangeRequest {
			pod_name: self.pod_name.clone(),
			pod_namespace: self.pod_namespace.clone(),
		};

		let response = self
			.client
			.post(&url)
			.bearer_auth(sa_token)
			.json(&request_body)
			.send()
			.await?;

		let status = response.status();
		if !status.is_success() {
			let body = response.text().await.unwrap_or_default();
			return Err(ClientError::SvidExchangeFailed(format!(
				"status {}: {}",
				status, body
			)));
		}

		let svid_response: SvidExchangeResponse = response.json().await?;
		Ok(svid_response)
	}

	async fn read_sa_token(&self) -> std::result::Result<String, std::io::Error> {
		let token = tokio::fs::read_to_string(&self.sa_token_path).await?;
		Ok(token.trim().to_string())
	}
}

pub struct BatchSender {
	tx: mpsc::Sender<WeaverAuditEvent>,
}

impl BatchSender {
	pub fn new(
		client: AuditClient,
		batch_interval: Duration,
		buffer_tx: mpsc::Sender<WeaverAuditEvent>,
		health_state: HealthState,
		metrics: std::sync::Arc<Metrics>,
	) -> Self {
		let (tx, rx) = mpsc::channel(10000);
		tokio::spawn(batch_loop(
			client,
			rx,
			batch_interval,
			buffer_tx,
			health_state,
			metrics,
		));
		BatchSender { tx }
	}

	pub fn sender(&self) -> &mpsc::Sender<WeaverAuditEvent> {
		&self.tx
	}

	#[allow(dead_code)] // Public API for manual event sending
	pub async fn send(
		&self,
		event: WeaverAuditEvent,
	) -> std::result::Result<(), mpsc::error::SendError<WeaverAuditEvent>> {
		self.tx.send(event).await
	}
}

/// Maximum number of events per batch (must match server's MAX_EVENTS_PER_BATCH)
const MAX_BATCH_SIZE: usize = 1000;

async fn batch_loop(
	client: AuditClient,
	mut rx: mpsc::Receiver<WeaverAuditEvent>,
	batch_interval: Duration,
	buffer_tx: mpsc::Sender<WeaverAuditEvent>,
	health_state: HealthState,
	metrics: std::sync::Arc<Metrics>,
) {
	let mut batch = Vec::with_capacity(MAX_BATCH_SIZE);
	let mut interval = tokio::time::interval(batch_interval);

	loop {
		tokio::select! {
			_ = interval.tick() => {
				if !batch.is_empty() {
					let events = std::mem::replace(&mut batch, Vec::with_capacity(MAX_BATCH_SIZE));
					send_batch(&client, events, &buffer_tx, &health_state, &metrics).await;
				}
			}
			event = rx.recv() => {
				match event {
					Some(e) => {
						batch.push(e);
						// Flush immediately when batch reaches max size (Option 2)
						if batch.len() >= MAX_BATCH_SIZE {
							let events = std::mem::replace(&mut batch, Vec::with_capacity(MAX_BATCH_SIZE));
							send_batch(&client, events, &buffer_tx, &health_state, &metrics).await;
							// Reset interval to avoid double-flush
							interval.reset();
						}
					}
					None => break,
				}
			}
		}
	}
}

/// Send a batch of events to the server, buffering on failure.
async fn send_batch(
	client: &AuditClient,
	events: Vec<WeaverAuditEvent>,
	buffer_tx: &mpsc::Sender<WeaverAuditEvent>,
	health_state: &HealthState,
	metrics: &Metrics,
) {
	let event_count = events.len();
	match client.send_events(&events).await {
		Ok(()) => {
			tracing::debug!("Successfully sent {} events to server", event_count);
			health_state.record_successful_send().await;
			metrics.record_batch_sent(true, event_count, 0);
			for event in &events {
				metrics.record_event_sent(event.event_type);
			}
		}
		Err(e) => {
			tracing::warn!(
				"Failed to send batch: {}, buffering {} events",
				e,
				event_count
			);
			metrics.record_batch_sent(false, event_count, 0);
			for event in events {
				metrics.record_event_buffered(event.event_type);
				let _ = buffer_tx.send(event).await;
			}
		}
	}
}
