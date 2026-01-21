// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! Load and import docs index into SQLite FTS5.

use anyhow::{Context, Result};
use loom_server_db::{DocIndexEntry, DocsRepository};
use serde::Deserialize;
use tracing::{info, warn};

#[derive(Debug, Deserialize)]
pub struct ExportedDoc {
	pub doc_id: String,
	pub path: String,
	pub title: String,
	#[serde(default)]
	pub summary: String,
	pub diataxis: String,
	#[serde(default)]
	pub tags: Vec<String>,
	#[serde(default)]
	pub updated_at: String,
	pub body: String,
}

#[derive(Debug, Deserialize)]
pub struct ExportedDocsIndex {
	pub version: u32,
	pub generated_at: String,
	pub docs: Vec<ExportedDoc>,
}

/// Load docs index from JSON file and populate FTS5 table.
///
/// This clears the existing docs_fts table and repopulates it from the JSON.
/// Called at server startup.
pub async fn load_docs_index(repo: &DocsRepository, path: &str) -> Result<usize> {
	let bytes = match tokio::fs::read(path).await {
		Ok(b) => b,
		Err(e) => {
			warn!(
				"Could not read docs index at {}: {}. Docs search will be empty.",
				path, e
			);
			return Ok(0);
		}
	};

	let index: ExportedDocsIndex =
		serde_json::from_slice(&bytes).context("Failed to parse docs index JSON")?;

	info!(
		"Loading docs index: {} docs, generated at {}",
		index.docs.len(),
		index.generated_at
	);

	let entries: Vec<DocIndexEntry> = index
		.docs
		.iter()
		.map(|doc| DocIndexEntry {
			doc_id: doc.doc_id.clone(),
			path: doc.path.clone(),
			title: doc.title.clone(),
			summary: doc.summary.clone(),
			body: doc.body.clone(),
			diataxis: doc.diataxis.clone(),
			tags: doc.tags.join(" "),
			updated_at: doc.updated_at.clone(),
		})
		.collect();

	let count = entries.len();
	repo.insert_docs(&entries).await?;

	info!("Loaded {} docs into search index", count);
	Ok(count)
}
