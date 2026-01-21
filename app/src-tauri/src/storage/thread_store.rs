//! ThreadStore: CRUD for thread metadata and snapshots (§2, §3).
//!
//! Stores thread records under `storage_root()/threads/<thread_id>.json`.

use super::{read_json, write_json, StorageError, StorageResult};
use crate::agent_state::AgentStateSnapshot;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tokio::fs;

/// Current schema version for thread records.
pub const THREAD_SCHEMA_VERSION: u32 = 1;

// ============================================================================
// Privacy Settings (§3)
// ============================================================================

/// Privacy controls for a thread.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadPrivacy {
    /// If true, thread is never synced to remote.
    pub local_only: bool,
    /// If true, redact user inputs before persistence.
    pub redact_inputs: bool,
    /// If true, redact assistant outputs before persistence.
    pub redact_outputs: bool,
}

impl Default for ThreadPrivacy {
    fn default() -> Self {
        Self {
            local_only: true,
            redact_inputs: false,
            redact_outputs: false,
        }
    }
}

// ============================================================================
// Thread Metadata (§3)
// ============================================================================

/// User-defined metadata for a thread.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThreadMetadata {
    /// Tags for categorization.
    pub tags: Vec<String>,
    /// Whether the thread is pinned to the top of lists.
    pub pinned: bool,
}

// ============================================================================
// ThreadRecord (§3)
// ============================================================================

/// User-visible conversation container.
///
/// One thread per workspace context. Stored at `threads/<thread_id>.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadRecord {
    /// Schema version for migrations.
    pub schema_version: u32,
    /// Unique thread identifier (e.g., "thr_123").
    pub id: String,
    /// User-visible title.
    pub title: String,
    /// ISO 8601 timestamp when thread was created.
    pub created_at: String,
    /// ISO 8601 timestamp when thread was last updated.
    pub updated_at: String,
    /// Absolute path to the project/workspace.
    pub project_path: String,
    /// Agent harness identifier (e.g., "opencode", "claude_code").
    pub harness: String,
    /// LLM model identifier.
    pub model: String,
    /// ID of the most recent session in this thread.
    pub last_session_id: Option<String>,
    /// Snapshot of agent state for resume.
    pub state_snapshot: Option<AgentStateSnapshot>,
    /// Privacy controls.
    pub privacy: ThreadPrivacy,
    /// User-defined metadata.
    pub metadata: ThreadMetadata,
}

// ============================================================================
// ThreadSummary (§2, §3)
// ============================================================================

/// Summary of a thread for list views.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadSummary {
    /// Thread ID.
    pub id: String,
    /// Thread title.
    pub title: String,
    /// ISO 8601 timestamp when thread was last updated.
    pub updated_at: String,
    /// Project path.
    pub project_path: String,
    /// Harness identifier.
    pub harness: String,
    /// Whether the thread is pinned.
    pub pinned: bool,
}

impl From<&ThreadRecord> for ThreadSummary {
    fn from(record: &ThreadRecord) -> Self {
        Self {
            id: record.id.clone(),
            title: record.title.clone(),
            updated_at: record.updated_at.clone(),
            project_path: record.project_path.clone(),
            harness: record.harness.clone(),
            pinned: record.metadata.pinned,
        }
    }
}

// ============================================================================
// ThreadStore (§2, §4)
// ============================================================================

/// ThreadStore manages thread record persistence.
pub struct ThreadStore {
    root: PathBuf,
}

impl ThreadStore {
    /// Create a new ThreadStore rooted at the given path.
    pub fn new(root: PathBuf) -> Self {
        Self {
            root: root.join("threads"),
        }
    }

    /// Get the storage path for threads.
    #[allow(dead_code)]
    pub fn path(&self) -> &PathBuf {
        &self.root
    }

    /// Get the path for a specific thread record.
    fn thread_path(&self, thread_id: &str) -> PathBuf {
        self.root.join(format!("{}.json", thread_id))
    }

    /// Load a thread by ID (§4: load_thread).
    pub async fn load(&self, thread_id: &str) -> StorageResult<ThreadRecord> {
        let path = self.thread_path(thread_id);
        let record: ThreadRecord = read_json(&path).await?;

        // Validate schema version (§6)
        if record.schema_version != THREAD_SCHEMA_VERSION {
            return Err(StorageError::SchemaVersionMismatch {
                expected: THREAD_SCHEMA_VERSION,
                found: record.schema_version,
            });
        }

        Ok(record)
    }

    /// Save a thread (§4: save_thread).
    ///
    /// Creates the thread if it doesn't exist, updates if it does.
    /// Returns the saved record with updated timestamp.
    pub async fn save(&self, mut record: ThreadRecord) -> StorageResult<ThreadRecord> {
        // Ensure schema version is current
        record.schema_version = THREAD_SCHEMA_VERSION;

        // Update timestamp
        record.updated_at = chrono::Utc::now().to_rfc3339();

        let path = self.thread_path(&record.id);
        write_json(&path, &record).await?;

        Ok(record)
    }

    /// Delete a thread by ID.
    pub async fn delete(&self, thread_id: &str) -> StorageResult<()> {
        let path = self.thread_path(thread_id);
        if path.exists() {
            fs::remove_file(&path).await?;
        }
        Ok(())
    }

    /// Check if a thread exists.
    #[allow(dead_code)]
    pub async fn exists(&self, thread_id: &str) -> bool {
        self.thread_path(thread_id).exists()
    }

    /// List all thread summaries (§4: list_threads).
    ///
    /// Scans the threads directory and returns summaries sorted by updatedAt descending.
    pub async fn list(&self) -> StorageResult<Vec<ThreadSummary>> {
        // Ensure directory exists
        if !self.root.exists() {
            fs::create_dir_all(&self.root).await?;
            return Ok(Vec::new());
        }

        let mut summaries = Vec::new();
        let mut entries = fs::read_dir(&self.root).await?;

        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if path.extension().map_or(false, |ext| ext == "json") {
                match read_json::<ThreadRecord>(&path).await {
                    Ok(record) => {
                        if record.schema_version == THREAD_SCHEMA_VERSION {
                            summaries.push(ThreadSummary::from(&record));
                        }
                        // Skip records with mismatched schema version
                    }
                    Err(_) => {
                        // Skip corrupt files (§5: edge case handling)
                        // TODO: Move to corrupt/ directory
                    }
                }
            }
        }

        // Sort by updatedAt descending, pinned first
        summaries.sort_by(|a, b| {
            match (a.pinned, b.pinned) {
                (true, false) => std::cmp::Ordering::Less,
                (false, true) => std::cmp::Ordering::Greater,
                _ => b.updated_at.cmp(&a.updated_at), // Descending by date
            }
        });

        Ok(summaries)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn make_test_thread(id: &str) -> ThreadRecord {
        ThreadRecord {
            schema_version: THREAD_SCHEMA_VERSION,
            id: id.to_string(),
            title: format!("Test Thread {}", id),
            created_at: "2026-01-21T10:00:00Z".to_string(),
            updated_at: "2026-01-21T10:00:00Z".to_string(),
            project_path: "/test/project".to_string(),
            harness: "opencode".to_string(),
            model: "gpt-4.1".to_string(),
            last_session_id: None,
            state_snapshot: None,
            privacy: ThreadPrivacy::default(),
            metadata: ThreadMetadata::default(),
        }
    }

    #[tokio::test]
    async fn test_save_and_load_thread() {
        let dir = tempdir().unwrap();
        let store = ThreadStore::new(dir.path().to_path_buf());

        let thread = make_test_thread("thr_123");
        let saved = store.save(thread).await.unwrap();

        assert_eq!(saved.id, "thr_123");
        assert_ne!(saved.updated_at, "2026-01-21T10:00:00Z"); // Updated

        let loaded = store.load("thr_123").await.unwrap();
        assert_eq!(loaded.id, saved.id);
        assert_eq!(loaded.title, saved.title);
    }

    #[tokio::test]
    async fn test_delete_thread() {
        let dir = tempdir().unwrap();
        let store = ThreadStore::new(dir.path().to_path_buf());

        let thread = make_test_thread("thr_del");
        store.save(thread).await.unwrap();
        assert!(store.exists("thr_del").await);

        store.delete("thr_del").await.unwrap();
        assert!(!store.exists("thr_del").await);
    }

    #[tokio::test]
    async fn test_list_threads_sorted() {
        let dir = tempdir().unwrap();
        let store = ThreadStore::new(dir.path().to_path_buf());

        // Create threads with different timestamps
        let mut t1 = make_test_thread("thr_1");
        t1.updated_at = "2026-01-21T10:00:00Z".to_string();

        let mut t2 = make_test_thread("thr_2");
        t2.updated_at = "2026-01-21T11:00:00Z".to_string();

        let mut t3 = make_test_thread("thr_3");
        t3.updated_at = "2026-01-21T09:00:00Z".to_string();
        t3.metadata.pinned = true;

        // Save in random order
        store.save(t1).await.unwrap();
        store.save(t3).await.unwrap();
        store.save(t2).await.unwrap();

        let summaries = store.list().await.unwrap();
        assert_eq!(summaries.len(), 3);

        // Pinned thread should be first
        assert!(summaries[0].pinned);
        assert_eq!(summaries[0].id, "thr_3");
    }

    #[tokio::test]
    async fn test_load_nonexistent_thread_fails() {
        let dir = tempdir().unwrap();
        let store = ThreadStore::new(dir.path().to_path_buf());

        let result = store.load("thr_nonexistent").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_schema_version_mismatch() {
        let dir = tempdir().unwrap();
        let store = ThreadStore::new(dir.path().to_path_buf());

        // Create a thread with wrong schema version
        let mut thread = make_test_thread("thr_old");
        thread.schema_version = 999;

        // Write directly to bypass save() which sets correct version
        let path = store.thread_path("thr_old");
        write_json(&path, &thread).await.unwrap();

        let result = store.load("thr_old").await;
        assert!(matches!(
            result,
            Err(StorageError::SchemaVersionMismatch { .. })
        ));
    }
}
