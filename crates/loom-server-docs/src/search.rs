// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! Documentation search using SQLite FTS5.

use anyhow::Result;
use loom_server_db::{DocSearchHit, DocSearchParams, DocsRepository};

pub use loom_server_db::{DocSearchHit as SearchHit, DocSearchParams as SearchParams};

/// Search documentation using FTS5.
///
/// Returns ranked results with highlighted snippets.
pub async fn search_docs(
	repo: &DocsRepository,
	params: &DocSearchParams,
) -> Result<Vec<DocSearchHit>> {
	let hits = repo.search(params).await?;
	Ok(hits)
}
