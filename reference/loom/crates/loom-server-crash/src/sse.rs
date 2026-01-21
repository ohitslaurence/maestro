// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! SSE broadcasting for crash analytics real-time updates.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};
use tracing::{debug, info};

use loom_crash_core::{CrashEventId, Issue, IssueId, ProjectId};

/// Configuration for the crash broadcaster.
#[derive(Debug, Clone)]
pub struct CrashBroadcasterConfig {
	/// Maximum channel capacity per organization
	pub channel_capacity: usize,
}

impl Default for CrashBroadcasterConfig {
	fn default() -> Self {
		Self {
			channel_capacity: 256,
		}
	}
}

/// Events that can be broadcast via SSE.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum CrashStreamEvent {
	/// Initial connection event with current state
	Init {
		project_id: ProjectId,
		issue_count: u64,
	},
	/// New crash event received
	CrashNew {
		event_id: CrashEventId,
		issue_id: IssueId,
		short_id: String,
		title: String,
		is_new_issue: bool,
	},
	/// Issue state changed to regressed
	IssueRegressed {
		issue_id: IssueId,
		short_id: String,
		title: String,
		times_regressed: u32,
		regressed_in_release: Option<String>,
	},
	/// Issue was resolved
	IssueResolved { issue_id: IssueId, short_id: String },
	/// Issue was assigned
	IssueAssigned {
		issue_id: IssueId,
		short_id: String,
		assigned_to: Option<String>,
	},
	/// Heartbeat to keep connection alive
	Heartbeat { timestamp: String },
}

impl CrashStreamEvent {
	/// Returns the SSE event type name.
	pub fn event_type(&self) -> &'static str {
		match self {
			CrashStreamEvent::Init { .. } => "init",
			CrashStreamEvent::CrashNew { .. } => "crash.new",
			CrashStreamEvent::IssueRegressed { .. } => "issue.regressed",
			CrashStreamEvent::IssueResolved { .. } => "issue.resolved",
			CrashStreamEvent::IssueAssigned { .. } => "issue.assigned",
			CrashStreamEvent::Heartbeat { .. } => "heartbeat",
		}
	}

	/// Create an init event for SSE connection.
	pub fn init(project_id: ProjectId, issue_count: u64) -> Self {
		CrashStreamEvent::Init {
			project_id,
			issue_count,
		}
	}
}

/// Broadcaster for crash analytics events.
#[derive(Clone)]
pub struct CrashBroadcaster {
	config: CrashBroadcasterConfig,
	/// Per-project channels
	channels: Arc<RwLock<HashMap<ProjectId, broadcast::Sender<CrashStreamEvent>>>>,
}

impl CrashBroadcaster {
	pub fn new(config: CrashBroadcasterConfig) -> Self {
		Self {
			config,
			channels: Arc::new(RwLock::new(HashMap::new())),
		}
	}

	/// Get or create a channel for a project.
	pub async fn subscribe(&self, project_id: ProjectId) -> broadcast::Receiver<CrashStreamEvent> {
		let mut channels = self.channels.write().await;

		if let Some(sender) = channels.get(&project_id) {
			debug!(project_id = %project_id, "Subscribing to existing channel");
			sender.subscribe()
		} else {
			info!(project_id = %project_id, "Creating new channel");
			let (sender, receiver) = broadcast::channel(self.config.channel_capacity);
			channels.insert(project_id, sender);
			receiver
		}
	}

	/// Broadcast an event to all subscribers of a project.
	pub async fn broadcast(&self, project_id: ProjectId, event: CrashStreamEvent) {
		let channels = self.channels.read().await;

		if let Some(sender) = channels.get(&project_id) {
			match sender.send(event) {
				Ok(count) => {
					debug!(project_id = %project_id, subscribers = count, "Broadcast event");
				}
				Err(_) => {
					// No receivers, that's ok
					debug!(project_id = %project_id, "No subscribers for broadcast");
				}
			}
		}
	}

	/// Broadcast a new crash event.
	pub async fn broadcast_new_crash(
		&self,
		project_id: ProjectId,
		event_id: CrashEventId,
		issue: &Issue,
		is_new_issue: bool,
	) {
		self
			.broadcast(
				project_id,
				CrashStreamEvent::CrashNew {
					event_id,
					issue_id: issue.id,
					short_id: issue.short_id.clone(),
					title: issue.title.clone(),
					is_new_issue,
				},
			)
			.await;
	}

	/// Broadcast an issue regression.
	pub async fn broadcast_regression(&self, project_id: ProjectId, issue: &Issue) {
		self
			.broadcast(
				project_id,
				CrashStreamEvent::IssueRegressed {
					issue_id: issue.id,
					short_id: issue.short_id.clone(),
					title: issue.title.clone(),
					times_regressed: issue.times_regressed,
					regressed_in_release: issue.regressed_in_release.clone(),
				},
			)
			.await;
	}

	/// Broadcast an issue resolution.
	pub async fn broadcast_resolved(&self, project_id: ProjectId, issue: &Issue) {
		self
			.broadcast(
				project_id,
				CrashStreamEvent::IssueResolved {
					issue_id: issue.id,
					short_id: issue.short_id.clone(),
				},
			)
			.await;
	}

	/// Get statistics about active channels.
	pub async fn stats(&self) -> BroadcasterStats {
		let channels = self.channels.read().await;
		let total_channels = channels.len();
		let total_subscribers: usize = channels.values().map(|s| s.receiver_count()).sum();

		BroadcasterStats {
			total_channels,
			total_subscribers,
		}
	}
}

/// Statistics about the broadcaster state.
#[derive(Debug, Clone, Serialize)]
pub struct BroadcasterStats {
	pub total_channels: usize,
	pub total_subscribers: usize,
}
