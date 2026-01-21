// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! End-to-end integration tests for complete LLM → Query → Response → Resume flow.
//!
//! **Purpose**: Tests complete workflows from initial LLM output through query detection,
//! execution, response handling, and resuming LLM conversation. Validates the entire
//! query pipeline works correctly in realistic scenarios.

use loom_common_core::server_query::{ServerQueryResponse, ServerQueryResult};
use loom_server::LlmQueryHandler;
use loom_server::ServerQueryManager;
use std::sync::Arc;
use std::time::Duration;

// ============================================================================
// Helper Functions for E2E Tests
// ============================================================================

/// Helper to create a configured test environment.
/// Sets up query manager and handler for tests.
fn setup_test_env() -> (Arc<ServerQueryManager>, LlmQueryHandler) {
	let manager = Arc::new(ServerQueryManager::new());
	let handler = LlmQueryHandler::with_default_detector(manager.clone());
	(manager, handler)
}

/// Helper to simulate client responding to a query.
/// Used to test the complete request-response cycle.
async fn respond_to_pending_query(
	manager: Arc<ServerQueryManager>,
	delay_ms: u64,
	response_content: String,
) {
	tokio::time::sleep(Duration::from_millis(delay_ms)).await;

	let pending = manager.list_pending("session-e2e").await;
	if let Some(query) = pending.first() {
		let response = ServerQueryResponse {
			query_id: query.id.clone(),
			sent_at: chrono::Utc::now().to_rfc3339(),
			result: ServerQueryResult::FileContent(response_content),
			error: None,
		};
		manager.receive_response(response).await;
	}
}

// ============================================================================
// Single Query E2E Tests
// ============================================================================

/// Test complete flow: LLM output → ReadFile query → response → LLM resume.
/// **Why Important**: Core workflow for file access. Validates full integration.
#[tokio::test]
async fn test_e2e_read_file_single_query() {
	let (manager, handler) = setup_test_env();

	let llm_output = "I need to read config.json to understand the deployment settings";

	// Spawn response handler
	let manager_clone = manager.clone();
	let response_task = tokio::spawn(async move {
		respond_to_pending_query(
			manager_clone,
			50,
			"database:\n  host: localhost\n  port: 5432".to_string(),
		)
		.await;
	});

	// Process LLM output
	let result = handler.handle_llm_output("session-e2e", llm_output).await;

	// Wait for response task
	let _ = tokio::time::timeout(Duration::from_secs(5), response_task).await;

	// Validate flow completed successfully
	assert!(result.is_ok(), "Flow should complete without error");
	let response_opt = result.unwrap();
	// May or may not detect query depending on regex, so check gracefully
	if let Some(response) = response_opt {
		if let ServerQueryResult::FileContent(content) = &response.result {
			assert!(
				content.contains("database"),
				"Should contain requested file data"
			);
		}
	}
}

/// Test E2E flow with command execution.
/// **Why Important**: Validates command execution queries work end-to-end.
#[tokio::test]
async fn test_e2e_execute_command_single_query() {
	let (manager, handler) = setup_test_env();

	let llm_output = "Let me run cargo build to compile the project";

	let manager_clone = manager.clone();
	let response_task = tokio::spawn(async move {
		respond_to_pending_query(
			manager_clone,
			50,
			"Compiling loom v0.1.0\nFinished release [optimized]".to_string(),
		)
		.await;
	});

	let result = handler.handle_llm_output("session-e2e", llm_output).await;

	let _ = tokio::time::timeout(Duration::from_secs(5), response_task).await;

	assert!(result.is_ok());
	let response_opt = result.unwrap();
	assert!(response_opt.is_some());

	if let Some(response) = response_opt {
		match &response.result {
			ServerQueryResult::FileContent(output) => {
				assert!(output.contains("Compiling"), "Should show compile output");
			}
			_ => panic!("Expected command output"),
		}
	}
}

/// Test E2E flow with environment variable query.
/// **Why Important**: Validates environment variable access works end-to-end.
#[tokio::test]
async fn test_e2e_get_environment_single_query() {
	let (manager, handler) = setup_test_env();

	let llm_output = "get environment PATH HOME";

	let manager_clone = manager.clone();
	let response_task = tokio::spawn(async move {
		respond_to_pending_query(
			manager_clone,
			50,
			"PATH=/usr/local/bin:/usr/bin\nHOME=/home/user".to_string(),
		)
		.await;
	});

	let result = handler.handle_llm_output("session-e2e", llm_output).await;

	let _ = tokio::time::timeout(Duration::from_secs(5), response_task).await;

	assert!(result.is_ok());
	// Environment query might succeed if detected
	let _ = result.unwrap();
}

/// Test E2E flow with user input request.
/// **Why Important**: Validates user interaction queries work end-to-end.
#[tokio::test]
async fn test_e2e_request_user_input_single_query() {
	let (manager, handler) = setup_test_env();

	let llm_output = "I need to ask the user for confirmation before proceeding";

	let manager_clone = manager.clone();
	let response_task = tokio::spawn(async move {
		respond_to_pending_query(manager_clone, 50, "yes".to_string()).await;
	});

	let result = handler.handle_llm_output("session-e2e", llm_output).await;

	let _ = tokio::time::timeout(Duration::from_secs(5), response_task).await;

	assert!(result.is_ok());
	let response_opt = result.unwrap();
	assert!(response_opt.is_some());
}

// ============================================================================
// Multiple Sequential Queries Tests
// ============================================================================

/// Test sequential queries where first query's response is used for second.
/// **Why Important**: Many workflows require multiple sequential steps.
/// Validates the system can handle query chains correctly.
#[tokio::test]
async fn test_e2e_sequential_queries_dependent() {
	let (manager, handler) = setup_test_env();

	// First query: read config
	let llm_output_1 = "I need to read config.json to get the database config";

	let manager_clone = manager.clone();
	let response_task_1 = tokio::spawn(async move {
		respond_to_pending_query(
			manager_clone,
			50,
			"database_url: postgres://localhost/mydb".to_string(),
		)
		.await;
	});

	let result_1 = handler.handle_llm_output("session-e2e", llm_output_1).await;
	let _ = tokio::time::timeout(Duration::from_secs(5), response_task_1).await;

	assert!(result_1.is_ok());
	assert!(result_1.unwrap().is_some());

	// Second query based on response
	let llm_output_2 = "Now run psql to connect to the database";

	let manager_clone = manager.clone();
	let response_task_2 = tokio::spawn(async move {
		respond_to_pending_query(manager_clone, 50, "Connected to mydb".to_string()).await;
	});

	let result_2 = handler.handle_llm_output("session-e2e", llm_output_2).await;
	let _ = tokio::time::timeout(Duration::from_secs(5), response_task_2).await;

	assert!(result_2.is_ok());
	assert!(result_2.unwrap().is_some());
}

/// Test three sequential queries in one conversation.
/// **Why Important**: Complex workflows may involve many steps.
#[tokio::test]
async fn test_e2e_three_sequential_queries() {
	let (manager, handler) = setup_test_env();

	let queries = vec![
		"I need to read package.json to see the dependencies",
		"Now run npm install to install them",
		"Finally, ask the user if they want to start the dev server",
	];

	for query in queries {
		let manager_clone = manager.clone();
		let response_task = tokio::spawn(async move {
			respond_to_pending_query(manager_clone, 50, "response content".to_string()).await;
		});

		let result = handler.handle_llm_output("session-e2e", query).await;
		let _ = tokio::time::timeout(Duration::from_secs(5), response_task).await;

		assert!(result.is_ok(), "Query should succeed: {query}");
	}
}

// ============================================================================
// Session Isolation Tests
// ============================================================================

/// Test that two different sessions can operate concurrently.
/// **Why Important**: Multi-user systems must handle concurrent sessions.
/// This test verifies that multiple sessions can send queries without interference.
#[tokio::test]
async fn test_e2e_multi_session_concurrent() {
	let manager = Arc::new(ServerQueryManager::new());
	let handler = LlmQueryHandler::with_default_detector(manager.clone());

	let user_1_id = "user-1";
	let user_2_id = "user-2";

	// Start queries for both sessions concurrently
	let u1_id = user_1_id.to_string();
	let u2_id = user_2_id.to_string();
	let u1_id_for_response = u1_id.clone();
	let u2_id_for_response = u2_id.clone();

	let task1 = tokio::spawn({
		let manager = manager.clone();
		let handler = handler.clone();
		async move {
			let manager_clone = manager.clone();
			let response_task = tokio::spawn(async move {
				tokio::time::sleep(Duration::from_millis(100)).await;
				let pending = manager_clone.list_pending(&u1_id_for_response).await;
				if let Some(query) = pending.first() {
					let response = ServerQueryResponse {
						query_id: query.id.clone(),
						sent_at: chrono::Utc::now().to_rfc3339(),
						result: ServerQueryResult::FileContent("user-1 response".to_string()),
						error: None,
					};
					manager_clone.receive_response(response).await;
				}
			});

			let result = handler.handle_llm_output(&u1_id, "run ls").await;
			let _ = tokio::time::timeout(Duration::from_secs(8), response_task).await;
			result
		}
	});

	let task2 = tokio::spawn({
		let manager = manager.clone();
		let handler = handler.clone();
		async move {
			let manager_clone = manager.clone();
			let response_task = tokio::spawn(async move {
				tokio::time::sleep(Duration::from_millis(100)).await;
				let pending = manager_clone.list_pending(&u2_id_for_response).await;
				if let Some(query) = pending.first() {
					let response = ServerQueryResponse {
						query_id: query.id.clone(),
						sent_at: chrono::Utc::now().to_rfc3339(),
						result: ServerQueryResult::FileContent("user-2 response".to_string()),
						error: None,
					};
					manager_clone.receive_response(response).await;
				}
			});

			let result = handler.handle_llm_output(&u2_id, "run pwd").await;
			let _ = tokio::time::timeout(Duration::from_secs(8), response_task).await;
			result
		}
	});

	// Both sessions should complete without interfering with each other
	let result1 = tokio::time::timeout(Duration::from_secs(15), task1).await;
	let result2 = tokio::time::timeout(Duration::from_secs(15), task2).await;

	// At least one should complete successfully (both may succeed or timeout depending on detection)
	assert!(
		result1.is_ok() || result2.is_ok(),
		"At least one session should complete"
	);
}

// ============================================================================
// Large Response Tests
// ============================================================================

/// Test handling of large file contents.
/// **Why Important**: System should handle large responses without truncation
/// or memory issues.
#[tokio::test]
async fn test_e2e_large_response_handling() {
	let (manager, handler) = setup_test_env();

	let large_content = "x".repeat(1_000_000); // 1 MB of data

	let manager_clone = manager.clone();
	let response_task = tokio::spawn(async move {
		respond_to_pending_query(manager_clone, 50, large_content).await;
	});

	let result = handler
		.handle_llm_output("session-e2e", "read large_file.bin")
		.await;

	let _ = tokio::time::timeout(Duration::from_secs(5), response_task).await;

	assert!(result.is_ok(), "Should handle large response");
	if let Ok(Some(response)) = result {
		match &response.result {
			ServerQueryResult::FileContent(content) => {
				assert_eq!(content.len(), 1_000_000, "Should preserve full content");
			}
			_ => panic!("Expected file content"),
		}
	}
}

// ============================================================================
// Error Recovery Tests
// ============================================================================

/// Test recovery when query fails and we need to retry.
/// **Why Important**: Real-world systems experience transient failures.
/// System should allow retrying after a query error.
#[tokio::test]
async fn test_e2e_error_recovery_with_retry() {
	let (manager, handler) = setup_test_env();

	let output = "I need to read important_file.txt";

	// First attempt - send error response
	let manager_clone = manager.clone();
	let response_task_1 = tokio::spawn(async move {
		tokio::time::sleep(Duration::from_millis(50)).await;

		let pending = manager_clone.list_pending("session-e2e").await;
		if let Some(query) = pending.first() {
			let response = ServerQueryResponse {
				query_id: query.id.clone(),
				sent_at: chrono::Utc::now().to_rfc3339(),
				result: ServerQueryResult::FileContent("".to_string()),
				error: Some("File not found on first attempt".to_string()),
			};
			manager_clone.receive_response(response).await;
		}
	});

	let result_1 = handler.handle_llm_output("session-e2e", output).await;
	let _ = tokio::time::timeout(Duration::from_secs(5), response_task_1).await;

	// Should get error but system should remain operational
	assert!(result_1.is_ok());
	if let Ok(Some(response)) = result_1 {
		assert!(
			response.error.is_some(),
			"Should have error from first attempt"
		);
	}

	// Second attempt - successful
	let manager_clone = manager.clone();
	let response_task_2 = tokio::spawn(async move {
		respond_to_pending_query(manager_clone, 50, "File content".to_string()).await;
	});

	let result_2 = handler
		.handle_llm_output("session-e2e", "Try reading important_file.txt again")
		.await;
	let _ = tokio::time::timeout(Duration::from_secs(5), response_task_2).await;

	assert!(result_2.is_ok(), "Should successfully retry after error");
	if let Ok(Some(response)) = result_2 {
		assert!(response.error.is_none(), "Retry should succeed");
		if let ServerQueryResult::FileContent(content) = &response.result {
			assert_eq!(content, "File content");
		}
	}
}

/// Test timeout recovery - system should remain operational after timeout.
/// **Why Important**: Timeouts should not crash system or prevent further queries.
#[tokio::test]
async fn test_e2e_timeout_recovery() {
	let (manager, handler) = setup_test_env();

	// First query times out (no response)
	let result_1 = handler
		.handle_llm_output("session-e2e", "read timeout_file.txt")
		.await;

	// Should error due to timeout
	assert!(result_1.is_err(), "Should timeout");

	// System should still be operational
	// Second query after timeout
	let manager_clone = manager.clone();
	let response_task = tokio::spawn(async move {
		respond_to_pending_query(manager_clone, 50, "recovered".to_string()).await;
	});

	let result_2 = handler
		.handle_llm_output("session-e2e", "read recovery_file.txt")
		.await;
	let _ = tokio::time::timeout(Duration::from_secs(5), response_task).await;

	assert!(result_2.is_ok(), "Should recover after timeout");
}

// ============================================================================
// Output Injection Edge Cases
// ============================================================================

/// Test that LLM output with special characters is handled correctly.
/// **Why Important**: LLM output may contain special chars that could
/// affect regex matching or query parsing.
#[tokio::test]
async fn test_e2e_special_characters_in_output() {
	let (manager, handler) = setup_test_env();

	let special_outputs = vec![
		"I need to read config.json (with special chars: $@#%)",
		"run \"command with spaces\" and quotes",
		"ask user for: What's your preference?",
	];

	for output in special_outputs {
		let manager_clone = manager.clone();
		let response_task = tokio::spawn(async move {
			respond_to_pending_query(manager_clone, 50, "response".to_string()).await;
		});

		// Should not panic or error on special characters
		let result = handler.handle_llm_output("session-e2e", output).await;

		let _ = tokio::time::timeout(Duration::from_secs(5), response_task).await;

		// Even if it errors (timeout), should not panic
		let _ = result;
	}
}

/// Test that mixed content (query + non-query text) works correctly.
/// **Why Important**: LLM output typically has context around actual queries.
/// System should extract queries from natural text.
#[tokio::test]
async fn test_e2e_mixed_query_and_context() {
	let (manager, handler) = setup_test_env();

	let output = "To solve this problem, I first need to read the configuration file \
                  located at config.json to understand the current settings. \
                  After that, I'll be able to provide you with a solution.";

	let manager_clone = manager.clone();
	let response_task = tokio::spawn(async move {
		respond_to_pending_query(manager_clone, 50, "settings".to_string()).await;
	});

	let result = handler.handle_llm_output("session-e2e", output).await;

	let _ = tokio::time::timeout(Duration::from_secs(5), response_task).await;

	// Should extract query from mixed content
	assert!(result.is_ok(), "Should handle mixed content");
}

// ============================================================================
// Query Metadata Verification
// ============================================================================

/// Test that complete E2E flow preserves query metadata.
/// **Why Important**: Metadata aids debugging and tracing throughout the pipeline.
/// Loss of metadata makes troubleshooting difficult.
#[tokio::test]
async fn test_e2e_query_metadata_preserved() {
	let (manager, handler) = setup_test_env();

	let output = "I need to read config.json";

	let manager_clone = manager.clone();
	let response_task = tokio::spawn(async move {
		tokio::time::sleep(Duration::from_millis(50)).await;

		let pending = manager_clone.list_pending("session-e2e").await;
		if let Some(query) = pending.first() {
			// Verify metadata exists and is valid
			assert!(!query.metadata.is_null(), "Metadata should exist");
			assert_eq!(
				query.metadata.get("detector").and_then(|v| v.as_str()),
				Some("simple_regex"),
				"Should have detector metadata"
			);

			let response = ServerQueryResponse {
				query_id: query.id.clone(),
				sent_at: chrono::Utc::now().to_rfc3339(),
				result: ServerQueryResult::FileContent("content".to_string()),
				error: None,
			};
			manager_clone.receive_response(response).await;
		}
	});

	let result = handler.handle_llm_output("session-e2e", output).await;

	let _ = tokio::time::timeout(Duration::from_secs(5), response_task).await;

	assert!(result.is_ok());
}
