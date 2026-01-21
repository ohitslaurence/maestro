// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! SSE (Server-Sent Events) streaming infrastructure for feature flags.
//!
//! This module provides the broadcast mechanism for real-time flag updates
//! to connected SDK clients.
//!
//! # Architecture
//!
//! The SSE system uses a per-environment broadcast channel architecture:
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────────┐
//! │                         FlagsBroadcaster                            │
//! │  ┌───────────────────────────────────────────────────────────────┐  │
//! │  │  channels: HashMap<(OrgId, EnvironmentId), broadcast::Sender> │  │
//! │  └───────────────────────────────────────────────────────────────┘  │
//! │                                │                                     │
//! │  Flag Updated ─────────────────┼──────────────> Broadcast to env    │
//! │                                │                                     │
//! └────────────────────────────────┼─────────────────────────────────────┘
//!                                  │
//!                                  ▼
//!    ┌─────────────────────────────────────────────────────────────────┐
//!    │                    Per-Environment Channels                      │
//!    │  ┌───────────────┐  ┌───────────────┐  ┌───────────────┐        │
//!    │  │ org1:prod     │  │ org1:staging  │  │ org2:prod     │  ...   │
//!    │  │ Sender+Recvrs │  │ Sender+Recvrs │  │ Sender+Recvrs │        │
//!    │  └───────────────┘  └───────────────┘  └───────────────┘        │
//!    └─────────────────────────────────────────────────────────────────┘
//! ```
//!
//! # Example
//!
//! ```ignore
//! use loom_server_flags::sse::{FlagsBroadcaster, BroadcasterConfig};
//! use loom_flags_core::{FlagStreamEvent, OrgId, EnvironmentId};
//!
//! // Create broadcaster
//! let config = BroadcasterConfig::default();
//! let broadcaster = FlagsBroadcaster::new(config);
//!
//! // Subscribe to updates for an environment
//! let receiver = broadcaster.subscribe(org_id, env_id);
//!
//! // Broadcast an event
//! let event = FlagStreamEvent::flag_updated("feature.new_flow", "prod", true);
//! broadcaster.broadcast(org_id, env_id, event);
//! ```

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::{broadcast, RwLock};
use tracing::{debug, info, warn};

use loom_flags_core::{EnvironmentId, FlagStreamEvent, OrgId};

/// Default channel capacity per environment.
const DEFAULT_CHANNEL_CAPACITY: usize = 256;

/// Default heartbeat interval in seconds.
const DEFAULT_HEARTBEAT_INTERVAL_SECS: u64 = 30;

/// Default maximum connections per environment.
const DEFAULT_MAX_CONNECTIONS_PER_ENV: usize = 10_000;

/// Configuration for the flags broadcaster.
#[derive(Debug, Clone)]
pub struct BroadcasterConfig {
	/// Capacity of each broadcast channel.
	pub channel_capacity: usize,
	/// Heartbeat interval for keep-alive.
	pub heartbeat_interval: Duration,
	/// Maximum connections per environment.
	pub max_connections_per_env: usize,
}

impl Default for BroadcasterConfig {
	fn default() -> Self {
		Self {
			channel_capacity: DEFAULT_CHANNEL_CAPACITY,
			heartbeat_interval: Duration::from_secs(DEFAULT_HEARTBEAT_INTERVAL_SECS),
			max_connections_per_env: DEFAULT_MAX_CONNECTIONS_PER_ENV,
		}
	}
}

/// Key for identifying an environment channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ChannelKey {
	pub org_id: OrgId,
	pub environment_id: EnvironmentId,
}

impl ChannelKey {
	pub fn new(org_id: OrgId, environment_id: EnvironmentId) -> Self {
		Self {
			org_id,
			environment_id,
		}
	}
}

/// Statistics for a broadcast channel.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelStats {
	/// Number of active receivers.
	pub receiver_count: usize,
	/// Total events sent on this channel.
	pub events_sent: u64,
	/// When the channel was created.
	pub created_at: DateTime<Utc>,
	/// Last event timestamp.
	pub last_event_at: Option<DateTime<Utc>>,
}

/// Internal channel state.
struct ChannelState {
	sender: broadcast::Sender<FlagStreamEvent>,
	stats: ChannelStats,
}

/// Broadcasts flag updates to connected SSE clients.
///
/// This is the central hub for real-time flag streaming. It manages
/// per-environment broadcast channels and tracks connection statistics.
pub struct FlagsBroadcaster {
	config: BroadcasterConfig,
	channels: RwLock<HashMap<ChannelKey, ChannelState>>,
	total_events: AtomicU64,
	total_connections: AtomicU64,
}

impl FlagsBroadcaster {
	/// Create a new broadcaster with the given configuration.
	pub fn new(config: BroadcasterConfig) -> Self {
		Self {
			config,
			channels: RwLock::new(HashMap::new()),
			total_events: AtomicU64::new(0),
			total_connections: AtomicU64::new(0),
		}
	}

	/// Create a new broadcaster with default configuration.
	pub fn with_defaults() -> Self {
		Self::new(BroadcasterConfig::default())
	}

	/// Subscribe to flag updates for a specific environment.
	///
	/// Returns a receiver that will receive all flag events for the
	/// specified organization and environment.
	pub async fn subscribe(
		&self,
		org_id: OrgId,
		environment_id: EnvironmentId,
	) -> broadcast::Receiver<FlagStreamEvent> {
		let key = ChannelKey::new(org_id, environment_id);

		// First, try to get an existing channel with a read lock
		{
			let channels = self.channels.read().await;
			if let Some(state) = channels.get(&key) {
				self.total_connections.fetch_add(1, Ordering::Relaxed);
				debug!(
					org_id = %org_id,
					environment_id = %environment_id,
					receiver_count = state.sender.receiver_count(),
					"Client subscribed to existing channel"
				);
				return state.sender.subscribe();
			}
		}

		// Channel doesn't exist, create it with a write lock
		let mut channels = self.channels.write().await;

		// Double-check in case another task created it while we were waiting
		if let Some(state) = channels.get(&key) {
			self.total_connections.fetch_add(1, Ordering::Relaxed);
			return state.sender.subscribe();
		}

		// Create new channel
		let (sender, _receiver) = broadcast::channel(self.config.channel_capacity);
		let state = ChannelState {
			sender,
			stats: ChannelStats {
				receiver_count: 1,
				events_sent: 0,
				created_at: Utc::now(),
				last_event_at: None,
			},
		};

		channels.insert(key, state);
		self.total_connections.fetch_add(1, Ordering::Relaxed);

		info!(
			org_id = %org_id,
			environment_id = %environment_id,
			"Created new broadcast channel for environment"
		);

		// Get receiver from the newly created channel
		channels.get(&key).unwrap().sender.subscribe()
	}

	/// Broadcast an event to all subscribers of a specific environment.
	///
	/// Returns the number of clients that received the event.
	pub async fn broadcast(
		&self,
		org_id: OrgId,
		environment_id: EnvironmentId,
		event: FlagStreamEvent,
	) -> usize {
		let key = ChannelKey::new(org_id, environment_id);
		let channels = self.channels.read().await;

		if let Some(state) = channels.get(&key) {
			let receiver_count = state.sender.receiver_count();
			if receiver_count == 0 {
				debug!(
					org_id = %org_id,
					environment_id = %environment_id,
					event_type = event.event_type(),
					"No receivers for broadcast"
				);
				return 0;
			}

			match state.sender.send(event.clone()) {
				Ok(count) => {
					self.total_events.fetch_add(1, Ordering::Relaxed);
					debug!(
						org_id = %org_id,
						environment_id = %environment_id,
						event_type = event.event_type(),
						receiver_count = count,
						"Broadcast event to receivers"
					);
					count
				}
				Err(e) => {
					warn!(
						org_id = %org_id,
						environment_id = %environment_id,
						error = %e,
						"Failed to broadcast event"
					);
					0
				}
			}
		} else {
			debug!(
				org_id = %org_id,
				environment_id = %environment_id,
				event_type = event.event_type(),
				"No channel exists for environment"
			);
			0
		}
	}

	/// Broadcast an event to all environments in an organization.
	///
	/// Useful for org-wide events like kill switch activations.
	pub async fn broadcast_to_org(&self, org_id: OrgId, event: FlagStreamEvent) -> usize {
		let channels = self.channels.read().await;
		let mut total = 0;

		for (key, state) in channels.iter() {
			if key.org_id == org_id {
				if let Ok(count) = state.sender.send(event.clone()) {
					total += count;
				}
			}
		}

		if total > 0 {
			self.total_events.fetch_add(1, Ordering::Relaxed);
			debug!(
				org_id = %org_id,
				event_type = event.event_type(),
				total_receivers = total,
				"Broadcast event to all org environments"
			);
		}

		total
	}

	/// Broadcast an event to all connected clients (platform-level events).
	///
	/// This is used for platform-level kill switches and other events that
	/// affect all organizations.
	pub async fn broadcast_to_all(&self, event: FlagStreamEvent) -> usize {
		let channels = self.channels.read().await;
		let mut total = 0;

		for (_key, state) in channels.iter() {
			if let Ok(count) = state.sender.send(event.clone()) {
				total += count;
			}
		}

		if total > 0 {
			self.total_events.fetch_add(1, Ordering::Relaxed);
			info!(
				event_type = event.event_type(),
				total_receivers = total,
				channel_count = channels.len(),
				"Broadcast platform event to all channels"
			);
		}

		total
	}

	/// Broadcast a heartbeat to all connected clients.
	pub async fn broadcast_heartbeat(&self) {
		let event = FlagStreamEvent::heartbeat();
		let channels = self.channels.read().await;

		for (_key, state) in channels.iter() {
			let _ = state.sender.send(event.clone());
		}

		debug!(
			channel_count = channels.len(),
			"Broadcast heartbeat to all channels"
		);
	}

	/// Get statistics for a specific channel.
	pub async fn channel_stats(
		&self,
		org_id: OrgId,
		environment_id: EnvironmentId,
	) -> Option<ChannelStats> {
		let key = ChannelKey::new(org_id, environment_id);
		let channels = self.channels.read().await;

		channels.get(&key).map(|state| ChannelStats {
			receiver_count: state.sender.receiver_count(),
			events_sent: state.stats.events_sent,
			created_at: state.stats.created_at,
			last_event_at: state.stats.last_event_at,
		})
	}

	/// Get total number of active channels.
	pub async fn channel_count(&self) -> usize {
		self.channels.read().await.len()
	}

	/// Get total number of connected receivers across all channels.
	pub async fn total_receiver_count(&self) -> usize {
		let channels = self.channels.read().await;
		channels.values().map(|s| s.sender.receiver_count()).sum()
	}

	/// Get total events sent across all channels.
	pub fn total_events_sent(&self) -> u64 {
		self.total_events.load(Ordering::Relaxed)
	}

	/// Get total connections ever made.
	pub fn total_connections(&self) -> u64 {
		self.total_connections.load(Ordering::Relaxed)
	}

	/// Get the heartbeat interval.
	pub fn heartbeat_interval(&self) -> Duration {
		self.config.heartbeat_interval
	}

	/// Clean up channels with no active receivers.
	pub async fn cleanup_empty_channels(&self) -> usize {
		let mut channels = self.channels.write().await;
		let initial_count = channels.len();

		channels.retain(|key, state| {
			let keep = state.sender.receiver_count() > 0;
			if !keep {
				debug!(
					org_id = %key.org_id,
					environment_id = %key.environment_id,
					"Removing empty broadcast channel"
				);
			}
			keep
		});

		let removed = initial_count - channels.len();
		if removed > 0 {
			info!(
				removed_channels = removed,
				"Cleaned up empty broadcast channels"
			);
		}
		removed
	}
}

/// Global broadcaster stats for monitoring.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BroadcasterStats {
	/// Total number of active channels.
	pub channel_count: usize,
	/// Total number of connected receivers.
	pub total_receivers: usize,
	/// Total events sent since start.
	pub total_events_sent: u64,
	/// Total connections ever made.
	pub total_connections: u64,
}

impl FlagsBroadcaster {
	/// Get global broadcaster statistics.
	pub async fn stats(&self) -> BroadcasterStats {
		BroadcasterStats {
			channel_count: self.channel_count().await,
			total_receivers: self.total_receiver_count().await,
			total_events_sent: self.total_events_sent(),
			total_connections: self.total_connections(),
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use tokio::time::timeout;

	#[tokio::test]
	async fn test_subscribe_creates_channel() {
		let broadcaster = FlagsBroadcaster::with_defaults();
		let org_id = OrgId::new();
		let env_id = EnvironmentId::new();

		assert_eq!(broadcaster.channel_count().await, 0);

		let _receiver = broadcaster.subscribe(org_id, env_id).await;

		assert_eq!(broadcaster.channel_count().await, 1);
		assert_eq!(broadcaster.total_receiver_count().await, 1);
	}

	#[tokio::test]
	async fn test_multiple_subscribers_same_channel() {
		let broadcaster = FlagsBroadcaster::with_defaults();
		let org_id = OrgId::new();
		let env_id = EnvironmentId::new();

		let _r1 = broadcaster.subscribe(org_id, env_id).await;
		let _r2 = broadcaster.subscribe(org_id, env_id).await;
		let _r3 = broadcaster.subscribe(org_id, env_id).await;

		assert_eq!(broadcaster.channel_count().await, 1);
		assert_eq!(broadcaster.total_receiver_count().await, 3);
	}

	#[tokio::test]
	async fn test_different_environments_different_channels() {
		let broadcaster = FlagsBroadcaster::with_defaults();
		let org_id = OrgId::new();
		let env1 = EnvironmentId::new();
		let env2 = EnvironmentId::new();

		let _r1 = broadcaster.subscribe(org_id, env1).await;
		let _r2 = broadcaster.subscribe(org_id, env2).await;

		assert_eq!(broadcaster.channel_count().await, 2);
	}

	#[tokio::test]
	async fn test_broadcast_to_subscribers() {
		let broadcaster = FlagsBroadcaster::with_defaults();
		let org_id = OrgId::new();
		let env_id = EnvironmentId::new();

		let mut receiver = broadcaster.subscribe(org_id, env_id).await;

		let event = FlagStreamEvent::flag_updated(
			"test.flag".to_string(),
			"prod".to_string(),
			true,
			"on".to_string(),
			loom_flags_core::VariantValue::Boolean(true),
		);

		let count = broadcaster.broadcast(org_id, env_id, event.clone()).await;
		assert_eq!(count, 1);

		let received = timeout(Duration::from_millis(100), receiver.recv()).await;
		assert!(received.is_ok());
		let received_event = received.unwrap().unwrap();
		assert_eq!(received_event.event_type(), "flag.updated");
	}

	#[tokio::test]
	async fn test_broadcast_to_nonexistent_channel() {
		let broadcaster = FlagsBroadcaster::with_defaults();
		let org_id = OrgId::new();
		let env_id = EnvironmentId::new();

		let event = FlagStreamEvent::heartbeat();
		let count = broadcaster.broadcast(org_id, env_id, event).await;

		assert_eq!(count, 0);
	}

	#[tokio::test]
	async fn test_broadcast_to_org() {
		let broadcaster = FlagsBroadcaster::with_defaults();
		let org_id = OrgId::new();
		let env1 = EnvironmentId::new();
		let env2 = EnvironmentId::new();

		let mut r1 = broadcaster.subscribe(org_id, env1).await;
		let mut r2 = broadcaster.subscribe(org_id, env2).await;

		let event = FlagStreamEvent::kill_switch_activated(
			"emergency".to_string(),
			vec!["flag1".to_string()],
			"outage".to_string(),
		);

		let count = broadcaster.broadcast_to_org(org_id, event).await;
		assert_eq!(count, 2);

		let recv1 = timeout(Duration::from_millis(100), r1.recv()).await;
		let recv2 = timeout(Duration::from_millis(100), r2.recv()).await;

		assert!(recv1.is_ok());
		assert!(recv2.is_ok());
	}

	#[tokio::test]
	async fn test_heartbeat_broadcast() {
		let broadcaster = FlagsBroadcaster::with_defaults();
		let org_id = OrgId::new();
		let env_id = EnvironmentId::new();

		let mut receiver = broadcaster.subscribe(org_id, env_id).await;

		broadcaster.broadcast_heartbeat().await;

		let received = timeout(Duration::from_millis(100), receiver.recv()).await;
		assert!(received.is_ok());
		assert_eq!(received.unwrap().unwrap().event_type(), "heartbeat");
	}

	#[tokio::test]
	async fn test_cleanup_empty_channels() {
		let broadcaster = FlagsBroadcaster::with_defaults();
		let org_id = OrgId::new();
		let env_id = EnvironmentId::new();

		{
			let _receiver = broadcaster.subscribe(org_id, env_id).await;
			assert_eq!(broadcaster.channel_count().await, 1);
		}
		// Receiver dropped here

		let removed = broadcaster.cleanup_empty_channels().await;
		assert_eq!(removed, 1);
		assert_eq!(broadcaster.channel_count().await, 0);
	}

	#[tokio::test]
	async fn test_stats() {
		let broadcaster = FlagsBroadcaster::with_defaults();
		let org_id = OrgId::new();
		let env_id = EnvironmentId::new();

		let _receiver = broadcaster.subscribe(org_id, env_id).await;

		let event = FlagStreamEvent::heartbeat();
		broadcaster.broadcast(org_id, env_id, event).await;

		let stats = broadcaster.stats().await;
		assert_eq!(stats.channel_count, 1);
		assert_eq!(stats.total_receivers, 1);
		assert_eq!(stats.total_events_sent, 1);
		assert!(stats.total_connections >= 1);
	}

	#[tokio::test]
	async fn test_channel_stats() {
		let broadcaster = FlagsBroadcaster::with_defaults();
		let org_id = OrgId::new();
		let env_id = EnvironmentId::new();

		// No channel yet
		assert!(broadcaster.channel_stats(org_id, env_id).await.is_none());

		let _receiver = broadcaster.subscribe(org_id, env_id).await;

		let stats = broadcaster.channel_stats(org_id, env_id).await;
		assert!(stats.is_some());
		let stats = stats.unwrap();
		assert_eq!(stats.receiver_count, 1);
		assert_eq!(stats.events_sent, 0);
	}
}
