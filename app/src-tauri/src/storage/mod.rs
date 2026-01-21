//! Local-first session persistence storage layer.
//!
//! Per session-persistence.md §2, this module provides:
//! - ThreadStore: CRUD for thread metadata and snapshots
//! - SessionStore: CRUD for session runtime records
//! - MessageStore: append-only message log
//! - SyncQueue: durable queue of upsert/delete intents (optional)
//! - IndexStore: small index for recent threads and fast list queries
//!
//! Storage root: `app_data_dir()/sessions` (§3)

use serde::de::DeserializeOwned;
use serde::Serialize;
use std::io;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager, Runtime};
use tokio::fs;

pub mod message_store;
pub mod session_store;
pub mod thread_store;

pub use message_store::MessageStore;
pub use session_store::{
    SessionAgentConfig, SessionRecord, SessionStatus, SessionStore, SessionToolRun,
    SessionToolRunStatus, SESSION_SCHEMA_VERSION,
};
pub use thread_store::{
    ThreadMetadata, ThreadPrivacy, ThreadRecord, ThreadStore, ThreadSummary, THREAD_SCHEMA_VERSION,
};

/// Error types for storage operations (§6).
#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    #[error("storage unavailable: {0}")]
    StorageUnavailable(String),

    #[error("serialization failed: {0}")]
    SerializationFailed(#[from] serde_json::Error),

    #[error("atomic write failed: {0}")]
    AtomicWriteFailed(io::Error),

    #[error("schema version mismatch: expected {expected}, found {found}")]
    SchemaVersionMismatch { expected: u32, found: u32 },

    #[error("io error: {0}")]
    Io(#[from] io::Error),
}

pub type StorageResult<T> = Result<T, StorageError>;

/// Resolve the storage root directory under `app_data_dir()/sessions` (§2, §3).
pub fn storage_root<R: Runtime>(app: &AppHandle<R>) -> StorageResult<PathBuf> {
    let app_data = app
        .path()
        .app_data_dir()
        .map_err(|e| StorageError::StorageUnavailable(e.to_string()))?;
    Ok(app_data.join("sessions"))
}

/// Write bytes atomically using temp file + rename (§4, §5).
///
/// Creates parent directories if needed. Writes to a `.tmp` file first,
/// then renames to the target path for crash safety.
pub async fn write_atomic(path: &Path, bytes: &[u8]) -> StorageResult<()> {
    // Ensure parent directory exists
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).await?;
    }

    // Write to temp file in same directory (ensures same filesystem for rename)
    let tmp_path = path.with_extension("tmp");
    fs::write(&tmp_path, bytes)
        .await
        .map_err(StorageError::AtomicWriteFailed)?;

    // Atomic rename
    fs::rename(&tmp_path, path)
        .await
        .map_err(StorageError::AtomicWriteFailed)?;

    Ok(())
}

/// Read and deserialize JSON from a file (§4).
pub async fn read_json<T: DeserializeOwned>(path: &Path) -> StorageResult<T> {
    let bytes = fs::read(path).await?;
    let value = serde_json::from_slice(&bytes)?;
    Ok(value)
}

/// Write a value as JSON atomically (§4).
pub async fn write_json<T: Serialize>(path: &Path, value: &T) -> StorageResult<()> {
    let bytes = serde_json::to_vec_pretty(value)?;
    write_atomic(path, &bytes).await
}

// ============================================================================
// Tauri Commands (§4)
// ============================================================================

/// List all thread summaries (§4: list_threads).
///
/// Returns summaries sorted by updatedAt descending, with pinned threads first.
#[tauri::command]
pub async fn list_threads(app: tauri::AppHandle) -> Result<Vec<ThreadSummary>, String> {
    let root = storage_root(&app).map_err(|e| e.to_string())?;
    let store = ThreadStore::new(root);
    store.list().await.map_err(|e| e.to_string())
}

/// Load a thread by ID (§4: load_thread).
#[tauri::command]
pub async fn load_thread(app: tauri::AppHandle, thread_id: String) -> Result<ThreadRecord, String> {
    let root = storage_root(&app).map_err(|e| e.to_string())?;
    let store = ThreadStore::new(root);
    store.load(&thread_id).await.map_err(|e| e.to_string())
}

/// Save a thread (§4: save_thread).
///
/// Creates the thread if it doesn't exist, updates if it does.
/// Returns the saved record with updated timestamp.
#[tauri::command]
pub async fn save_thread(
    app: tauri::AppHandle,
    thread: ThreadRecord,
) -> Result<ThreadRecord, String> {
    let root = storage_root(&app).map_err(|e| e.to_string())?;
    let store = ThreadStore::new(root);
    store.save(thread).await.map_err(|e| e.to_string())
}

/// Create a new session (§4: create_session).
///
/// Generates a new session ID and persists the record.
#[tauri::command]
pub async fn create_session(
    app: tauri::AppHandle,
    thread_id: String,
    workspace_root: String,
    agent_config: SessionAgentConfig,
) -> Result<SessionRecord, String> {
    let root = storage_root(&app).map_err(|e| e.to_string())?;
    let store = SessionStore::new(root);
    store
        .create(&thread_id, &workspace_root, agent_config)
        .await
        .map_err(|e| e.to_string())
}

/// Mark a session as ended (§4: mark_session_ended).
///
/// Updates the session status and sets the ended_at timestamp.
#[tauri::command]
pub async fn mark_session_ended(
    app: tauri::AppHandle,
    session_id: String,
    status: SessionStatus,
) -> Result<(), String> {
    let root = storage_root(&app).map_err(|e| e.to_string())?;
    let store = SessionStore::new(root);
    store
        .mark_ended(&session_id, status)
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_write_atomic_creates_parent_dirs() {
        let dir = tempdir().unwrap();
        let nested_path = dir.path().join("a").join("b").join("test.txt");

        write_atomic(&nested_path, b"hello").await.unwrap();

        let contents = fs::read_to_string(&nested_path).await.unwrap();
        assert_eq!(contents, "hello");
    }

    #[tokio::test]
    async fn test_write_atomic_no_tmp_file_remains() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.txt");
        let tmp_path = path.with_extension("tmp");

        write_atomic(&path, b"data").await.unwrap();

        assert!(path.exists());
        assert!(!tmp_path.exists());
    }

    #[tokio::test]
    async fn test_read_write_json_roundtrip() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("data.json");

        #[derive(Debug, PartialEq, serde::Serialize, serde::Deserialize)]
        struct TestData {
            name: String,
            value: i32,
        }

        let original = TestData {
            name: "test".to_string(),
            value: 42,
        };

        write_json(&path, &original).await.unwrap();
        let loaded: TestData = read_json(&path).await.unwrap();

        assert_eq!(original, loaded);
    }
}
