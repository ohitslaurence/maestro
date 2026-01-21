// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use serde::Serialize;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};
use tracing::instrument;
use uuid::Uuid;

const CHANNEL_CAPACITY: usize = 64;

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type")]
pub enum PeerEvent {
	#[serde(rename = "peer_added")]
	PeerAdded {
		public_key: String,
		allowed_ip: String,
		session_id: String,
	},
	#[serde(rename = "peer_removed")]
	PeerRemoved {
		public_key: String,
		session_id: String,
	},
}

#[derive(Clone)]
pub struct PeerNotifier {
	senders: Arc<RwLock<HashMap<Uuid, broadcast::Sender<PeerEvent>>>>,
}

impl Default for PeerNotifier {
	fn default() -> Self {
		Self::new()
	}
}

impl PeerNotifier {
	pub fn new() -> Self {
		Self {
			senders: Arc::new(RwLock::new(HashMap::new())),
		}
	}

	#[instrument(skip(self), fields(%weaver_id))]
	pub async fn subscribe(&self, weaver_id: Uuid) -> broadcast::Receiver<PeerEvent> {
		let mut senders = self.senders.write().await;

		if let Some(sender) = senders.get(&weaver_id) {
			return sender.subscribe();
		}

		let (tx, rx) = broadcast::channel(CHANNEL_CAPACITY);
		senders.insert(weaver_id, tx);
		rx
	}

	#[instrument(skip(self, event), fields(%weaver_id))]
	pub async fn notify_peer_added(&self, weaver_id: Uuid, event: PeerEvent) {
		let senders = self.senders.read().await;

		if let Some(sender) = senders.get(&weaver_id) {
			let _ = sender.send(event);
		}
	}

	#[instrument(skip(self, event), fields(%weaver_id))]
	pub async fn notify_peer_removed(&self, weaver_id: Uuid, event: PeerEvent) {
		let senders = self.senders.read().await;

		if let Some(sender) = senders.get(&weaver_id) {
			let _ = sender.send(event);
		}
	}

	#[instrument(skip(self), fields(%weaver_id))]
	pub async fn unregister(&self, weaver_id: Uuid) {
		let mut senders = self.senders.write().await;
		senders.remove(&weaver_id);
	}
}
