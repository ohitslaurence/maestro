// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights
// reserved. SPDX-License-Identifier: Proprietary

/// Configuration for auto-commit behavior.
#[derive(Clone, Debug)]
pub struct AutoCommitConfig {
	/// Whether auto-commit is enabled.
	pub enabled: bool,
	/// Model to use for commit message generation.
	pub model: String,
	/// Maximum diff size in bytes before truncation.
	pub max_diff_bytes: usize,
	/// Tools that trigger auto-commit when successful.
	pub trigger_tools: Vec<String>,
}

impl Default for AutoCommitConfig {
	fn default() -> Self {
		Self {
			enabled: true,
			model: "claude-3-haiku-20240307".to_string(),
			max_diff_bytes: 32 * 1024,
			trigger_tools: vec!["edit_file".to_string(), "bash".to_string()],
		}
	}
}

impl AutoCommitConfig {
	pub fn disabled() -> Self {
		Self {
			enabled: false,
			..Default::default()
		}
	}

	pub fn with_model(mut self, model: impl Into<String>) -> Self {
		self.model = model.into();
		self
	}

	pub fn with_max_diff_bytes(mut self, bytes: usize) -> Self {
		self.max_diff_bytes = bytes;
		self
	}

	pub fn with_trigger_tools(mut self, tools: Vec<String>) -> Self {
		self.trigger_tools = tools;
		self
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use proptest::prelude::*;

	proptest! {
			/// Validates that disabled() always produces a config with enabled=false.
			/// This is important because the disabled() constructor is the way
			/// users explicitly opt-out of auto-commit behavior.
			#[test]
			fn disabled_constructor_always_sets_enabled_false(
					model in "[a-z]{1,20}",
					max_bytes in 1usize..1_000_000,
			) {
					let config = AutoCommitConfig::disabled()
							.with_model(model)
							.with_max_diff_bytes(max_bytes);

					prop_assert!(!config.enabled);
			}

			/// Validates that default() always produces a config with enabled=true.
			/// This ensures auto-commit is on by default for seamless workflow.
			#[test]
			fn default_constructor_always_sets_enabled_true(_unused in 0..1i32) {
					let config = AutoCommitConfig::default();
					prop_assert!(config.enabled);
			}

			/// Validates that builder methods correctly apply values.
			/// This ensures the fluent API works correctly for configuration.
			#[test]
			fn builder_methods_apply_values_correctly(
					model in "[a-z]{1,20}",
					max_bytes in 1usize..1_000_000,
					tool_count in 0usize..5,
			) {
					let tools: Vec<String> = (0..tool_count).map(|i| format!("tool_{i}")).collect();
					let config = AutoCommitConfig::default()
							.with_model(model.clone())
							.with_max_diff_bytes(max_bytes)
							.with_trigger_tools(tools.clone());

					prop_assert_eq!(config.model, model);
					prop_assert_eq!(config.max_diff_bytes, max_bytes);
					prop_assert_eq!(config.trigger_tools, tools);
			}
	}

	/// Test: Default config is enabled and has expected trigger tools.
	///
	/// Why this test is important: Verifies the default behavior is auto-commit
	/// enabled with edit_file and bash as triggers. This is the core contract
	/// for the opt-out design (enabled by default, disable via env var).
	#[test]
	fn default_config_has_correct_trigger_tools() {
		let config = AutoCommitConfig::default();
		assert!(config.trigger_tools.contains(&"edit_file".to_string()));
		assert!(config.trigger_tools.contains(&"bash".to_string()));
		assert_eq!(config.trigger_tools.len(), 2);
	}

	/// Test: Default config uses hardcoded Haiku model.
	///
	/// Why this test is important: The model is not user-configurable via CLI.
	/// This test ensures we don't accidentally change the default model.
	#[test]
	fn default_config_uses_haiku_model() {
		let config = AutoCommitConfig::default();
		assert_eq!(config.model, "claude-3-haiku-20240307");
	}

	/// Test: Max diff bytes has reasonable default.
	///
	/// Why this test is important: Ensures we have a sensible limit to avoid
	/// sending huge diffs to the LLM which would be slow and expensive.
	#[test]
	fn default_config_has_reasonable_max_diff_bytes() {
		let config = AutoCommitConfig::default();
		assert_eq!(config.max_diff_bytes, 32 * 1024); // 32KB
	}
}
