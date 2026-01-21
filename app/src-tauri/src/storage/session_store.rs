//! SessionStore: CRUD for session runtime records (§2, §3).
//!
//! Stores session records under `storage_root()/sessions/<session_id>.json`.

use super::{read_json, write_json, StorageError, StorageResult};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use tokio::fs;

/// Current schema version for session records.
pub const SESSION_SCHEMA_VERSION: u32 = 1;

// ============================================================================
// Session Status (§3)
// ============================================================================

/// Status of a session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionStatus {
    Running,
    Completed,
    Failed,
    Stopped,
}

// ============================================================================
// Session Agent Config (§3)
// ============================================================================

/// Agent configuration embedded in a session.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionAgentConfig {
    /// Harness identifier.
    pub harness: String,
    /// Hash of the agent configuration for change detection.
    pub config_hash: String,
    /// Environment variables passed to the agent.
    pub env: HashMap<String, String>,
}

// ============================================================================
// Session Tool Run (§3)
// ============================================================================

/// Tool execution summary within a session.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionToolRun {
    /// Unique run identifier.
    pub run_id: String,
    /// Name of the tool that was executed.
    pub tool_name: String,
    /// Final status of the tool run.
    pub status: SessionToolRunStatus,
}

/// Status of a tool run within a session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SessionToolRunStatus {
    Succeeded,
    Failed,
    Canceled,
}

// ============================================================================
// SessionRecord (§3)
// ============================================================================

/// Runtime instance of an agent.
///
/// Linked to a thread. Stored at `sessions/<session_id>.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionRecord {
    /// Schema version for migrations.
    pub schema_version: u32,
    /// Unique session identifier (e.g., "ses_456").
    pub id: String,
    /// ID of the thread this session belongs to.
    pub thread_id: String,
    /// Current status of the session.
    pub status: SessionStatus,
    /// ISO 8601 timestamp when session started.
    pub started_at: String,
    /// ISO 8601 timestamp when session ended, or null if running.
    pub ended_at: Option<String>,
    /// Absolute path to the workspace root.
    pub workspace_root: String,
    /// Agent configuration.
    pub agent: SessionAgentConfig,
    /// Summary of tool executions in this session.
    pub tool_runs: Vec<SessionToolRun>,
}

// ============================================================================
// SessionStore (§2, §4)
// ============================================================================

/// SessionStore manages session record persistence.
pub struct SessionStore {
    root: PathBuf,
}

impl SessionStore {
    /// Create a new SessionStore rooted at the given path.
    pub fn new(root: PathBuf) -> Self {
        Self {
            root: root.join("sessions"),
        }
    }

    /// Get the storage path for sessions.
    #[allow(dead_code)]
    pub fn path(&self) -> &PathBuf {
        &self.root
    }

    /// Get the path for a specific session record.
    fn session_path(&self, session_id: &str) -> PathBuf {
        self.root.join(format!("{}.json", session_id))
    }

    /// Load a session by ID.
    pub async fn load(&self, session_id: &str) -> StorageResult<SessionRecord> {
        let path = self.session_path(session_id);
        let record: SessionRecord = read_json(&path).await?;

        // Validate schema version (§6)
        if record.schema_version != SESSION_SCHEMA_VERSION {
            return Err(StorageError::SchemaVersionMismatch {
                expected: SESSION_SCHEMA_VERSION,
                found: record.schema_version,
            });
        }

        Ok(record)
    }

    /// Create a new session (§4: create_session).
    ///
    /// Generates a new session ID and persists the record.
    pub async fn create(
        &self,
        thread_id: &str,
        workspace_root: &str,
        agent_config: SessionAgentConfig,
    ) -> StorageResult<SessionRecord> {
        let session_id = format!("ses_{}", uuid::Uuid::new_v4());
        let now = chrono::Utc::now().to_rfc3339();

        let record = SessionRecord {
            schema_version: SESSION_SCHEMA_VERSION,
            id: session_id,
            thread_id: thread_id.to_string(),
            status: SessionStatus::Running,
            started_at: now,
            ended_at: None,
            workspace_root: workspace_root.to_string(),
            agent: agent_config,
            tool_runs: Vec::new(),
        };

        let path = self.session_path(&record.id);
        write_json(&path, &record).await?;

        Ok(record)
    }

    /// Save a session (update).
    pub async fn save(&self, mut record: SessionRecord) -> StorageResult<SessionRecord> {
        // Ensure schema version is current
        record.schema_version = SESSION_SCHEMA_VERSION;

        let path = self.session_path(&record.id);
        write_json(&path, &record).await?;

        Ok(record)
    }

    /// Mark a session as ended (§4: mark_session_ended).
    ///
    /// Updates the session status and sets the ended_at timestamp.
    pub async fn mark_ended(
        &self,
        session_id: &str,
        status: SessionStatus,
    ) -> StorageResult<SessionRecord> {
        let mut record = self.load(session_id).await?;

        record.status = status;
        record.ended_at = Some(chrono::Utc::now().to_rfc3339());

        self.save(record).await
    }

    /// Add a tool run to a session.
    #[allow(dead_code)]
    pub async fn add_tool_run(
        &self,
        session_id: &str,
        tool_run: SessionToolRun,
    ) -> StorageResult<SessionRecord> {
        let mut record = self.load(session_id).await?;
        record.tool_runs.push(tool_run);
        self.save(record).await
    }

    /// Delete a session by ID.
    #[allow(dead_code)]
    pub async fn delete(&self, session_id: &str) -> StorageResult<()> {
        let path = self.session_path(session_id);
        if path.exists() {
            fs::remove_file(&path).await?;
        }
        Ok(())
    }

    /// Check if a session exists.
    #[allow(dead_code)]
    pub async fn exists(&self, session_id: &str) -> bool {
        self.session_path(session_id).exists()
    }

    /// List all sessions for a thread.
    #[allow(dead_code)]
    pub async fn list_by_thread(&self, thread_id: &str) -> StorageResult<Vec<SessionRecord>> {
        // Ensure directory exists
        if !self.root.exists() {
            fs::create_dir_all(&self.root).await?;
            return Ok(Vec::new());
        }

        let mut sessions = Vec::new();
        let mut entries = fs::read_dir(&self.root).await?;

        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if path.extension().map_or(false, |ext| ext == "json") {
                match read_json::<SessionRecord>(&path).await {
                    Ok(record) => {
                        if record.schema_version == SESSION_SCHEMA_VERSION
                            && record.thread_id == thread_id
                        {
                            sessions.push(record);
                        }
                    }
                    Err(_) => {
                        // Skip corrupt files
                    }
                }
            }
        }

        // Sort by started_at descending
        sessions.sort_by(|a, b| b.started_at.cmp(&a.started_at));

        Ok(sessions)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn make_test_config() -> SessionAgentConfig {
        SessionAgentConfig {
            harness: "opencode".to_string(),
            config_hash: "sha256:abc123".to_string(),
            env: HashMap::new(),
        }
    }

    #[tokio::test]
    async fn test_create_and_load_session() {
        let dir = tempdir().unwrap();
        let store = SessionStore::new(dir.path().to_path_buf());

        let created = store
            .create("thr_123", "/test/workspace", make_test_config())
            .await
            .unwrap();

        assert!(created.id.starts_with("ses_"));
        assert_eq!(created.thread_id, "thr_123");
        assert_eq!(created.status, SessionStatus::Running);
        assert!(created.ended_at.is_none());

        let loaded = store.load(&created.id).await.unwrap();
        assert_eq!(loaded.id, created.id);
        assert_eq!(loaded.thread_id, created.thread_id);
    }

    #[tokio::test]
    async fn test_mark_session_ended() {
        let dir = tempdir().unwrap();
        let store = SessionStore::new(dir.path().to_path_buf());

        let created = store
            .create("thr_123", "/test/workspace", make_test_config())
            .await
            .unwrap();

        let ended = store
            .mark_ended(&created.id, SessionStatus::Completed)
            .await
            .unwrap();

        assert_eq!(ended.status, SessionStatus::Completed);
        assert!(ended.ended_at.is_some());
    }

    #[tokio::test]
    async fn test_add_tool_run() {
        let dir = tempdir().unwrap();
        let store = SessionStore::new(dir.path().to_path_buf());

        let created = store
            .create("thr_123", "/test/workspace", make_test_config())
            .await
            .unwrap();

        let tool_run = SessionToolRun {
            run_id: "tool_1".to_string(),
            tool_name: "edit_file".to_string(),
            status: SessionToolRunStatus::Succeeded,
        };

        let updated = store.add_tool_run(&created.id, tool_run).await.unwrap();
        assert_eq!(updated.tool_runs.len(), 1);
        assert_eq!(updated.tool_runs[0].tool_name, "edit_file");
    }

    #[tokio::test]
    async fn test_delete_session() {
        let dir = tempdir().unwrap();
        let store = SessionStore::new(dir.path().to_path_buf());

        let created = store
            .create("thr_123", "/test/workspace", make_test_config())
            .await
            .unwrap();

        assert!(store.exists(&created.id).await);

        store.delete(&created.id).await.unwrap();
        assert!(!store.exists(&created.id).await);
    }

    #[tokio::test]
    async fn test_list_by_thread() {
        let dir = tempdir().unwrap();
        let store = SessionStore::new(dir.path().to_path_buf());

        // Create sessions for two different threads
        store
            .create("thr_1", "/test/workspace", make_test_config())
            .await
            .unwrap();
        store
            .create("thr_1", "/test/workspace", make_test_config())
            .await
            .unwrap();
        store
            .create("thr_2", "/test/workspace", make_test_config())
            .await
            .unwrap();

        let sessions_1 = store.list_by_thread("thr_1").await.unwrap();
        assert_eq!(sessions_1.len(), 2);

        let sessions_2 = store.list_by_thread("thr_2").await.unwrap();
        assert_eq!(sessions_2.len(), 1);
    }

    #[tokio::test]
    async fn test_load_nonexistent_session_fails() {
        let dir = tempdir().unwrap();
        let store = SessionStore::new(dir.path().to_path_buf());

        let result = store.load("ses_nonexistent").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_schema_version_mismatch() {
        let dir = tempdir().unwrap();
        let store = SessionStore::new(dir.path().to_path_buf());

        // Create a session with wrong schema version
        let record = SessionRecord {
            schema_version: 999,
            id: "ses_old".to_string(),
            thread_id: "thr_123".to_string(),
            status: SessionStatus::Running,
            started_at: "2026-01-21T10:00:00Z".to_string(),
            ended_at: None,
            workspace_root: "/test".to_string(),
            agent: make_test_config(),
            tool_runs: Vec::new(),
        };

        // Write directly to bypass create() which sets correct version
        let path = store.session_path("ses_old");
        write_json(&path, &record).await.unwrap();

        let result = store.load("ses_old").await;
        assert!(matches!(
            result,
            Err(StorageError::SchemaVersionMismatch { .. })
        ));
    }
}
