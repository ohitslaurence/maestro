//! MessageStore: append-only message log (§2, §3).
//!
//! Stores messages under `storage_root()/messages/<thread_id>/<message_id>.json`.

use super::{read_json, write_json, StorageError, StorageResult};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tokio::fs;

/// Current schema version for message records.
pub const MESSAGE_SCHEMA_VERSION: u32 = 1;

// ============================================================================
// Message Role (§3)
// ============================================================================

/// Role of a message sender.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageRole {
    User,
    Assistant,
    Tool,
    System,
}

// ============================================================================
// MessageRecord (§3)
// ============================================================================

/// A single message in the conversation log.
///
/// Append-only. Stored at `messages/<thread_id>/<message_id>.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MessageRecord {
    /// Schema version for migrations.
    pub schema_version: u32,
    /// Unique message identifier (e.g., "msg_001").
    pub id: String,
    /// ID of the thread this message belongs to.
    pub thread_id: String,
    /// ID of the session that produced this message.
    pub session_id: String,
    /// Role of the message sender.
    pub role: MessageRole,
    /// Message content (text or tool output).
    pub content: String,
    /// ISO 8601 timestamp when message was created.
    pub created_at: String,
    /// Tool call ID if this is a tool response message.
    pub tool_call_id: Option<String>,
}

// ============================================================================
// MessageStore (§2, §4)
// ============================================================================

/// MessageStore manages message record persistence.
///
/// Messages are stored append-only. Each message is written to its own file
/// under `messages/<thread_id>/<message_id>.json`.
pub struct MessageStore {
    root: PathBuf,
}

impl MessageStore {
    /// Create a new MessageStore rooted at the given path.
    pub fn new(root: PathBuf) -> Self {
        Self {
            root: root.join("messages"),
        }
    }

    /// Get the storage path for messages.
    #[allow(dead_code)]
    pub fn path(&self) -> &PathBuf {
        &self.root
    }

    /// Get the directory path for a thread's messages.
    fn thread_dir(&self, thread_id: &str) -> PathBuf {
        self.root.join(thread_id)
    }

    /// Get the path for a specific message record.
    fn message_path(&self, thread_id: &str, message_id: &str) -> PathBuf {
        self.thread_dir(thread_id).join(format!("{}.json", message_id))
    }

    /// Load a message by thread ID and message ID.
    #[allow(dead_code)]
    pub async fn load(&self, thread_id: &str, message_id: &str) -> StorageResult<MessageRecord> {
        let path = self.message_path(thread_id, message_id);
        let record: MessageRecord = read_json(&path).await?;

        // Validate schema version (§6)
        if record.schema_version != MESSAGE_SCHEMA_VERSION {
            return Err(StorageError::SchemaVersionMismatch {
                expected: MESSAGE_SCHEMA_VERSION,
                found: record.schema_version,
            });
        }

        Ok(record)
    }

    /// Append a message to the log (§4: append_message).
    ///
    /// Generates a new message ID if not provided and persists the record.
    /// This is append-only: existing messages should not be modified.
    pub async fn append(&self, mut record: MessageRecord) -> StorageResult<MessageRecord> {
        // Generate message ID if empty
        if record.id.is_empty() {
            record.id = format!("msg_{}", uuid::Uuid::new_v4());
        }

        // Ensure schema version is current
        record.schema_version = MESSAGE_SCHEMA_VERSION;

        // Set timestamp if not provided
        if record.created_at.is_empty() {
            record.created_at = chrono::Utc::now().to_rfc3339();
        }

        let path = self.message_path(&record.thread_id, &record.id);
        write_json(&path, &record).await?;

        Ok(record)
    }

    /// List all messages for a thread, ordered by createdAt ascending (§5).
    ///
    /// Preserves order when reloaded.
    pub async fn list_by_thread(&self, thread_id: &str) -> StorageResult<Vec<MessageRecord>> {
        let thread_dir = self.thread_dir(thread_id);

        // Ensure directory exists
        if !thread_dir.exists() {
            fs::create_dir_all(&thread_dir).await?;
            return Ok(Vec::new());
        }

        let mut messages = Vec::new();
        let mut entries = fs::read_dir(&thread_dir).await?;

        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if path.extension().map_or(false, |ext| ext == "json") {
                match read_json::<MessageRecord>(&path).await {
                    Ok(record) => {
                        if record.schema_version == MESSAGE_SCHEMA_VERSION {
                            messages.push(record);
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

        // Sort by createdAt ascending to preserve message order
        messages.sort_by(|a, b| a.created_at.cmp(&b.created_at));

        Ok(messages)
    }

    /// List messages for a specific session within a thread.
    #[allow(dead_code)]
    pub async fn list_by_session(
        &self,
        thread_id: &str,
        session_id: &str,
    ) -> StorageResult<Vec<MessageRecord>> {
        let all_messages = self.list_by_thread(thread_id).await?;
        Ok(all_messages
            .into_iter()
            .filter(|m| m.session_id == session_id)
            .collect())
    }

    /// Check if a message exists.
    #[allow(dead_code)]
    pub async fn exists(&self, thread_id: &str, message_id: &str) -> bool {
        self.message_path(thread_id, message_id).exists()
    }

    /// Delete all messages for a thread.
    ///
    /// Used when deleting a thread.
    #[allow(dead_code)]
    pub async fn delete_thread_messages(&self, thread_id: &str) -> StorageResult<()> {
        let thread_dir = self.thread_dir(thread_id);
        if thread_dir.exists() {
            fs::remove_dir_all(&thread_dir).await?;
        }
        Ok(())
    }

    /// Count messages in a thread.
    #[allow(dead_code)]
    pub async fn count(&self, thread_id: &str) -> StorageResult<usize> {
        let messages = self.list_by_thread(thread_id).await?;
        Ok(messages.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn make_test_message(thread_id: &str, session_id: &str, role: MessageRole) -> MessageRecord {
        MessageRecord {
            schema_version: MESSAGE_SCHEMA_VERSION,
            id: String::new(), // Will be generated
            thread_id: thread_id.to_string(),
            session_id: session_id.to_string(),
            role,
            content: "Test message content".to_string(),
            created_at: String::new(), // Will be generated
            tool_call_id: None,
        }
    }

    #[tokio::test]
    async fn test_append_and_load_message() {
        let dir = tempdir().unwrap();
        let store = MessageStore::new(dir.path().to_path_buf());

        let msg = make_test_message("thr_123", "ses_456", MessageRole::User);
        let appended = store.append(msg).await.unwrap();

        assert!(appended.id.starts_with("msg_"));
        assert_eq!(appended.thread_id, "thr_123");
        assert_eq!(appended.session_id, "ses_456");
        assert_eq!(appended.role, MessageRole::User);
        assert!(!appended.created_at.is_empty());

        let loaded = store.load("thr_123", &appended.id).await.unwrap();
        assert_eq!(loaded.id, appended.id);
        assert_eq!(loaded.content, appended.content);
    }

    #[tokio::test]
    async fn test_list_by_thread_preserves_order() {
        let dir = tempdir().unwrap();
        let store = MessageStore::new(dir.path().to_path_buf());

        // Append messages with explicit timestamps to test ordering
        let mut msg1 = make_test_message("thr_123", "ses_456", MessageRole::User);
        msg1.created_at = "2026-01-21T10:00:00Z".to_string();
        msg1.content = "First".to_string();

        let mut msg2 = make_test_message("thr_123", "ses_456", MessageRole::Assistant);
        msg2.created_at = "2026-01-21T10:01:00Z".to_string();
        msg2.content = "Second".to_string();

        let mut msg3 = make_test_message("thr_123", "ses_456", MessageRole::User);
        msg3.created_at = "2026-01-21T10:02:00Z".to_string();
        msg3.content = "Third".to_string();

        // Append in random order
        store.append(msg2).await.unwrap();
        store.append(msg1).await.unwrap();
        store.append(msg3).await.unwrap();

        let messages = store.list_by_thread("thr_123").await.unwrap();
        assert_eq!(messages.len(), 3);

        // Should be ordered by createdAt ascending
        assert_eq!(messages[0].content, "First");
        assert_eq!(messages[1].content, "Second");
        assert_eq!(messages[2].content, "Third");
    }

    #[tokio::test]
    async fn test_list_by_session() {
        let dir = tempdir().unwrap();
        let store = MessageStore::new(dir.path().to_path_buf());

        // Messages from two different sessions
        let msg1 = make_test_message("thr_123", "ses_1", MessageRole::User);
        let msg2 = make_test_message("thr_123", "ses_1", MessageRole::Assistant);
        let msg3 = make_test_message("thr_123", "ses_2", MessageRole::User);

        store.append(msg1).await.unwrap();
        store.append(msg2).await.unwrap();
        store.append(msg3).await.unwrap();

        let session_1_msgs = store.list_by_session("thr_123", "ses_1").await.unwrap();
        assert_eq!(session_1_msgs.len(), 2);

        let session_2_msgs = store.list_by_session("thr_123", "ses_2").await.unwrap();
        assert_eq!(session_2_msgs.len(), 1);
    }

    #[tokio::test]
    async fn test_delete_thread_messages() {
        let dir = tempdir().unwrap();
        let store = MessageStore::new(dir.path().to_path_buf());

        let msg1 = make_test_message("thr_123", "ses_456", MessageRole::User);
        let msg2 = make_test_message("thr_123", "ses_456", MessageRole::Assistant);

        store.append(msg1).await.unwrap();
        store.append(msg2).await.unwrap();

        assert_eq!(store.count("thr_123").await.unwrap(), 2);

        store.delete_thread_messages("thr_123").await.unwrap();

        assert_eq!(store.count("thr_123").await.unwrap(), 0);
    }

    #[tokio::test]
    async fn test_tool_message_with_call_id() {
        let dir = tempdir().unwrap();
        let store = MessageStore::new(dir.path().to_path_buf());

        let mut msg = make_test_message("thr_123", "ses_456", MessageRole::Tool);
        msg.tool_call_id = Some("call_abc123".to_string());
        msg.content = "Tool output".to_string();

        let appended = store.append(msg).await.unwrap();
        assert_eq!(appended.tool_call_id, Some("call_abc123".to_string()));

        let loaded = store.load("thr_123", &appended.id).await.unwrap();
        assert_eq!(loaded.tool_call_id, Some("call_abc123".to_string()));
    }

    #[tokio::test]
    async fn test_load_nonexistent_message_fails() {
        let dir = tempdir().unwrap();
        let store = MessageStore::new(dir.path().to_path_buf());

        let result = store.load("thr_123", "msg_nonexistent").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_schema_version_mismatch() {
        let dir = tempdir().unwrap();
        let store = MessageStore::new(dir.path().to_path_buf());

        // Create a message with wrong schema version
        let mut record = MessageRecord {
            schema_version: 999,
            id: "msg_old".to_string(),
            thread_id: "thr_123".to_string(),
            session_id: "ses_456".to_string(),
            role: MessageRole::User,
            content: "Old message".to_string(),
            created_at: "2026-01-21T10:00:00Z".to_string(),
            tool_call_id: None,
        };

        // Write directly to bypass append() which sets correct version
        let path = store.message_path("thr_123", "msg_old");
        write_json(&path, &record).await.unwrap();

        let result = store.load("thr_123", "msg_old").await;
        assert!(matches!(
            result,
            Err(StorageError::SchemaVersionMismatch { .. })
        ));
    }

    #[tokio::test]
    async fn test_empty_thread_returns_empty_list() {
        let dir = tempdir().unwrap();
        let store = MessageStore::new(dir.path().to_path_buf());

        let messages = store.list_by_thread("thr_empty").await.unwrap();
        assert!(messages.is_empty());
    }

    #[tokio::test]
    async fn test_message_exists() {
        let dir = tempdir().unwrap();
        let store = MessageStore::new(dir.path().to_path_buf());

        let msg = make_test_message("thr_123", "ses_456", MessageRole::User);
        let appended = store.append(msg).await.unwrap();

        assert!(store.exists("thr_123", &appended.id).await);
        assert!(!store.exists("thr_123", "msg_nonexistent").await);
    }
}
