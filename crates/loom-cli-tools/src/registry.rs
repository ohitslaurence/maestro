// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights
// reserved. SPDX-License-Identifier: Proprietary

use async_trait::async_trait;
use loom_common_core::{ToolContext, ToolDefinition, ToolError};
use std::collections::HashMap;

#[async_trait]
pub trait Tool: Send + Sync {
	fn name(&self) -> &str;

	fn description(&self) -> &str;

	fn input_schema(&self) -> serde_json::Value;

	fn to_definition(&self) -> ToolDefinition {
		ToolDefinition {
			name: self.name().to_string(),
			description: self.description().to_string(),
			input_schema: self.input_schema(),
		}
	}

	async fn invoke(
		&self,
		args: serde_json::Value,
		ctx: &ToolContext,
	) -> Result<serde_json::Value, ToolError>;
}

pub struct ToolRegistry {
	tools: HashMap<String, Box<dyn Tool>>,
}

impl ToolRegistry {
	pub fn new() -> Self {
		Self {
			tools: HashMap::new(),
		}
	}

	pub fn register(&mut self, tool: Box<dyn Tool>) {
		let name = tool.name().to_string();
		tracing::debug!(tool_name = %name, "registering tool");
		self.tools.insert(name, tool);
	}

	pub fn get(&self, name: &str) -> Option<&dyn Tool> {
		self.tools.get(name).map(|t| t.as_ref())
	}

	pub fn definitions(&self) -> Vec<ToolDefinition> {
		self.tools.values().map(|t| t.to_definition()).collect()
	}
}

impl Default for ToolRegistry {
	fn default() -> Self {
		Self::new()
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use proptest::prelude::*;

	struct MockTool {
		name: String,
	}

	#[async_trait]
	impl Tool for MockTool {
		fn name(&self) -> &str {
			&self.name
		}

		fn description(&self) -> &str {
			"A mock tool for testing"
		}

		fn input_schema(&self) -> serde_json::Value {
			serde_json::json!({
					"type": "object",
					"properties": {}
			})
		}

		async fn invoke(
			&self,
			_args: serde_json::Value,
			_ctx: &ToolContext,
		) -> Result<serde_json::Value, ToolError> {
			Ok(serde_json::json!({"result": "ok"}))
		}
	}

	proptest! {
			/// Verifies that any tool registered with a valid name can be retrieved by that exact name.
			/// This property ensures the registry maintains consistent key-value semantics.
			#[test]
			fn registry_stores_and_retrieves_tools_by_name(name in "[a-zA-Z][a-zA-Z0-9_]{0,30}") {
					let mut registry = ToolRegistry::new();
					let tool = MockTool { name: name.clone() };
					registry.register(Box::new(tool));

					prop_assert!(registry.get(&name).is_some());
					prop_assert_eq!(registry.get(&name).unwrap().name(), name);
			}

			/// Verifies that definitions() returns exactly one definition per registered tool.
			/// This property ensures no tools are lost or duplicated when generating definitions.
			#[test]
			fn definitions_count_matches_registered_tools(
					names in prop::collection::hash_set("[a-zA-Z][a-zA-Z0-9_]{0,20}", 0..10)
			) {
					let mut registry = ToolRegistry::new();
					for name in &names {
							registry.register(Box::new(MockTool { name: name.clone() }));
					}

					prop_assert_eq!(registry.definitions().len(), names.len());
			}
	}

	#[test]
	fn get_returns_none_for_unregistered_tool() {
		let registry = ToolRegistry::new();
		assert!(registry.get("nonexistent").is_none());
	}
}
