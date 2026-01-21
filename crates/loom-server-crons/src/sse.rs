// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! SSE (Server-Sent Events) streaming infrastructure for cron monitoring.
//!
//! This module provides the broadcast mechanism for real-time cron monitoring
//! updates to connected clients.
//!
//! # Architecture
//!
//! The SSE system uses a per-organization broadcast channel architecture:
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────────┐
//! │                         CronsBroadcaster                            │
//! │  ┌───────────────────────────────────────────────────────────────┐  │
//! │  │            channels: HashMap<OrgId, broadcast::Sender>        │  │
//! │  └───────────────────────────────────────────────────────────────┘  │
//! │                                │                                     │
//! │  Check-in Update ──────────────┼──────────────> Broadcast to org    │
//! │                                │                                     │
//! └────────────────────────────────┼─────────────────────────────────────┘
//!                                  │
//!                                  ▼
//!    ┌─────────────────────────────────────────────────────────────────┐
//!    │                    Per-Organization Channels                     │
//!    │  ┌───────────────┐  ┌───────────────┐  ┌───────────────┐        │
//!    │  │ org1          │  │ org2          │  │ org3          │  ...   │
//!    │  │ Sender+Recvrs │  │ Sender+Recvrs │  │ Sender+Recvrs │        │
//!    │  └───────────────┘  └───────────────┘  └───────────────┘        │
//!    └─────────────────────────────────────────────────────────────────┘
//! ```

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::{broadcast, RwLock};
use tracing::{debug, info, warn};

use loom_crons_core::{CronStreamEvent, OrgId};

/// Default channel capacity per organization.
const DEFAULT_CHANNEL_CAPACITY: usize = 256;

/// Default heartbeat interval in seconds.
const DEFAULT_HEARTBEAT_INTERVAL_SECS: u64 = 30;

/// Default maximum connections per organization.
const DEFAULT_MAX_CONNECTIONS_PER_ORG: usize = 10_000;

/// Configuration for the crons broadcaster.
#[derive(Debug, Clone)]
pub struct CronsBroadcasterConfig {
	/// Capacity of each broadcast channel.
	pub channel_capacity: usize,
	/// Heartbeat interval for keep-alive.
	pub heartbeat_interval: Duration,
	/// Maximum connections per organization.
	pub max_connections_per_org: usize,
}

impl Default for CronsBroadcasterConfig {
	fn default() -> Self {
		Self {
			channel_capacity: DEFAULT_CHANNEL_CAPACITY,
			heartbeat_interval: Duration::from_secs(DEFAULT_HEARTBEAT_INTERVAL_SECS),
			max_connections_per_org: DEFAULT_MAX_CONNECTIONS_PER_ORG,
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
	sender: broadcast::Sender<CronStreamEvent>,
	stats: ChannelStats,
}

/// Broadcasts cron monitoring updates to connected SSE clients.
///
/// This is the central hub for real-time cron streaming. It manages
/// per-organization broadcast channels and tracks connection statistics.
pub struct CronsBroadcaster {
	config: CronsBroadcasterConfig,
	channels: RwLock<HashMap<OrgId, ChannelState>>,
	total_events: AtomicU64,
	total_connections: AtomicU64,
}

impl CronsBroadcaster {
	/// Create a new broadcaster with the given configuration.
	pub fn new(config: CronsBroadcasterConfig) -> Self {
		Self {
			config,
			channels: RwLock::new(HashMap::new()),
			total_events: AtomicU64::new(0),
			total_connections: AtomicU64::new(0),
		}
	}

	/// Create a new broadcaster with default configuration.
	pub fn with_defaults() -> Self {
		Self::new(CronsBroadcasterConfig::default())
	}

	/// Subscribe to cron updates for a specific organization.
	///
	/// Returns a receiver that will receive all cron events for the
	/// specified organization.
	pub async fn subscribe(&self, org_id: OrgId) -> broadcast::Receiver<CronStreamEvent> {
		// First, try to get an existing channel with a read lock
		{
			let channels = self.channels.read().await;
			if let Some(state) = channels.get(&org_id) {
				self.total_connections.fetch_add(1, Ordering::Relaxed);
				debug!(
					org_id = %org_id,
					receiver_count = state.sender.receiver_count(),
					"Client subscribed to existing crons channel"
				);
				return state.sender.subscribe();
			}
		}

		// Channel doesn't exist, create it with a write lock
		let mut channels = self.channels.write().await;

		// Double-check in case another task created it while we were waiting
		if let Some(state) = channels.get(&org_id) {
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

		channels.insert(org_id, state);
		self.total_connections.fetch_add(1, Ordering::Relaxed);

		info!(
			org_id = %org_id,
			"Created new broadcast channel for crons"
		);

		// Get receiver from the newly created channel
		channels.get(&org_id).unwrap().sender.subscribe()
	}

	/// Broadcast an event to all subscribers of a specific organization.
	///
	/// Returns the number of clients that received the event.
	pub async fn broadcast(&self, org_id: OrgId, event: CronStreamEvent) -> usize {
		let channels = self.channels.read().await;

		if let Some(state) = channels.get(&org_id) {
			let receiver_count = state.sender.receiver_count();
			if receiver_count == 0 {
				debug!(
					org_id = %org_id,
					event_type = event.event_type(),
					"No receivers for crons broadcast"
				);
				return 0;
			}

			match state.sender.send(event.clone()) {
				Ok(count) => {
					self.total_events.fetch_add(1, Ordering::Relaxed);
					debug!(
						org_id = %org_id,
						event_type = event.event_type(),
						receiver_count = count,
						"Broadcast cron event to receivers"
					);
					count
				}
				Err(e) => {
					warn!(
						org_id = %org_id,
						error = %e,
						"Failed to broadcast cron event"
					);
					0
				}
			}
		} else {
			debug!(
				org_id = %org_id,
				event_type = event.event_type(),
				"No channel exists for org"
			);
			0
		}
	}

	/// Broadcast a heartbeat to all connected clients.
	pub async fn broadcast_heartbeat(&self) {
		let event = CronStreamEvent::heartbeat();
		let channels = self.channels.read().await;

		for (_key, state) in channels.iter() {
			let _ = state.sender.send(event.clone());
		}

		debug!(
			channel_count = channels.len(),
			"Broadcast heartbeat to all crons channels"
		);
	}

	/// Get statistics for a specific channel.
	pub async fn channel_stats(&self, org_id: OrgId) -> Option<ChannelStats> {
		let channels = self.channels.read().await;

		channels.get(&org_id).map(|state| ChannelStats {
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
					org_id = %key,
					"Removing empty crons broadcast channel"
				);
			}
			keep
		});

		let removed = initial_count - channels.len();
		if removed > 0 {
			info!(
				removed_channels = removed,
				"Cleaned up empty crons broadcast channels"
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

impl CronsBroadcaster {
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
		let broadcaster = CronsBroadcaster::with_defaults();
		let org_id = OrgId::new();

		assert_eq!(broadcaster.channel_count().await, 0);

		let _receiver = broadcaster.subscribe(org_id).await;

		assert_eq!(broadcaster.channel_count().await, 1);
		assert_eq!(broadcaster.total_receiver_count().await, 1);
	}

	#[tokio::test]
	async fn test_multiple_subscribers_same_channel() {
		let broadcaster = CronsBroadcaster::with_defaults();
		let org_id = OrgId::new();

		let _r1 = broadcaster.subscribe(org_id).await;
		let _r2 = broadcaster.subscribe(org_id).await;
		let _r3 = broadcaster.subscribe(org_id).await;

		assert_eq!(broadcaster.channel_count().await, 1);
		assert_eq!(broadcaster.total_receiver_count().await, 3);
	}

	#[tokio::test]
	async fn test_different_orgs_different_channels() {
		let broadcaster = CronsBroadcaster::with_defaults();
		let org1 = OrgId::new();
		let org2 = OrgId::new();

		let _r1 = broadcaster.subscribe(org1).await;
		let _r2 = broadcaster.subscribe(org2).await;

		assert_eq!(broadcaster.channel_count().await, 2);
	}

	#[tokio::test]
	async fn test_broadcast_to_subscribers() {
		use loom_crons_core::{CheckInId, MonitorId};

		let broadcaster = CronsBroadcaster::with_defaults();
		let org_id = OrgId::new();

		let mut receiver = broadcaster.subscribe(org_id).await;

		let event = CronStreamEvent::checkin_ok(
			MonitorId::new(),
			"test".to_string(),
			CheckInId::new(),
			Some(5000),
		);

		let count = broadcaster.broadcast(org_id, event.clone()).await;
		assert_eq!(count, 1);

		let received = timeout(Duration::from_millis(100), receiver.recv()).await;
		assert!(received.is_ok());
		let received_event = received.unwrap().unwrap();
		assert_eq!(received_event.event_type(), "checkin.ok");
	}

	#[tokio::test]
	async fn test_broadcast_to_nonexistent_channel() {
		let broadcaster = CronsBroadcaster::with_defaults();
		let org_id = OrgId::new();

		let event = CronStreamEvent::heartbeat();
		let count = broadcaster.broadcast(org_id, event).await;

		assert_eq!(count, 0);
	}

	#[tokio::test]
	async fn test_heartbeat_broadcast() {
		let broadcaster = CronsBroadcaster::with_defaults();
		let org_id = OrgId::new();

		let mut receiver = broadcaster.subscribe(org_id).await;

		broadcaster.broadcast_heartbeat().await;

		let received = timeout(Duration::from_millis(100), receiver.recv()).await;
		assert!(received.is_ok());
		assert_eq!(received.unwrap().unwrap().event_type(), "heartbeat");
	}

	#[tokio::test]
	async fn test_cleanup_empty_channels() {
		let broadcaster = CronsBroadcaster::with_defaults();
		let org_id = OrgId::new();

		{
			let _receiver = broadcaster.subscribe(org_id).await;
			assert_eq!(broadcaster.channel_count().await, 1);
		}
		// Receiver dropped here

		let removed = broadcaster.cleanup_empty_channels().await;
		assert_eq!(removed, 1);
		assert_eq!(broadcaster.channel_count().await, 0);
	}

	#[tokio::test]
	async fn test_stats() {
		use loom_crons_core::{CheckInId, MonitorId};

		let broadcaster = CronsBroadcaster::with_defaults();
		let org_id = OrgId::new();

		let _receiver = broadcaster.subscribe(org_id).await;

		let event =
			CronStreamEvent::checkin_ok(MonitorId::new(), "test".to_string(), CheckInId::new(), None);
		broadcaster.broadcast(org_id, event).await;

		let stats = broadcaster.stats().await;
		assert_eq!(stats.channel_count, 1);
		assert_eq!(stats.total_receivers, 1);
		assert_eq!(stats.total_events_sent, 1);
		assert!(stats.total_connections >= 1);
	}
}
