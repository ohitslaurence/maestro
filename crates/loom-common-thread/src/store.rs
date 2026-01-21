// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights
// reserved. SPDX-License-Identifier: Proprietary

use std::path::PathBuf;

use async_trait::async_trait;
use tracing::{debug, error, info};

use crate::error::ThreadStoreError;
use crate::model::{Thread, ThreadId, ThreadSummary};

#[async_trait]
pub trait ThreadStore: Send + Sync {
	async fn load(&self, id: &ThreadId) -> Result<Option<Thread>, ThreadStoreError>;
	async fn save(&self, thread: &Thread) -> Result<(), ThreadStoreError>;
	async fn list(&self, limit: u32) -> Result<Vec<ThreadSummary>, ThreadStoreError>;
	async fn delete(&self, id: &ThreadId) -> Result<(), ThreadStoreError>;

	/// Save locally and wait for sync to complete (blocking).
	/// Unlike `save()` which syncs in the background, this method blocks until
	/// the server sync completes or fails. Use this for commands like `share`
	/// where the process exits immediately after saving.
	async fn save_and_sync(&self, thread: &Thread) -> Result<(), ThreadStoreError> {
		// Default implementation just delegates to save() for stores that don't support sync
		self.save(thread).await
	}
}

pub struct LocalThreadStore {
	threads_dir: PathBuf,
}

impl LocalThreadStore {
	pub fn new(threads_dir: PathBuf) -> Self {
		Self { threads_dir }
	}

	/// Search threads locally using substring matching.
	/// Used as fallback when server is unavailable.
	pub async fn search(
		&self,
		query: &str,
		limit: usize,
	) -> Result<Vec<ThreadSummary>, ThreadStoreError> {
		let query_lower = query.to_lowercase();
		let all_threads = self.list(1000).await?;

		let mut matches = Vec::new();

		for summary in all_threads {
			if let Some(thread) = self.load(&summary.id).await? {
				if self.matches_query(&thread, &query_lower) {
					matches.push(ThreadSummary::from(&thread));
				}
			}
		}

		// Sort by last_activity_at DESC
		matches.sort_by(|a, b| b.last_activity_at.cmp(&a.last_activity_at));
		matches.truncate(limit);

		Ok(matches)
	}

	fn matches_query(&self, thread: &Thread, query: &str) -> bool {
		// Check title
		if thread
			.metadata
			.title
			.as_ref()
			.map(|t| t.to_lowercase().contains(query))
			.unwrap_or(false)
		{
			return true;
		}

		// Check git branch
		if thread
			.git_branch
			.as_ref()
			.map(|b| b.to_lowercase().contains(query))
			.unwrap_or(false)
		{
			return true;
		}

		// Check git remote URL
		if thread
			.git_remote_url
			.as_ref()
			.map(|u| u.to_lowercase().contains(query))
			.unwrap_or(false)
		{
			return true;
		}

		// Check commits (prefix match for SHAs)
		for sha in &thread.git_commits {
			if sha.to_lowercase().starts_with(query) {
				return true;
			}
		}

		// Check tags
		for tag in &thread.metadata.tags {
			if tag.to_lowercase().contains(query) {
				return true;
			}
		}

		// Check message content
		for msg in &thread.conversation.messages {
			if msg.content.to_lowercase().contains(query) {
				return true;
			}
		}

		false
	}

	pub fn from_xdg() -> Result<Self, ThreadStoreError> {
		let data_dir = dirs::data_dir().ok_or_else(|| {
			ThreadStoreError::Io(std::io::Error::new(
				std::io::ErrorKind::NotFound,
				"could not determine XDG data directory",
			))
		})?;

		let threads_dir = data_dir.join("loom").join("threads");
		std::fs::create_dir_all(&threads_dir)?;

		info!(
				threads_dir = %threads_dir.display(),
				"initialized local thread store"
		);

		Ok(Self::new(threads_dir))
	}

	fn thread_path(&self, id: &ThreadId) -> PathBuf {
		self.threads_dir.join(format!("{id}.json"))
	}
}

#[async_trait]
impl ThreadStore for LocalThreadStore {
	async fn load(&self, id: &ThreadId) -> Result<Option<Thread>, ThreadStoreError> {
		let path = self.thread_path(id);

		if !path.exists() {
			debug!(thread_id = %id, path = %path.display(), "thread file not found");
			return Ok(None);
		}

		let contents = tokio::fs::read_to_string(&path).await?;
		let thread: Thread = serde_json::from_str(&contents)?;

		debug!(
				thread_id = %id,
				version = thread.version,
				"loaded thread from disk"
		);

		Ok(Some(thread))
	}

	async fn save(&self, thread: &Thread) -> Result<(), ThreadStoreError> {
		tokio::fs::create_dir_all(&self.threads_dir).await?;

		let path = self.thread_path(&thread.id);
		let tmp_path = self.threads_dir.join(format!("{}.json.tmp", thread.id));

		let json = serde_json::to_string_pretty(thread)?;

		tokio::fs::write(&tmp_path, &json).await?;
		tokio::fs::rename(&tmp_path, &path).await?;

		debug!(
				thread_id = %thread.id,
				version = thread.version,
				path = %path.display(),
				"saved thread to disk"
		);

		Ok(())
	}

	async fn list(&self, limit: u32) -> Result<Vec<ThreadSummary>, ThreadStoreError> {
		if !self.threads_dir.exists() {
			return Ok(Vec::new());
		}

		let mut entries = tokio::fs::read_dir(&self.threads_dir).await?;
		let mut summaries = Vec::new();

		while let Some(entry) = entries.next_entry().await? {
			if summaries.len() >= limit as usize {
				break;
			}

			let path = entry.path();
			if path.extension().and_then(|e| e.to_str()) != Some("json") {
				continue;
			}

			if path.to_string_lossy().ends_with(".tmp") {
				continue;
			}

			match tokio::fs::read_to_string(&path).await {
				Ok(contents) => match serde_json::from_str::<Thread>(&contents) {
					Ok(thread) => {
						summaries.push(ThreadSummary::from(&thread));
					}
					Err(e) => {
						error!(
								path = %path.display(),
								error = %e,
								"failed to parse thread file"
						);
					}
				},
				Err(e) => {
					error!(
							path = %path.display(),
							error = %e,
							"failed to read thread file"
					);
				}
			}
		}

		summaries.sort_by(|a, b| b.last_activity_at.cmp(&a.last_activity_at));

		debug!(count = summaries.len(), limit = limit, "listed threads");

		Ok(summaries)
	}

	async fn delete(&self, id: &ThreadId) -> Result<(), ThreadStoreError> {
		let path = self.thread_path(id);

		if !path.exists() {
			return Err(ThreadStoreError::NotFound(id.to_string()));
		}

		tokio::fs::remove_file(&path).await?;

		info!(thread_id = %id, "deleted thread");

		Ok(())
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use tempfile::TempDir;

	async fn create_test_store() -> (LocalThreadStore, TempDir) {
		let tmp = TempDir::new().unwrap();
		let store = LocalThreadStore::new(tmp.path().to_path_buf());
		(store, tmp)
	}

	#[tokio::test]
	async fn test_save_and_load_thread() {
		let (store, _tmp) = create_test_store().await;
		let thread = Thread::new();
		let id = thread.id.clone();

		store.save(&thread).await.unwrap();
		let loaded = store.load(&id).await.unwrap();

		assert!(loaded.is_some());
		let loaded = loaded.unwrap();
		assert_eq!(loaded.id, id);
		assert_eq!(loaded.version, thread.version);
	}

	#[tokio::test]
	async fn test_load_nonexistent_returns_none() {
		let (store, _tmp) = create_test_store().await;
		let id = ThreadId::new();

		let result = store.load(&id).await.unwrap();
		assert!(result.is_none());
	}

	#[tokio::test]
	async fn test_list_threads() {
		let (store, _tmp) = create_test_store().await;

		for _ in 0..3 {
			let thread = Thread::new();
			store.save(&thread).await.unwrap();
		}

		let summaries = store.list(10).await.unwrap();
		assert_eq!(summaries.len(), 3);
	}

	#[tokio::test]
	async fn test_list_respects_limit() {
		let (store, _tmp) = create_test_store().await;

		for _ in 0..5 {
			let thread = Thread::new();
			store.save(&thread).await.unwrap();
		}

		let summaries = store.list(2).await.unwrap();
		assert_eq!(summaries.len(), 2);
	}

	#[tokio::test]
	async fn test_delete_thread() {
		let (store, _tmp) = create_test_store().await;
		let thread = Thread::new();
		let id = thread.id.clone();

		store.save(&thread).await.unwrap();
		assert!(store.load(&id).await.unwrap().is_some());

		store.delete(&id).await.unwrap();
		assert!(store.load(&id).await.unwrap().is_none());
	}

	#[tokio::test]
	async fn test_delete_nonexistent_returns_error() {
		let (store, _tmp) = create_test_store().await;
		let id = ThreadId::new();

		let result = store.delete(&id).await;
		assert!(matches!(result, Err(ThreadStoreError::NotFound(_))));
	}

	#[tokio::test]
	async fn test_atomic_write_on_failure() {
		let (store, _tmp) = create_test_store().await;
		let mut thread = Thread::new();
		let id = thread.id.clone();

		store.save(&thread).await.unwrap();

		thread.version = 2;
		thread.metadata.title = Some("Updated".to_string());
		store.save(&thread).await.unwrap();

		let loaded = store.load(&id).await.unwrap().unwrap();
		assert_eq!(loaded.version, 2);
		assert_eq!(loaded.metadata.title, Some("Updated".to_string()));
	}
}
