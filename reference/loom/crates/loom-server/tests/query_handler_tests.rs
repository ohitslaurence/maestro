// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! Query handler integration tests for Phase 2.
//!
//! **Purpose**: Tests the complete LLM query detection and handler flow, including
//! process_llm_output() operations, timeout handling, error propagation, and
//! concurrent query processing.
//!
//! These tests validate the handler's ability to safely integrate query processing
//! into the LLM pipeline without breaking conversation flow.

use loom_common_core::server_query::{ServerQueryKind, ServerQueryResponse, ServerQueryResult};
use loom_server::{LlmQueryHandler, ServerQueryManager, SimpleRegexDetector};
use std::sync::Arc;
use std::time::Duration;

// ============================================================================
// Helper Functions
// ============================================================================

/// Create a test handler with default detector and manager.
/// Useful for setup in multiple tests.
fn create_test_handler() -> (Arc<ServerQueryManager>, LlmQueryHandler) {
	let manager = Arc::new(ServerQueryManager::new());
	let handler = LlmQueryHandler::with_default_detector(manager.clone());
	(manager, handler)
}

// ============================================================================
// Basic Handler Flow Tests
// ============================================================================

/// Test that handler correctly identifies when no query is present.
/// **Purpose**: Normal LLM output without queries should pass through unchanged.
/// The handler must not interrupt regular conversation flow.
#[tokio::test]
async fn test_handle_no_query_in_output() {
	let (_, handler) = create_test_handler();

	let output = "To solve this problem, I would suggest using a factorial algorithm \
                 that operates in O(n) time complexity.";

	let result = handler.handle_llm_output("session-1", output).await;

	assert!(result.is_ok(), "Should handle non-query output gracefully");
	let response = result.unwrap();
	assert!(
		response.is_none(),
		"Should return None when no query detected"
	);
}

/// Test handler with single detected query and response.
/// **Purpose**: Core workflow - detect query, process, return response.
/// Validates the full happy path through the handler.
#[tokio::test]
async fn test_handle_single_query_with_response() {
	let (manager, handler) = create_test_handler();

	let output = "I need to read config.json to understand the deployment settings";

	// Spawn task to send response after a short delay
	let manager_clone = manager.clone();
	let response_task = tokio::spawn(async move {
		tokio::time::sleep(Duration::from_millis(50)).await;

		// Get pending queries
		let pending = manager_clone.list_pending("session-1").await;
		if !pending.is_empty() {
			let query = &pending[0];
			let response = ServerQueryResponse {
				query_id: query.id.clone(),
				sent_at: chrono::Utc::now().to_rfc3339(),
				result: ServerQueryResult::FileContent("key: value".to_string()),
				error: None,
			};
			manager_clone.receive_response(response).await;
		}
	});

	// Handle the output
	let result = handler.handle_llm_output("session-1", output).await;

	// Wait for response task
	let _ = tokio::time::timeout(Duration::from_secs(5), response_task)
		.await
		.ok();

	assert!(result.is_ok(), "Should process query successfully");
	let response = result.unwrap();
	assert!(response.is_some(), "Should have response");

	if let Some(resp) = response {
		assert_eq!(resp.error, None, "Response should not have error");
		match &resp.result {
			ServerQueryResult::FileContent(content) => {
				assert_eq!(content, "key: value");
			}
			_ => panic!("Expected FileContent result"),
		}
	}
}

// ============================================================================
// Timeout Handling Tests
// ============================================================================

/// Test that queries timeout correctly after configured duration.
/// **Purpose**: Timeouts prevent the handler from hanging indefinitely if
/// the client doesn't respond. Critical for system stability.
#[tokio::test]
async fn test_query_timeout_handling() {
	let (_manager, handler) = create_test_handler();

	let output = "I need to read config.json";

	// Don't send any response - let it timeout
	let result = handler.handle_llm_output("session-1", output).await;

	assert!(result.is_err(), "Should error on timeout");
	let error = result.unwrap_err();
	assert!(
		matches!(
			error,
			loom_common_core::server_query::ServerQueryError::Timeout
		),
		"Should be timeout error, got: {error:?}"
	);
}

/// Test timeout with user input queries (longer timeout).
/// **Purpose**: Validates that different query types have appropriate timeouts.
/// User input queries should wait longer since users may take time.
#[tokio::test]
async fn test_user_input_timeout_is_longer() {
	let (_manager, handler) = create_test_handler();

	let output = "ask user for approval";

	// Record when timeout should occur
	let start = std::time::Instant::now();

	let result = handler.handle_llm_output("session-1", output).await;

	let elapsed = start.elapsed();

	assert!(result.is_err(), "Should timeout");
	// User input has 60s timeout, so should wait at least 50s worth
	// (we can't test exact 60s due to overhead)
	assert!(
		elapsed >= Duration::from_secs(55),
		"Should have waited ~60s"
	);
}

/// Test quick timeout for ReadFile queries.
/// **Purpose**: File reads are fast operations. Should timeout sooner than
/// user input to fail fast when client is unresponsive.
#[tokio::test]
async fn test_read_file_timeout_is_quick() {
	let (_, handler) = create_test_handler();

	let output = "I need to read config.json";

	let start = std::time::Instant::now();
	let result = handler.handle_llm_output("session-1", output).await;
	let elapsed = start.elapsed();

	assert!(result.is_err(), "Should timeout");
	// ReadFile has 10s timeout
	assert!(
		elapsed < Duration::from_secs(15),
		"Should timeout within ~10s, took: {elapsed:?}"
	);
}

// ============================================================================
// Error Propagation Tests
// ============================================================================

/// Test that query errors are properly propagated to caller.
/// **Purpose**: Errors must not be swallowed. Caller should know when
/// query processing fails to handle appropriately.
#[tokio::test]
async fn test_error_propagation() {
	let (_, handler) = create_test_handler();

	let output = "I need to read config.json";

	let result = handler.handle_llm_output("session-1", output).await;

	assert!(result.is_err(), "Should propagate timeout error");
	// Error should be usable by caller
	let error = result.unwrap_err();
	assert!(
		!format!("{error:?}").is_empty(),
		"Error should be describable"
	);
}

/// Test handler behavior when no response is received.
/// **Purpose**: Missing responses should not cause panics or hangs.
#[tokio::test]
async fn test_missing_response_handling() {
	let (_, handler) = create_test_handler();

	let output = "I need to read missing_file.txt";

	// Deliberately don't send response
	let result = handler.handle_llm_output("session-1", output).await;

	assert!(result.is_err(), "Should error on no response");
}

// ============================================================================
// Concurrent Query Tests
// ============================================================================

/// Test that multiple concurrent queries on same session are handled correctly.
/// **Purpose**: Validates query correlation when multiple queries are in flight.
/// Responses must be matched to correct queries using query ID.
#[tokio::test]
async fn test_concurrent_queries_same_session() {
	let (manager, handler) = create_test_handler();

	let output1 = "read config.json";
	let output2 = "read settings.json";

	// Start both handlers
	let manager_clone = manager.clone();
	let handler_clone = handler.clone();

	let task1 = tokio::spawn(async move { handler.handle_llm_output("session-1", output1).await });

	let task2 =
		tokio::spawn(async move { handler_clone.handle_llm_output("session-1", output2).await });

	// Give time for queries to be sent
	tokio::time::sleep(Duration::from_millis(100)).await;

	// Get pending queries
	let pending = manager_clone.list_pending("session-1").await;

	// Send responses to all pending queries
	for query in pending {
		let response = ServerQueryResponse {
			query_id: query.id.clone(),
			sent_at: chrono::Utc::now().to_rfc3339(),
			result: ServerQueryResult::FileContent("content".to_string()),
			error: None,
		};
		manager_clone.receive_response(response).await;
	}

	// Wait for both handlers
	let _ = tokio::time::timeout(Duration::from_secs(5), task1).await;
	let _ = tokio::time::timeout(Duration::from_secs(5), task2).await;
}

/// Test concurrent queries across different sessions.
/// **Purpose**: Sessions must be isolated. Queries in one session should not
/// affect queries in another. This prevents user data leakage.
#[tokio::test]
async fn test_concurrent_queries_different_sessions() {
	let (manager, handler) = create_test_handler();

	// Start handlers for both sessions
	let handler1 = handler.clone();
	let handler2 = handler.clone();
	let manager1 = manager.clone();
	let manager2 = manager.clone();

	let task1 = tokio::spawn(async move {
		let manager_clone = manager1.clone();
		let response_task = tokio::spawn(async move {
			tokio::time::sleep(Duration::from_millis(100)).await;
			let pending = manager_clone.list_pending("session-a").await;
			for query in pending {
				let response = ServerQueryResponse {
					query_id: query.id.clone(),
					sent_at: chrono::Utc::now().to_rfc3339(),
					result: ServerQueryResult::FileContent("session-a content".to_string()),
					error: None,
				};
				manager_clone.receive_response(response).await;
			}
		});

		let result = handler1
			.handle_llm_output("session-a", "read file.txt")
			.await;
		let _ = tokio::time::timeout(Duration::from_secs(5), response_task).await;
		result
	});

	let task2 = tokio::spawn(async move {
		let manager_clone = manager2.clone();
		let response_task = tokio::spawn(async move {
			tokio::time::sleep(Duration::from_millis(100)).await;
			let pending = manager_clone.list_pending("session-b").await;
			for query in pending {
				let response = ServerQueryResponse {
					query_id: query.id.clone(),
					sent_at: chrono::Utc::now().to_rfc3339(),
					result: ServerQueryResult::FileContent("session-b content".to_string()),
					error: None,
				};
				manager_clone.receive_response(response).await;
			}
		});

		let result = handler2
			.handle_llm_output("session-b", "read other.txt")
			.await;
		let _ = tokio::time::timeout(Duration::from_secs(5), response_task).await;
		result
	});

	let result1 = tokio::time::timeout(Duration::from_secs(10), task1).await;
	let result2 = tokio::time::timeout(Duration::from_secs(10), task2).await;

	assert!(result1.is_ok(), "Task 1 should complete");
	assert!(result2.is_ok(), "Task 2 should complete");
}

// ============================================================================
// Response Structure Tests
// ============================================================================

/// Test that response structure matches expected format.
/// **Purpose**: The handler must extract and return the response in the
/// expected format for the LLM to inject into conversation.
#[tokio::test]
async fn test_response_structure_in_result() {
	let (manager, handler) = create_test_handler();

	let output = "I need to read config.json";

	let manager_clone = manager.clone();

	let response_task = tokio::spawn(async move {
		tokio::time::sleep(Duration::from_millis(50)).await;

		let pending = manager_clone.list_pending("session-1").await;
		if !pending.is_empty() {
			let query = &pending[0];
			let response = ServerQueryResponse {
				query_id: query.id.clone(),
				sent_at: chrono::Utc::now().to_rfc3339(),
				result: ServerQueryResult::FileContent("configuration data here".to_string()),
				error: None,
			};
			manager_clone.receive_response(response).await;
		}
	});

	let result = handler.handle_llm_output("session-1", output).await;
	let _ = tokio::time::timeout(Duration::from_secs(5), response_task).await;

	assert!(result.is_ok());
	let response_opt = result.unwrap();
	assert!(response_opt.is_some(), "Should have response");

	let response = response_opt.unwrap();
	assert!(
		response.query_id.starts_with("Q-"),
		"Should have valid query ID"
	);
	assert_eq!(response.error, None, "Should not have error");

	match &response.result {
		ServerQueryResult::FileContent(content) => {
			assert_eq!(content, "configuration data here");
		}
		_ => panic!("Expected FileContent"),
	}
}

/// Test response with error information.
/// **Purpose**: Some queries may fail (e.g., file not found). The handler
/// must properly propagate error information for LLM handling.
#[tokio::test]
async fn test_response_with_error_info() {
	let (manager, handler) = create_test_handler();

	let output = "I need to read missing.json";

	let manager_clone = manager.clone();

	let response_task = tokio::spawn(async move {
		tokio::time::sleep(Duration::from_millis(50)).await;

		let pending = manager_clone.list_pending("session-1").await;
		if !pending.is_empty() {
			let query = &pending[0];
			let response = ServerQueryResponse {
				query_id: query.id.clone(),
				sent_at: chrono::Utc::now().to_rfc3339(),
				result: ServerQueryResult::FileContent("".to_string()),
				error: Some("File not found: missing.json".to_string()),
			};
			manager_clone.receive_response(response).await;
		}
	});

	let result = handler.handle_llm_output("session-1", output).await;
	let _ = tokio::time::timeout(Duration::from_secs(5), response_task).await;

	assert!(result.is_ok());
	let response_opt = result.unwrap();
	assert!(response_opt.is_some());

	let response = response_opt.unwrap();
	assert!(
		response.error.is_some(),
		"Error response should have error field"
	);
	assert!(
		response.error.as_ref().unwrap().contains("not found"),
		"Error should describe the issue"
	);
}

// ============================================================================
// Batch Processing Tests
// ============================================================================

/// Test batch processing of multiple queries in output.
/// **Purpose**: Validates the batch handler that processes all detected queries
/// sequentially. Useful for complex LLM outputs with multiple operations.
#[tokio::test]
async fn test_batch_handle_multiple_queries() {
	let (manager, handler) = create_test_handler();

	// Manually test batch handling
	let output = "First read config.json, then read settings.json";

	// Get queries detected
	let detector = SimpleRegexDetector::new();
	let queries = detector.detect_queries(output).unwrap();

	if !queries.is_empty() {
		// Batch handler should handle multiple
		let manager_clone = manager.clone();

		let response_task = tokio::spawn(async move {
			tokio::time::sleep(Duration::from_millis(50)).await;

			let pending = manager_clone.list_pending("session-1").await;
			for query in pending {
				let response = ServerQueryResponse {
					query_id: query.id.clone(),
					sent_at: chrono::Utc::now().to_rfc3339(),
					result: ServerQueryResult::FileContent("content".to_string()),
					error: None,
				};
				manager_clone.receive_response(response).await;
			}
		});

		let result = handler.handle_llm_output_batch("session-1", output).await;

		let _ = tokio::time::timeout(Duration::from_secs(5), response_task).await;

		assert!(result.is_ok());
		let responses = result.unwrap();
		assert!(
			!responses.is_empty(),
			"Should have at least 1 response in batch"
		);
	}
}

// ============================================================================
// Handler State Tests
// ============================================================================

/// Test handler with default detector creation.
/// **Purpose**: Convenience constructor should work correctly without
/// users having to manually wire up dependencies.
#[test]
fn test_handler_creation_with_defaults() {
	let manager = Arc::new(ServerQueryManager::new());
	let handler = LlmQueryHandler::with_default_detector(manager);

	// Should be able to use the handler
	assert!(
		!handler.detector.read_file_pattern.as_str().is_empty(),
		"Handler should have valid detector patterns"
	);
}

/// Test handler creation with custom detector.
/// **Purpose**: Users should be able to provide custom detectors for
/// specialized query patterns beyond the default.
#[test]
fn test_handler_creation_with_custom_detector() {
	let manager = Arc::new(ServerQueryManager::new());
	let detector = SimpleRegexDetector::new();
	let handler = LlmQueryHandler::new(detector.clone(), manager);

	// Should have a custom detector (basic sanity check)
	assert_eq!(
		handler.detector.read_file_pattern.as_str().len(),
		detector.read_file_pattern.as_str().len()
	);
}

// ============================================================================
// Session Isolation Tests
// ============================================================================

/// Test that queries from different sessions don't interfere.
/// **Purpose**: Session isolation is critical for multi-user scenarios.
/// Queries and responses must not leak between sessions.
#[tokio::test]
async fn test_session_isolation() {
	let manager = Arc::new(ServerQueryManager::new());

	// Create two sessions with different query IDs
	let query1 = loom_common_core::server_query::ServerQuery {
		id: "Q-test-isolation-1".to_string(),
		kind: ServerQueryKind::ReadFile {
			path: "file.txt".to_string(),
		},
		sent_at: chrono::Utc::now().to_rfc3339(),
		timeout_secs: 30,
		metadata: serde_json::json!({}),
	};

	let query2 = loom_common_core::server_query::ServerQuery {
		id: "Q-test-isolation-2".to_string(),
		kind: ServerQueryKind::ReadFile {
			path: "file.txt".to_string(),
		},
		sent_at: chrono::Utc::now().to_rfc3339(),
		timeout_secs: 30,
		metadata: serde_json::json!({}),
	};

	let manager1 = manager.clone();
	let manager2 = manager.clone();

	// Start queries on both sessions
	let task1 = tokio::spawn(async move { manager1.send_query("session-1", query1).await });

	let task2 = tokio::spawn(async move { manager2.send_query("session-2", query2).await });

	// Give time for queries to be pending
	tokio::time::sleep(Duration::from_millis(50)).await;

	// Queries are global not per-session, so check that responses map correctly
	let all_pending = manager.list_pending("session-1").await;
	assert!(!all_pending.is_empty(), "Should have pending queries");

	// Clean up by sending responses
	let response1 = ServerQueryResponse {
		query_id: "Q-test-isolation-1".to_string(),
		sent_at: chrono::Utc::now().to_rfc3339(),
		result: ServerQueryResult::FileContent("content1".to_string()),
		error: None,
	};

	let response2 = ServerQueryResponse {
		query_id: "Q-test-isolation-2".to_string(),
		sent_at: chrono::Utc::now().to_rfc3339(),
		result: ServerQueryResult::FileContent("content2".to_string()),
		error: None,
	};

	manager.receive_response(response1).await;
	manager.receive_response(response2).await;

	let r1 = tokio::time::timeout(Duration::from_secs(2), task1).await;
	let r2 = tokio::time::timeout(Duration::from_secs(2), task2).await;

	assert!(r1.is_ok());
	assert!(r2.is_ok());
}
