// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights
// reserved. SPDX-License-Identifier: Proprietary

//! LLM output processor for detecting and handling server queries.
//!
//! This module processes LLM output to detect when a query is needed
//! and integrates with the ServerQueryManager for query lifecycle management.

use crate::server_query::ServerQueryManager;
use loom_common_core::server_query::{ServerQuery, ServerQueryError};
use std::sync::Arc;
use tracing::instrument;

/// Processes LLM output and initiates queries as needed.
#[derive(Clone)]
pub struct LlmQueryProcessor {
	/// Query manager for lifecycle management. Will be used in Phase 2.
	#[allow(dead_code)]
	query_manager: Arc<ServerQueryManager>,
}

impl LlmQueryProcessor {
	/// Create a new LLM query processor.
	///
	/// # Arguments
	/// * `query_manager` - The server query manager for handling queries
	pub fn new(query_manager: Arc<ServerQueryManager>) -> Self {
		Self { query_manager }
	}

	/// Process LLM output to check if a query is needed.
	///
	/// This function analyzes the LLM output and determines if a server query
	/// should be initiated. Currently returns None (Phase 2 logic will parse
	/// LLM output for query requests).
	///
	/// # Arguments
	/// * `session_id` - The session ID for logging context
	/// * `output` - The LLM output to analyze
	///
	/// # Returns
	/// Some(ServerQuery) if a query is detected, None otherwise
	/// Or ServerQueryError if processing fails
	///
	/// # Phase 2 Implementation Notes
	/// Future versions will implement:
	/// - Pattern matching for query indicators in LLM output
	/// - Query kind detection (ReadFile, WriteFile, Execute, etc.)
	/// - Extraction of query parameters from output
	/// - Timeout and metadata configuration
	#[instrument(skip(self, output), fields(session_id = %session_id))]
	pub async fn process_llm_output(
		&self,
		session_id: &str,
		output: &str,
	) -> Result<Option<ServerQuery>, ServerQueryError> {
		tracing::debug!(
				session_id = %session_id,
				output_len = output.len(),
				"processing LLM output for queries"
		);

		// Phase 2: Parse LLM output for query requests
		// For now: return None (no query detected)
		Ok(None)
	}
}

impl Default for LlmQueryProcessor {
	fn default() -> Self {
		Self::new(Arc::new(ServerQueryManager::new()))
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	/// Test that LlmQueryProcessor can be instantiated with a manager.
	/// This verifies the basic constructor and state initialization.
	#[test]
	fn test_llm_query_processor_new() {
		let manager = Arc::new(ServerQueryManager::new());
		let processor = LlmQueryProcessor::new(manager.clone());
		// Verify processor was created (no panics)
		drop(processor);
	}

	/// Test that default instance can be created.
	/// This verifies that the Default impl works correctly.
	#[test]
	fn test_llm_query_processor_default() {
		let processor = LlmQueryProcessor::default();
		// Verify processor was created (no panics)
		drop(processor);
	}

	/// Test that process_llm_output returns None for arbitrary output.
	/// This verifies Phase 1 behavior where no queries are detected.
	/// Purpose: Ensure the placeholder implementation works correctly
	/// and is ready for Phase 2 query detection logic.
	#[tokio::test]
	async fn test_process_llm_output_returns_none() {
		let processor = LlmQueryProcessor::default();
		let result = processor
			.process_llm_output("session-1", "Hello, this is LLM output")
			.await;

		assert!(result.is_ok());
		assert_eq!(result.unwrap(), None);
	}

	/// Test that process_llm_output handles empty output.
	/// This verifies the function works correctly with edge case inputs.
	#[tokio::test]
	async fn test_process_llm_output_empty_string() {
		let processor = LlmQueryProcessor::default();
		let result = processor.process_llm_output("session-1", "").await;

		assert!(result.is_ok());
		assert_eq!(result.unwrap(), None);
	}

	/// Test that process_llm_output handles long output.
	/// This verifies the function doesn't panic or error on large inputs.
	#[tokio::test]
	async fn test_process_llm_output_long_string() {
		let processor = LlmQueryProcessor::default();
		let long_output = "x".repeat(10000);
		let result = processor
			.process_llm_output("session-1", &long_output)
			.await;

		assert!(result.is_ok());
		assert_eq!(result.unwrap(), None);
	}

	/// Test processor can be cloned.
	/// This verifies thread-safe sharing of the processor across async tasks.
	#[test]
	fn test_llm_query_processor_clone() {
		let processor = LlmQueryProcessor::default();
		let _cloned = processor.clone();
		// Verify clone succeeds (no panics)
	}
}
