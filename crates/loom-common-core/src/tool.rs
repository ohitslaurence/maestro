// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights
// reserved. SPDX-License-Identifier: Proprietary

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Definition of a tool for the LLM.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ToolDefinition {
	pub name: String,
	pub description: String,
	pub input_schema: serde_json::Value,
}

impl ToolDefinition {
	pub fn new(
		name: impl Into<String>,
		description: impl Into<String>,
		input_schema: serde_json::Value,
	) -> Self {
		let name = name.into();
		tracing::debug!(
				tool_name = %name,
				"Creating tool definition"
		);
		Self {
			name,
			description: description.into(),
			input_schema,
		}
	}
}

/// Context provided to tools during execution.
#[derive(Clone, Debug)]
pub struct ToolContext {
	pub workspace_root: PathBuf,
}

impl ToolContext {
	pub fn new(workspace_root: impl Into<PathBuf>) -> Self {
		let workspace_root = workspace_root.into();
		tracing::debug!(
				workspace_root = %workspace_root.display(),
				"Creating tool context"
		);
		Self { workspace_root }
	}
}
