//! MessageStore: append-only message log (§2, §3).
//!
//! Stores messages under `storage_root()/messages/<thread_id>/<message_id>.json`.

use std::path::PathBuf;

/// MessageStore manages message record persistence.
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
    pub fn path(&self) -> &PathBuf {
        &self.root
    }
}
