//! ThreadStore: CRUD for thread metadata and snapshots (§2, §3).
//!
//! Stores thread records under `storage_root()/threads/<thread_id>.json`.

use std::path::PathBuf;

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
    pub fn path(&self) -> &PathBuf {
        &self.root
    }
}
