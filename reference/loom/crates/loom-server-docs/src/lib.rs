// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights
// reserved. SPDX-License-Identifier: Proprietary

//! Documentation search service for Loom.
//!
//! Provides FTS5 full-text search over indexed documentation.

pub mod index;
pub mod search;

pub use index::{load_docs_index, ExportedDoc, ExportedDocsIndex};
pub use loom_server_db::{DocIndexEntry, DocSearchHit, DocSearchParams, DocsRepository};
pub use search::search_docs;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum DocsError {
	#[error("Database error: {0}")]
	Database(#[from] sqlx::Error),

	#[error("Index file not found: {0}")]
	IndexNotFound(String),

	#[error("Invalid index format: {0}")]
	InvalidIndex(String),
}

pub type Result<T> = std::result::Result<T, DocsError>;
