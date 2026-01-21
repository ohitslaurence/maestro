//! SessionStore: CRUD for session runtime records (§2, §3).
//!
//! Stores session records under `storage_root()/sessions/<session_id>.json`.

use std::path::PathBuf;

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
    pub fn path(&self) -> &PathBuf {
        &self.root
    }
}
