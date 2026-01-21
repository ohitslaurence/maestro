// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! SSE (Server-Sent Events) connection for real-time flag updates.
//!
//! This module manages the SSE connection to the server for receiving
//! real-time updates to flag configurations and kill switch states.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use eventsource_stream::{Event, Eventsource};
use futures::StreamExt;
use loom_flags_core::FlagStreamEvent;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tracing::{debug, error, info, warn};

use crate::cache::FlagCache;
use crate::error::{FlagsError, Result};

/// Configuration for SSE connection behavior.
#[derive(Debug, Clone)]
pub struct SseConfig {
	/// Base delay for reconnection attempts.
	pub reconnect_base_delay: Duration,
	/// Maximum delay for reconnection attempts.
	pub reconnect_max_delay: Duration,
	/// Maximum number of reconnection attempts (0 = unlimited).
	pub max_reconnect_attempts: u32,
	/// Whether to use exponential backoff for reconnection.
	pub use_exponential_backoff: bool,
}

impl Default for SseConfig {
	fn default() -> Self {
		Self {
			reconnect_base_delay: Duration::from_secs(1),
			reconnect_max_delay: Duration::from_secs(30),
			max_reconnect_attempts: 0, // Unlimited
			use_exponential_backoff: true,
		}
	}
}

/// Manages an SSE connection for real-time flag updates.
#[derive(Debug)]
pub struct SseConnection {
	/// Whether the connection is currently active.
	connected: Arc<AtomicBool>,
	/// Number of reconnection attempts.
	reconnect_attempts: Arc<AtomicU64>,
	/// Number of events received.
	events_received: Arc<AtomicU64>,
	/// Handle to the background task.
	task_handle: Option<JoinHandle<()>>,
	/// Channel to signal shutdown.
	shutdown_tx: Option<mpsc::Sender<()>>,
}

impl SseConnection {
	/// Creates a new SSE connection manager.
	pub fn new() -> Self {
		Self {
			connected: Arc::new(AtomicBool::new(false)),
			reconnect_attempts: Arc::new(AtomicU64::new(0)),
			events_received: Arc::new(AtomicU64::new(0)),
			task_handle: None,
			shutdown_tx: None,
		}
	}

	/// Starts the SSE connection in a background task.
	///
	/// The connection will automatically reconnect on failure with exponential backoff.
	pub async fn start(
		&mut self,
		stream_url: String,
		sdk_key: String,
		cache: FlagCache,
		config: SseConfig,
	) -> Result<()> {
		// If already running, stop first
		self.stop().await;

		let (shutdown_tx, shutdown_rx) = mpsc::channel::<()>(1);
		self.shutdown_tx = Some(shutdown_tx);

		let connected = Arc::clone(&self.connected);
		let reconnect_attempts = Arc::clone(&self.reconnect_attempts);
		let events_received = Arc::clone(&self.events_received);

		let handle = tokio::spawn(async move {
			run_sse_loop(
				stream_url,
				sdk_key,
				cache,
				config,
				connected,
				reconnect_attempts,
				events_received,
				shutdown_rx,
			)
			.await;
		});

		self.task_handle = Some(handle);
		Ok(())
	}

	/// Stops the SSE connection.
	pub async fn stop(&mut self) {
		if let Some(tx) = self.shutdown_tx.take() {
			let _ = tx.send(()).await;
		}
		if let Some(handle) = self.task_handle.take() {
			handle.abort();
			let _ = handle.await;
		}
		self.connected.store(false, Ordering::SeqCst);
	}

	/// Returns true if the SSE connection is currently active.
	pub fn is_connected(&self) -> bool {
		self.connected.load(Ordering::SeqCst)
	}

	/// Returns the number of reconnection attempts since the connection was started.
	pub fn reconnect_attempts(&self) -> u64 {
		self.reconnect_attempts.load(Ordering::SeqCst)
	}

	/// Returns the number of events received since the connection was started.
	pub fn events_received(&self) -> u64 {
		self.events_received.load(Ordering::SeqCst)
	}
}

impl Default for SseConnection {
	fn default() -> Self {
		Self::new()
	}
}

impl Drop for SseConnection {
	fn drop(&mut self) {
		if let Some(handle) = self.task_handle.take() {
			handle.abort();
		}
	}
}

/// Runs the SSE connection loop with reconnection logic.
#[allow(clippy::too_many_arguments)]
async fn run_sse_loop(
	stream_url: String,
	sdk_key: String,
	cache: FlagCache,
	config: SseConfig,
	connected: Arc<AtomicBool>,
	reconnect_attempts: Arc<AtomicU64>,
	events_received: Arc<AtomicU64>,
	mut shutdown_rx: mpsc::Receiver<()>,
) {
	let mut consecutive_failures: u32 = 0;

	loop {
		// Check for shutdown signal
		if shutdown_rx.try_recv().is_ok() {
			info!("SSE connection received shutdown signal");
			break;
		}

		info!(url = %stream_url, "Connecting to SSE stream");

		match connect_and_process(&stream_url, &sdk_key, &cache, &connected, &events_received).await {
			Ok(()) => {
				// Normal disconnect (e.g., server closed connection)
				debug!("SSE stream ended normally");
				consecutive_failures = 0;
			}
			Err(e) => {
				error!(error = %e, "SSE connection error");
				consecutive_failures += 1;
			}
		}

		connected.store(false, Ordering::SeqCst);

		// Check max reconnect attempts
		if config.max_reconnect_attempts > 0 && consecutive_failures >= config.max_reconnect_attempts {
			error!(
				attempts = consecutive_failures,
				"Max reconnection attempts reached, stopping SSE"
			);
			break;
		}

		// Calculate backoff delay
		let delay = if config.use_exponential_backoff {
			let factor = 2u64.saturating_pow(consecutive_failures.min(10));
			let delay_ms = config.reconnect_base_delay.as_millis() as u64 * factor;
			Duration::from_millis(delay_ms.min(config.reconnect_max_delay.as_millis() as u64))
		} else {
			config.reconnect_base_delay
		};

		reconnect_attempts.fetch_add(1, Ordering::SeqCst);
		warn!(
			delay_ms = delay.as_millis(),
			attempts = consecutive_failures,
			"Reconnecting to SSE stream"
		);

		// Wait with shutdown check
		tokio::select! {
			_ = tokio::time::sleep(delay) => {}
			_ = shutdown_rx.recv() => {
				info!("SSE connection received shutdown signal during reconnect wait");
				break;
			}
		}
	}
}

/// Connects to the SSE stream and processes events until disconnection.
async fn connect_and_process(
	stream_url: &str,
	sdk_key: &str,
	cache: &FlagCache,
	connected: &Arc<AtomicBool>,
	events_received: &Arc<AtomicU64>,
) -> Result<()> {
	let client = loom_common_http::builder()
		.build()
		.map_err(FlagsError::ConnectionFailed)?;

	let request = client
		.get(stream_url)
		.header("Authorization", format!("Bearer {}", sdk_key))
		.header("Accept", "text/event-stream")
		.header("Cache-Control", "no-cache");

	let response = request.send().await.map_err(FlagsError::ConnectionFailed)?;

	if !response.status().is_success() {
		return Err(FlagsError::ServerError {
			status: response.status().as_u16(),
			message: response.text().await.unwrap_or_default(),
		});
	}

	connected.store(true, Ordering::SeqCst);
	info!("SSE connection established");

	let stream = response.bytes_stream();
	let mut event_stream = stream.eventsource();

	while let Some(event_result) = event_stream.next().await {
		match event_result {
			Ok(event) => {
				events_received.fetch_add(1, Ordering::SeqCst);
				if let Err(e) = process_event(event, cache).await {
					warn!(error = %e, "Failed to process SSE event");
				}
			}
			Err(e) => {
				return Err(FlagsError::SseStreamError(e.to_string()));
			}
		}
	}

	Ok(())
}

/// Processes a single SSE event and updates the cache.
async fn process_event(event: Event, cache: &FlagCache) -> Result<()> {
	// Skip comment events and empty data
	if event.data.is_empty() {
		return Ok(());
	}

	let stream_event: FlagStreamEvent = serde_json::from_str(&event.data).map_err(|e| {
		warn!(data = %event.data, error = %e, "Failed to parse SSE event");
		FlagsError::ParseFailed(e.to_string())
	})?;

	debug!(event_type = %stream_event.event_type(), "Processing SSE event");

	match stream_event {
		FlagStreamEvent::Init(data) => {
			cache.initialize(data.flags, data.kill_switches).await;
			let flag_count = cache.flag_count().await;
			let ks_count = cache.kill_switch_count().await;
			info!(
				flags = flag_count,
				kill_switches = ks_count,
				"Cache initialized from SSE"
			);
		}
		FlagStreamEvent::FlagUpdated(data) => {
			// Update the flag's enabled status in cache
			cache
				.update_flag_enabled(&data.flag_key, data.enabled)
				.await;
			debug!(flag_key = %data.flag_key, enabled = data.enabled, "Flag updated");
		}
		FlagStreamEvent::FlagArchived(data) => {
			cache.archive_flag(&data.flag_key).await;
			debug!(flag_key = %data.flag_key, "Flag archived");
		}
		FlagStreamEvent::FlagRestored(data) => {
			cache.restore_flag(&data.flag_key, data.enabled).await;
			debug!(flag_key = %data.flag_key, "Flag restored");
		}
		FlagStreamEvent::KillSwitchActivated(data) => {
			cache
				.activate_kill_switch(&data.kill_switch_key, &data.reason)
				.await;
			info!(
				kill_switch = %data.kill_switch_key,
				reason = %data.reason,
				affected_flags = ?data.linked_flag_keys,
				"Kill switch activated"
			);
		}
		FlagStreamEvent::KillSwitchDeactivated(data) => {
			cache.deactivate_kill_switch(&data.kill_switch_key).await;
			info!(
				kill_switch = %data.kill_switch_key,
				affected_flags = ?data.linked_flag_keys,
				"Kill switch deactivated"
			);
		}
		FlagStreamEvent::Heartbeat(_) => {
			debug!("Heartbeat received");
		}
	}

	Ok(())
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_sse_config_defaults() {
		let config = SseConfig::default();
		assert_eq!(config.reconnect_base_delay, Duration::from_secs(1));
		assert_eq!(config.reconnect_max_delay, Duration::from_secs(30));
		assert_eq!(config.max_reconnect_attempts, 0);
		assert!(config.use_exponential_backoff);
	}

	#[test]
	fn test_sse_connection_initial_state() {
		let conn = SseConnection::new();
		assert!(!conn.is_connected());
		assert_eq!(conn.reconnect_attempts(), 0);
		assert_eq!(conn.events_received(), 0);
	}

	#[tokio::test]
	async fn test_process_init_event() {
		use loom_flags_core::{FlagId, FlagState, KillSwitchId, KillSwitchState, VariantValue};

		let cache = FlagCache::new();

		let init_data = loom_flags_core::sse::InitData {
			flags: vec![FlagState {
				key: "feature.test".to_string(),
				id: FlagId::new(),
				enabled: true,
				default_variant: "on".to_string(),
				default_value: VariantValue::Boolean(true),
				archived: false,
			}],
			kill_switches: vec![KillSwitchState {
				key: "emergency".to_string(),
				id: KillSwitchId::new(),
				is_active: false,
				linked_flag_keys: vec![],
				activation_reason: None,
			}],
			timestamp: chrono::Utc::now(),
		};

		let event = FlagStreamEvent::Init(init_data);
		let event_json = serde_json::to_string(&event).unwrap();

		let sse_event = Event {
			event: "init".to_string(),
			data: event_json,
			id: String::new(),
			retry: None,
		};

		process_event(sse_event, &cache).await.unwrap();

		assert!(cache.is_initialized().await);
		assert_eq!(cache.flag_count().await, 1);
		assert_eq!(cache.kill_switch_count().await, 1);
	}

	#[tokio::test]
	async fn test_process_flag_updated_event() {
		use loom_flags_core::{FlagId, FlagState, VariantValue};

		let cache = FlagCache::new();
		cache
			.initialize(
				vec![FlagState {
					key: "feature.test".to_string(),
					id: FlagId::new(),
					enabled: false,
					default_variant: "off".to_string(),
					default_value: VariantValue::Boolean(false),
					archived: false,
				}],
				vec![],
			)
			.await;

		let event = FlagStreamEvent::flag_updated(
			"feature.test".to_string(),
			"prod".to_string(),
			true,
			"on".to_string(),
			VariantValue::Boolean(true),
		);
		let event_json = serde_json::to_string(&event).unwrap();

		let sse_event = Event {
			event: "flag.updated".to_string(),
			data: event_json,
			id: String::new(),
			retry: None,
		};

		process_event(sse_event, &cache).await.unwrap();

		let flag = cache.get_flag("feature.test").await.unwrap();
		assert!(flag.enabled);
	}

	#[tokio::test]
	async fn test_process_kill_switch_activated_event() {
		use loom_flags_core::KillSwitchId;

		let cache = FlagCache::new();
		cache
			.initialize(
				vec![],
				vec![loom_flags_core::KillSwitchState {
					key: "emergency".to_string(),
					id: KillSwitchId::new(),
					is_active: false,
					linked_flag_keys: vec!["feature.test".to_string()],
					activation_reason: None,
				}],
			)
			.await;

		let event = FlagStreamEvent::kill_switch_activated(
			"emergency".to_string(),
			vec!["feature.test".to_string()],
			"System outage".to_string(),
		);
		let event_json = serde_json::to_string(&event).unwrap();

		let sse_event = Event {
			event: "killswitch.activated".to_string(),
			data: event_json,
			id: String::new(),
			retry: None,
		};

		process_event(sse_event, &cache).await.unwrap();

		let ks = cache.get_kill_switch("emergency").await.unwrap();
		assert!(ks.is_active);
		assert_eq!(ks.activation_reason, Some("System outage".to_string()));
	}
}
