// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights
// reserved. SPDX-License-Identifier: Proprietary

use std::path::PathBuf;

use chrono::Utc;
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};

use crate::error::ThreadStoreError;
use crate::model::ThreadId;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PendingSyncEntry {
	pub thread_id: ThreadId,
	pub operation: SyncOperation,
	pub failed_at: String,
	pub retry_count: u32,
	pub last_error: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SyncOperation {
	Upsert,
	Delete,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct PendingSyncQueue {
	pub entries: Vec<PendingSyncEntry>,
}

impl PendingSyncQueue {
	pub fn new() -> Self {
		Self::default()
	}

	pub fn add(&mut self, thread_id: ThreadId, operation: SyncOperation, error: Option<String>) {
		if let Some(existing) = self
			.entries
			.iter_mut()
			.find(|e| e.thread_id == thread_id && e.operation == operation)
		{
			existing.retry_count += 1;
			existing.failed_at = Utc::now().to_rfc3339();
			existing.last_error = error;
		} else {
			self.entries.push(PendingSyncEntry {
				thread_id,
				operation,
				failed_at: Utc::now().to_rfc3339(),
				retry_count: 0,
				last_error: error,
			});
		}
	}

	pub fn remove(&mut self, thread_id: &ThreadId, operation: &SyncOperation) {
		self
			.entries
			.retain(|e| !(&e.thread_id == thread_id && &e.operation == operation));
	}

	pub fn is_empty(&self) -> bool {
		self.entries.is_empty()
	}

	pub fn len(&self) -> usize {
		self.entries.len()
	}
}

pub struct PendingSyncStore {
	path: PathBuf,
}

impl PendingSyncStore {
	pub fn new(state_dir: PathBuf) -> Self {
		Self {
			path: state_dir.join("sync").join("pending.json"),
		}
	}

	pub fn from_xdg() -> Result<Self, ThreadStoreError> {
		let state_dir = dirs::state_dir()
			.or_else(|| dirs::data_dir().map(|d| d.join("state")))
			.ok_or_else(|| {
				ThreadStoreError::Io(std::io::Error::new(
					std::io::ErrorKind::NotFound,
					"could not determine XDG state directory",
				))
			})?;

		let sync_dir = state_dir.join("loom").join("sync");
		std::fs::create_dir_all(&sync_dir)?;

		info!(
			sync_dir = %sync_dir.display(),
			"initialized pending sync store"
		);

		Ok(Self {
			path: sync_dir.join("pending.json"),
		})
	}

	pub async fn load(&self) -> Result<PendingSyncQueue, ThreadStoreError> {
		if !self.path.exists() {
			debug!(path = %self.path.display(), "pending sync file not found, returning empty queue");
			return Ok(PendingSyncQueue::new());
		}

		let contents = tokio::fs::read_to_string(&self.path).await?;
		let queue: PendingSyncQueue = serde_json::from_str(&contents)?;

		debug!(
			path = %self.path.display(),
			count = queue.len(),
			"loaded pending sync queue"
		);

		Ok(queue)
	}

	pub async fn save(&self, queue: &PendingSyncQueue) -> Result<(), ThreadStoreError> {
		if let Some(parent) = self.path.parent() {
			tokio::fs::create_dir_all(parent).await?;
		}

		let tmp_path = self.path.with_extension("json.tmp");
		let json = serde_json::to_string_pretty(queue)?;

		tokio::fs::write(&tmp_path, &json).await?;
		tokio::fs::rename(&tmp_path, &self.path).await?;

		debug!(
			path = %self.path.display(),
			count = queue.len(),
			"saved pending sync queue"
		);

		Ok(())
	}

	pub async fn add_pending(
		&self,
		thread_id: ThreadId,
		operation: SyncOperation,
		error: Option<String>,
	) -> Result<(), ThreadStoreError> {
		let mut queue = self.load().await.unwrap_or_else(|e| {
			warn!(error = %e, "failed to load pending queue, starting fresh");
			PendingSyncQueue::new()
		});

		queue.add(thread_id.clone(), operation.clone(), error);
		self.save(&queue).await?;

		info!(
			thread_id = %thread_id,
			operation = ?operation,
			"added to pending sync queue"
		);

		Ok(())
	}

	pub async fn remove_pending(
		&self,
		thread_id: &ThreadId,
		operation: &SyncOperation,
	) -> Result<(), ThreadStoreError> {
		let mut queue = self.load().await?;
		let before = queue.len();
		queue.remove(thread_id, operation);

		if queue.len() < before {
			self.save(&queue).await?;
			debug!(
				thread_id = %thread_id,
				operation = ?operation,
				"removed from pending sync queue"
			);
		}

		Ok(())
	}

	pub async fn clear(&self) -> Result<(), ThreadStoreError> {
		if self.path.exists() {
			tokio::fs::remove_file(&self.path).await?;
			info!(path = %self.path.display(), "cleared pending sync queue");
		}
		Ok(())
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use tempfile::TempDir;

	async fn create_test_store() -> (PendingSyncStore, TempDir) {
		let tmp = TempDir::new().unwrap();
		let store = PendingSyncStore::new(tmp.path().to_path_buf());
		(store, tmp)
	}

	#[tokio::test]
	async fn test_add_and_load_pending() {
		let (store, _tmp) = create_test_store().await;
		let id = ThreadId::new();

		store
			.add_pending(
				id.clone(),
				SyncOperation::Upsert,
				Some("network error".into()),
			)
			.await
			.unwrap();

		let queue = store.load().await.unwrap();
		assert_eq!(queue.len(), 1);
		assert_eq!(queue.entries[0].thread_id, id);
		assert_eq!(queue.entries[0].operation, SyncOperation::Upsert);
		assert_eq!(queue.entries[0].retry_count, 0);
	}

	#[tokio::test]
	async fn test_retry_count_increments() {
		let (store, _tmp) = create_test_store().await;
		let id = ThreadId::new();

		store
			.add_pending(id.clone(), SyncOperation::Upsert, None)
			.await
			.unwrap();
		store
			.add_pending(id.clone(), SyncOperation::Upsert, Some("error 2".into()))
			.await
			.unwrap();

		let queue = store.load().await.unwrap();
		assert_eq!(queue.len(), 1);
		assert_eq!(queue.entries[0].retry_count, 1);
	}

	#[tokio::test]
	async fn test_remove_pending() {
		let (store, _tmp) = create_test_store().await;
		let id = ThreadId::new();

		store
			.add_pending(id.clone(), SyncOperation::Upsert, None)
			.await
			.unwrap();
		store
			.remove_pending(&id, &SyncOperation::Upsert)
			.await
			.unwrap();

		let queue = store.load().await.unwrap();
		assert!(queue.is_empty());
	}

	#[tokio::test]
	async fn test_different_operations_tracked_separately() {
		let (store, _tmp) = create_test_store().await;
		let id = ThreadId::new();

		store
			.add_pending(id.clone(), SyncOperation::Upsert, None)
			.await
			.unwrap();
		store
			.add_pending(id.clone(), SyncOperation::Delete, None)
			.await
			.unwrap();

		let queue = store.load().await.unwrap();
		assert_eq!(queue.len(), 2);
	}

	#[tokio::test]
	async fn test_clear() {
		let (store, _tmp) = create_test_store().await;
		let id = ThreadId::new();

		store
			.add_pending(id, SyncOperation::Upsert, None)
			.await
			.unwrap();
		store.clear().await.unwrap();

		let queue = store.load().await.unwrap();
		assert!(queue.is_empty());
	}
}
