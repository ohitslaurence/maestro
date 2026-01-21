//! IndexStore: small index for recent threads and fast list queries (§2).
//!
//! Stores a thread index at `storage_root()/index.json`.
//! Automatically rebuilds from `threads/` when missing (§5).

use super::{read_json, write_json, StorageResult, ThreadStore, ThreadSummary};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tokio::fs;

/// Current schema version for the index.
pub const INDEX_SCHEMA_VERSION: u32 = 1;

// ============================================================================
// ThreadIndex (§2, §3)
// ============================================================================

/// Index of recent threads for fast list queries.
///
/// Stored at `index.json`. Rebuilt from threads/ if missing (§5).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadIndex {
    /// Schema version for migrations.
    pub schema_version: u32,
    /// List of thread summaries, ordered by updatedAt descending.
    pub threads: Vec<ThreadSummary>,
    /// ISO 8601 timestamp when index was last rebuilt.
    pub rebuilt_at: String,
}

impl Default for ThreadIndex {
    fn default() -> Self {
        Self {
            schema_version: INDEX_SCHEMA_VERSION,
            threads: Vec::new(),
            rebuilt_at: chrono::Utc::now().to_rfc3339(),
        }
    }
}

// ============================================================================
// IndexStore (§2, §5)
// ============================================================================

/// IndexStore manages the thread index for fast list queries.
///
/// Per spec §5:
/// - Missing index: rebuild by scanning `threads/`.
/// - Index is updated on thread save/delete operations.
pub struct IndexStore {
    root: PathBuf,
}

impl IndexStore {
    /// Create a new IndexStore rooted at the given path.
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    /// Get the path to the index file.
    fn index_path(&self) -> PathBuf {
        self.root.join("index.json")
    }

    /// Load the index, rebuilding if missing or corrupt (§5).
    ///
    /// This is the primary entry point for reading the index.
    /// If the index file is missing, it triggers a full rebuild
    /// by scanning the threads directory.
    pub async fn load_or_rebuild(&self) -> StorageResult<ThreadIndex> {
        let path = self.index_path();

        // Try to load existing index
        if path.exists() {
            match read_json::<ThreadIndex>(&path).await {
                Ok(index) => {
                    // Check schema version
                    if index.schema_version == INDEX_SCHEMA_VERSION {
                        return Ok(index);
                    }
                    // Schema mismatch: rebuild
                    eprintln!(
                        "Index schema version mismatch (expected {}, found {}), rebuilding",
                        INDEX_SCHEMA_VERSION, index.schema_version
                    );
                }
                Err(e) => {
                    // Corrupt index: log and rebuild
                    eprintln!("Failed to read index, rebuilding: {}", e);
                }
            }
        }

        // Index missing or corrupt: rebuild from threads/
        self.rebuild().await
    }

    /// Rebuild the index by scanning the threads directory (§5).
    ///
    /// This scans all thread files and creates a fresh index.
    /// Used when:
    /// - Index file is missing
    /// - Index is corrupt
    /// - Schema version mismatch
    pub async fn rebuild(&self) -> StorageResult<ThreadIndex> {
        // Use ThreadStore to list all threads
        let thread_store = ThreadStore::new(self.root.clone());
        let threads = thread_store.list().await?;

        let index = ThreadIndex {
            schema_version: INDEX_SCHEMA_VERSION,
            threads,
            rebuilt_at: chrono::Utc::now().to_rfc3339(),
        };

        // Save the rebuilt index
        self.save(&index).await?;

        Ok(index)
    }

    /// Save the index atomically.
    pub async fn save(&self, index: &ThreadIndex) -> StorageResult<()> {
        let path = self.index_path();
        write_json(&path, index).await
    }

    /// Update the index with a new or modified thread summary.
    ///
    /// Inserts or updates the summary, maintaining sort order
    /// (pinned first, then by updatedAt descending).
    pub async fn upsert_thread(&self, summary: ThreadSummary) -> StorageResult<ThreadIndex> {
        let mut index = self.load_or_rebuild().await?;

        // Remove existing entry if present
        index.threads.retain(|t| t.id != summary.id);

        // Insert new entry
        index.threads.push(summary);

        // Re-sort: pinned first, then by updatedAt descending
        index.threads.sort_by(|a, b| {
            match (a.pinned, b.pinned) {
                (true, false) => std::cmp::Ordering::Less,
                (false, true) => std::cmp::Ordering::Greater,
                _ => b.updated_at.cmp(&a.updated_at),
            }
        });

        self.save(&index).await?;
        Ok(index)
    }

    /// Remove a thread from the index.
    pub async fn remove_thread(&self, thread_id: &str) -> StorageResult<ThreadIndex> {
        let mut index = self.load_or_rebuild().await?;
        index.threads.retain(|t| t.id != thread_id);
        self.save(&index).await?;
        Ok(index)
    }

    /// Get all thread summaries from the index.
    ///
    /// This loads or rebuilds the index, then returns the threads.
    pub async fn list(&self) -> StorageResult<Vec<ThreadSummary>> {
        let index = self.load_or_rebuild().await?;
        Ok(index.threads)
    }

    /// Check if the index file exists.
    #[allow(dead_code)]
    pub fn exists(&self) -> bool {
        self.index_path().exists()
    }

    /// Delete the index file.
    ///
    /// The next call to load_or_rebuild will trigger a rebuild.
    #[allow(dead_code)]
    pub async fn delete(&self) -> StorageResult<()> {
        let path = self.index_path();
        if path.exists() {
            fs::remove_file(&path).await?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::{ThreadMetadata, ThreadPrivacy, ThreadRecord, THREAD_SCHEMA_VERSION};
    use tempfile::tempdir;

    fn make_test_thread(id: &str, updated_at: &str, pinned: bool) -> ThreadRecord {
        ThreadRecord {
            schema_version: THREAD_SCHEMA_VERSION,
            id: id.to_string(),
            title: format!("Test Thread {}", id),
            created_at: "2026-01-21T10:00:00Z".to_string(),
            updated_at: updated_at.to_string(),
            project_path: "/test/project".to_string(),
            harness: "opencode".to_string(),
            model: "gpt-4.1".to_string(),
            last_session_id: None,
            state_snapshot: None,
            privacy: ThreadPrivacy::default(),
            metadata: ThreadMetadata {
                tags: vec![],
                pinned,
            },
        }
    }

    #[tokio::test]
    async fn test_load_or_rebuild_creates_index_when_missing() {
        let dir = tempdir().unwrap();
        let store = IndexStore::new(dir.path().to_path_buf());

        // No index exists yet
        assert!(!store.exists());

        // Load should create an empty index
        let index = store.load_or_rebuild().await.unwrap();
        assert!(index.threads.is_empty());
        assert!(store.exists());
    }

    #[tokio::test]
    async fn test_rebuild_scans_threads_directory() {
        let dir = tempdir().unwrap();
        let thread_store = ThreadStore::new(dir.path().to_path_buf());
        let index_store = IndexStore::new(dir.path().to_path_buf());

        // Create some threads
        thread_store
            .save(make_test_thread("thr_1", "2026-01-21T10:00:00Z", false))
            .await
            .unwrap();
        thread_store
            .save(make_test_thread("thr_2", "2026-01-21T11:00:00Z", false))
            .await
            .unwrap();
        thread_store
            .save(make_test_thread("thr_3", "2026-01-21T09:00:00Z", true))
            .await
            .unwrap();

        // Delete any existing index
        index_store.delete().await.unwrap();
        assert!(!index_store.exists());

        // Rebuild should scan threads
        let index = index_store.rebuild().await.unwrap();
        assert_eq!(index.threads.len(), 3);

        // Pinned thread should be first
        assert!(index.threads[0].pinned);
    }

    #[tokio::test]
    async fn test_upsert_thread_maintains_sort_order() {
        let dir = tempdir().unwrap();
        let index_store = IndexStore::new(dir.path().to_path_buf());

        // Add threads
        let s1 = ThreadSummary {
            id: "thr_1".to_string(),
            title: "Thread 1".to_string(),
            updated_at: "2026-01-21T10:00:00Z".to_string(),
            project_path: "/test".to_string(),
            harness: "opencode".to_string(),
            pinned: false,
        };

        let s2 = ThreadSummary {
            id: "thr_2".to_string(),
            title: "Thread 2".to_string(),
            updated_at: "2026-01-21T11:00:00Z".to_string(),
            project_path: "/test".to_string(),
            harness: "opencode".to_string(),
            pinned: false,
        };

        let s3_pinned = ThreadSummary {
            id: "thr_3".to_string(),
            title: "Thread 3".to_string(),
            updated_at: "2026-01-21T09:00:00Z".to_string(),
            project_path: "/test".to_string(),
            harness: "opencode".to_string(),
            pinned: true,
        };

        index_store.upsert_thread(s1).await.unwrap();
        index_store.upsert_thread(s2).await.unwrap();
        let index = index_store.upsert_thread(s3_pinned).await.unwrap();

        // Check order: pinned first, then by date descending
        assert_eq!(index.threads[0].id, "thr_3"); // pinned
        assert_eq!(index.threads[1].id, "thr_2"); // newer
        assert_eq!(index.threads[2].id, "thr_1"); // older
    }

    #[tokio::test]
    async fn test_upsert_updates_existing_thread() {
        let dir = tempdir().unwrap();
        let index_store = IndexStore::new(dir.path().to_path_buf());

        let s1 = ThreadSummary {
            id: "thr_1".to_string(),
            title: "Original Title".to_string(),
            updated_at: "2026-01-21T10:00:00Z".to_string(),
            project_path: "/test".to_string(),
            harness: "opencode".to_string(),
            pinned: false,
        };

        index_store.upsert_thread(s1).await.unwrap();

        // Update same thread
        let s1_updated = ThreadSummary {
            id: "thr_1".to_string(),
            title: "Updated Title".to_string(),
            updated_at: "2026-01-21T12:00:00Z".to_string(),
            project_path: "/test".to_string(),
            harness: "opencode".to_string(),
            pinned: false,
        };

        let index = index_store.upsert_thread(s1_updated).await.unwrap();

        assert_eq!(index.threads.len(), 1);
        assert_eq!(index.threads[0].title, "Updated Title");
    }

    #[tokio::test]
    async fn test_remove_thread() {
        let dir = tempdir().unwrap();
        let index_store = IndexStore::new(dir.path().to_path_buf());

        let s1 = ThreadSummary {
            id: "thr_1".to_string(),
            title: "Thread 1".to_string(),
            updated_at: "2026-01-21T10:00:00Z".to_string(),
            project_path: "/test".to_string(),
            harness: "opencode".to_string(),
            pinned: false,
        };

        let s2 = ThreadSummary {
            id: "thr_2".to_string(),
            title: "Thread 2".to_string(),
            updated_at: "2026-01-21T11:00:00Z".to_string(),
            project_path: "/test".to_string(),
            harness: "opencode".to_string(),
            pinned: false,
        };

        index_store.upsert_thread(s1).await.unwrap();
        index_store.upsert_thread(s2).await.unwrap();

        let index = index_store.remove_thread("thr_1").await.unwrap();

        assert_eq!(index.threads.len(), 1);
        assert_eq!(index.threads[0].id, "thr_2");
    }

    #[tokio::test]
    async fn test_schema_version_mismatch_triggers_rebuild() {
        let dir = tempdir().unwrap();
        let index_store = IndexStore::new(dir.path().to_path_buf());

        // Create an index with wrong schema version
        let bad_index = ThreadIndex {
            schema_version: 999,
            threads: vec![],
            rebuilt_at: "2026-01-21T10:00:00Z".to_string(),
        };

        write_json(&index_store.index_path(), &bad_index)
            .await
            .unwrap();

        // Load should detect mismatch and rebuild
        let index = index_store.load_or_rebuild().await.unwrap();
        assert_eq!(index.schema_version, INDEX_SCHEMA_VERSION);
    }

    #[tokio::test]
    async fn test_delete_index() {
        let dir = tempdir().unwrap();
        let index_store = IndexStore::new(dir.path().to_path_buf());

        // Create index
        index_store.load_or_rebuild().await.unwrap();
        assert!(index_store.exists());

        // Delete
        index_store.delete().await.unwrap();
        assert!(!index_store.exists());
    }
}
