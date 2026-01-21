// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights
// reserved. SPDX-License-Identifier: Proprietary

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommitInfo {
	pub sha: String,
	pub message: String,
	pub author_name: String,
	pub author_email: String,
	pub author_date: DateTime<Utc>,
	pub committer_name: String,
	pub committer_email: String,
	pub committer_date: DateTime<Utc>,
	pub parent_shas: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TreeEntry {
	pub name: String,
	pub path: String,
	pub kind: TreeEntryKind,
	pub sha: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TreeEntryKind {
	File,
	Directory,
	Submodule,
	Symlink,
}
