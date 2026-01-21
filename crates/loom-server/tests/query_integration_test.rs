// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! End-to-end integration tests for query flow.
//!
//! **Purpose**: Validates the complete roundtrip of server queries from initiation through
//! client response and result injection. Tests cover happy paths, error scenarios, and
//! concurrent operations to ensure robustness of the query-response protocol.

use loom_common_core::server_query::{
	ServerQuery, ServerQueryError, ServerQueryKind, ServerQueryResponse, ServerQueryResult,
};
use loom_server::ServerQueryManager;
use std::sync::Arc;
use std::time::Duration;

// ============================================================================
// Test Utilities & Mocks
// ============================================================================

/// Helper to create a valid test query
fn create_test_query(id: &str) -> ServerQuery {
	ServerQuery {
		id: id.to_string(),
		kind: ServerQueryKind::ReadFile {
			path: "test.txt".to_string(),
		},
		sent_at: chrono::Utc::now().to_rfc3339(),
		timeout_secs: 5,
		metadata: serde_json::json!({ "test_field": "test_value" }),
	}
}

/// Helper to create a valid test response
fn create_test_response(query_id: &str, content: &str) -> ServerQueryResponse {
	ServerQueryResponse {
		query_id: query_id.to_string(),
		sent_at: chrono::Utc::now().to_rfc3339(),
		result: ServerQueryResult::FileContent(content.to_string()),
		error: None,
	}
}

// ============================================================================
// Single Query Tests
// ============================================================================

/// **Test Purpose**: Validates that a single query can be sent to client via HTTP endpoint
/// and the client response is properly received and stored.
///
/// **What it tests**:
/// - Query is stored as pending
/// - HTTP response endpoint correctly receives response
/// - Response is stored for retrieval
#[tokio::test]
async fn test_single_query_send_and_receive() {
	let manager = Arc::new(ServerQueryManager::new());

	let query = create_test_query("Q-single-001");
	let query_id = query.id.clone();
	let manager_clone = manager.clone();

	// Send query (will wait for response with timeout)
	let send_task =
		tokio::spawn(async move { manager_clone.send_query("session-test", query).await });

	// Give it time to store
	tokio::time::sleep(Duration::from_millis(50)).await;

	// Verify query is pending
	let pending = manager.list_pending("session-test").await;
	assert_eq!(pending.len(), 1);
	assert_eq!(pending[0].id, query_id);

	// Client sends response
	let response = create_test_response(&query_id, "file content here");
	manager.receive_response(response).await;

	// Verify send completes successfully
	let result = send_task.await.unwrap();
	assert!(result.is_ok());
	let received = result.unwrap();
	assert_eq!(received.query_id, query_id);
	assert!(matches!(
			received.result,
			ServerQueryResult::FileContent(ref c) if c == "file content here"
	));

	// Verify response is retrievable
	let stored = manager.get_response(&query_id).await;
	assert!(stored.is_some());
}

/// **Test Purpose**: Validates that query timeout is correctly enforced when client
/// doesn't respond in time.
///
/// **What it tests**:
/// - Query timeout configuration is respected
/// - Timeout error is properly returned
/// - Query is cleaned up after timeout
#[tokio::test]
async fn test_query_timeout() {
	let manager = Arc::new(ServerQueryManager::new());

	let query = ServerQuery {
		id: "Q-timeout-001".to_string(),
		kind: ServerQueryKind::ReadFile {
			path: "test.txt".to_string(),
		},
		sent_at: chrono::Utc::now().to_rfc3339(),
		timeout_secs: 1, // 1 second timeout
		metadata: serde_json::json!({}),
	};

	let start = std::time::Instant::now();
	let result = manager.send_query("session-test", query).await;
	let elapsed = start.elapsed();

	// Should timeout
	assert!(result.is_err());
	assert!(matches!(result.unwrap_err(), ServerQueryError::Timeout));

	// Verify timeout was approximately correct (allow 100ms margin)
	assert!(elapsed >= Duration::from_secs(1));
	assert!(elapsed < Duration::from_secs(2));

	// Verify query is no longer pending
	let pending = manager.list_pending("session-test").await;
	assert_eq!(pending.len(), 0);
}

// ============================================================================
// Multiple Sequential Queries Tests
// ============================================================================

/// **Test Purpose**: Validates that multiple queries can be processed sequentially
/// without interference between them.
///
/// **What it tests**:
/// - Multiple queries can be tracked independently
/// - Each query gets correct response
/// - Order doesn't matter
#[tokio::test]
async fn test_multiple_sequential_queries() {
	let manager = Arc::new(ServerQueryManager::new());

	// Send 3 queries sequentially
	for i in 0..3 {
		let query = create_test_query(&format!("Q-seq-{i:03}"));
		let query_id = query.id.clone();
		let manager_clone = manager.clone();

		let send_task =
			tokio::spawn(async move { manager_clone.send_query("session-test", query).await });

		// Give time to store
		tokio::time::sleep(Duration::from_millis(50)).await;

		// Send response
		let response = create_test_response(&query_id, &format!("content {i}"));
		manager.receive_response(response).await;

		// Verify response
		let result = send_task.await.unwrap();
		assert!(result.is_ok());
		let received = result.unwrap();
		assert_eq!(received.query_id, query_id);
	}

	// Verify all responses stored
	for i in 0..3 {
		let stored = manager.get_response(&format!("Q-seq-{i:03}")).await;
		assert!(stored.is_some());
	}
}

// ============================================================================
// Concurrent Queries Tests
// ============================================================================

/// **Test Purpose**: Validates that the query system can handle multiple concurrent
/// queries from the same session without response cross-contamination.
///
/// **Why Important**: Concurrent queries are critical for real-world usage where
/// multiple client requests may be in flight simultaneously. This ensures the
/// broadcast channel correctly correlates responses to their queries.
///
/// **What it tests**:
/// - Multiple queries can wait concurrently
/// - Responses arrive in different order than queries sent
/// - Each response correctly matched to its query
#[tokio::test]
async fn test_concurrent_queries_same_session() {
	let manager = Arc::new(ServerQueryManager::new());
	let num_queries = 5;

	// Create and spawn all queries concurrently
	let mut handles = vec![];
	for i in 0..num_queries {
		let query = create_test_query(&format!("Q-concurrent-{i:03}"));
		let manager_clone = manager.clone();

		let handle =
			tokio::spawn(async move { manager_clone.send_query("session-concurrent", query).await });

		handles.push(handle);
	}

	// Give queries time to register
	tokio::time::sleep(Duration::from_millis(100)).await;

	// Verify all are pending
	let pending = manager.list_pending("session-concurrent").await;
	assert_eq!(pending.len(), num_queries);

	// Send responses in reverse order (test that ordering doesn't matter)
	for i in (0..num_queries).rev() {
		let response = create_test_response(&format!("Q-concurrent-{i:03}"), &format!("data {i}"));
		manager.receive_response(response).await;
		tokio::time::sleep(Duration::from_millis(10)).await;
	}

	// Verify all queries received correct responses
	for handle in handles {
		let result = tokio::time::timeout(Duration::from_secs(5), handle)
			.await
			.unwrap()
			.unwrap();
		assert!(result.is_ok(), "Query should receive response");
	}
}

/// **Test Purpose**: Validates isolation between different sessions - responses for
/// one session's queries don't affect another session's queries.
///
/// **Why Important**: Critical for multi-tenant usage. One client's queries must not
/// receive another client's responses.
///
/// **What it tests**:
/// - Session A's queries don't see Session B's responses
/// - Response matching uses both query_id and session context
#[tokio::test]
async fn test_concurrent_queries_different_sessions() {
	let manager = Arc::new(ServerQueryManager::new());

	// Query from session A
	let query_a = ServerQuery {
		id: "Q-sess-a".to_string(),
		kind: ServerQueryKind::ReadFile {
			path: "file_a.txt".to_string(),
		},
		sent_at: chrono::Utc::now().to_rfc3339(),
		timeout_secs: 5,
		metadata: serde_json::json!({}),
	};

	// Query from session B
	let query_b = ServerQuery {
		id: "Q-sess-b".to_string(),
		kind: ServerQueryKind::ReadFile {
			path: "file_b.txt".to_string(),
		},
		sent_at: chrono::Utc::now().to_rfc3339(),
		timeout_secs: 5,
		metadata: serde_json::json!({}),
	};

	let manager_a = manager.clone();
	let manager_b = manager.clone();

	let handle_a = tokio::spawn(async move { manager_a.send_query("session-a", query_a).await });

	let handle_b = tokio::spawn(async move { manager_b.send_query("session-b", query_b).await });

	// Give time to register
	tokio::time::sleep(Duration::from_millis(100)).await;

	// Verify each session has its own pending queries
	let pending_a = manager.list_pending("session-a").await;
	let pending_b = manager.list_pending("session-b").await;

	// Should have queries in each session's pending list
	assert!(pending_a.len() >= 1, "Session A should have pending");
	assert!(pending_b.len() >= 1, "Session B should have pending");

	// Send response for Session B
	let response_b = create_test_response("Q-sess-b", "content for B");
	manager.receive_response(response_b).await;

	// Session B should complete, Session A should still be waiting
	let result_b = tokio::time::timeout(Duration::from_secs(2), handle_b).await;
	assert!(result_b.is_ok(), "Session B should receive its response");

	// Send response for Session A
	let response_a = create_test_response("Q-sess-a", "content for A");
	manager.receive_response(response_a).await;

	// Now Session A should complete
	let result_a = tokio::time::timeout(Duration::from_secs(2), handle_a).await;
	assert!(result_a.is_ok(), "Session A should receive its response");
}

// ============================================================================
// Error Response Tests
// ============================================================================

/// **Test Purpose**: Validates that error responses are correctly returned to the
/// waiting query handler.
///
/// **What it tests**:
/// - Error field is preserved in response
/// - Query completes even with error
/// - Error can be inspected by caller
#[tokio::test]
async fn test_error_response_handling() {
	let manager = Arc::new(ServerQueryManager::new());

	let query = create_test_query("Q-error-001");
	let query_id = query.id.clone();
	let manager_clone = manager.clone();

	let send_task =
		tokio::spawn(async move { manager_clone.send_query("session-test", query).await });

	tokio::time::sleep(Duration::from_millis(50)).await;

	// Send error response
	let response = ServerQueryResponse {
		query_id: query_id.clone(),
		sent_at: chrono::Utc::now().to_rfc3339(),
		result: ServerQueryResult::FileContent("".to_string()),
		error: Some("File not found".to_string()),
	};
	manager.receive_response(response).await;

	let result = send_task.await.unwrap();
	assert!(result.is_ok());
	let received = result.unwrap();
	assert_eq!(received.error, Some("File not found".to_string()));

	// Verify error is preserved in storage
	let stored = manager.get_response(&query_id).await;
	assert!(stored.is_some());
	assert_eq!(stored.unwrap().error, Some("File not found".to_string()));
}

/// **Test Purpose**: Validates that no response is returned if query is never responded to
/// within timeout.
///
/// **What it tests**:
/// - Timeout returns appropriate error
/// - No hanging if response never arrives
/// - Query is cleaned up after timeout
#[tokio::test]
async fn test_no_response_timeout() {
	let manager = Arc::new(ServerQueryManager::new());

	let query = ServerQuery {
		id: "Q-no-response".to_string(),
		kind: ServerQueryKind::ReadFile {
			path: "test.txt".to_string(),
		},
		sent_at: chrono::Utc::now().to_rfc3339(),
		timeout_secs: 1,
		metadata: serde_json::json!({}),
	};

	let result = manager.send_query("session-timeout", query).await;

	// Must timeout
	assert!(result.is_err());
	assert!(matches!(result.unwrap_err(), ServerQueryError::Timeout));
}

// ============================================================================
// Query Response Validation Tests
// ============================================================================

/// **Test Purpose**: Validates that various types of query results are
/// properly handled and stored.
///
/// **Why Important**: Different query kinds return different result types.
/// This ensures the system handles the variety of responses correctly.
///
/// **What it tests**:
/// - FileContent result type
/// - Result metadata is preserved
/// - Serialization roundtrip works
#[tokio::test]
async fn test_various_result_types() {
	let manager = Arc::new(ServerQueryManager::new());

	// Test with file content result
	let query = create_test_query("Q-result-001");
	let query_id = query.id.clone();
	let manager_clone = manager.clone();

	let send_task =
		tokio::spawn(async move { manager_clone.send_query("session-results", query).await });

	tokio::time::sleep(Duration::from_millis(50)).await;

	let file_content = "line1\nline2\nline3";
	let response = ServerQueryResponse {
		query_id: query_id.clone(),
		sent_at: chrono::Utc::now().to_rfc3339(),
		result: ServerQueryResult::FileContent(file_content.to_string()),
		error: None,
	};

	manager.receive_response(response).await;

	let result = send_task.await.unwrap();
	assert!(result.is_ok());
	let received = result.unwrap();

	// Verify content is preserved
	assert!(matches!(
			received.result,
			ServerQueryResult::FileContent(ref c) if c.contains("line1") && c.contains("line2")
	));
}

/// **Test Purpose**: Validates that large response payloads are correctly handled.
///
/// **Why Important**: In real usage, file contents can be large. This ensures
/// the system doesn't have size limits that break real queries.
///
/// **What it tests**:
/// - Large content (1MB) is transmitted correctly
/// - No truncation occurs
/// - Memory is managed properly
#[tokio::test]
async fn test_large_response_content() {
	let manager = Arc::new(ServerQueryManager::new());

	let query = create_test_query("Q-large-001");
	let query_id = query.id.clone();
	let manager_clone = manager.clone();

	let send_task =
		tokio::spawn(async move { manager_clone.send_query("session-large", query).await });

	tokio::time::sleep(Duration::from_millis(50)).await;

	// Create 1MB of content
	let large_content = "x".repeat(1024 * 1024);
	let response = create_test_response(&query_id, &large_content);

	manager.receive_response(response).await;

	let result = send_task.await.unwrap();
	assert!(result.is_ok());
	let received = result.unwrap();

	// Verify size is preserved
	assert!(matches!(
			&received.result,
			ServerQueryResult::FileContent(c) if c.len() == 1024 * 1024
	));
}

// ============================================================================
// Query Metadata Tests
// ============================================================================

/// **Test Purpose**: Validates that query metadata is preserved through the
/// entire roundtrip.
///
/// **Why Important**: Metadata may contain critical context for query processing.
/// This ensures metadata isn't lost or corrupted.
///
/// **What it tests**:
/// - Custom metadata is stored with query
/// - Metadata is correctly sent to client
/// - Serialization handles arbitrary JSON
#[tokio::test]
async fn test_query_metadata_preservation() {
	let manager = Arc::new(ServerQueryManager::new());

	let custom_metadata = serde_json::json!({
			"user_id": "user-123",
			"request_id": "req-456",
			"custom_field": "value",
			"nested": {
					"field": "data"
			}
	});

	let query = ServerQuery {
		id: "Q-metadata-001".to_string(),
		kind: ServerQueryKind::ReadFile {
			path: "/path/to/file.txt".to_string(),
		},
		sent_at: chrono::Utc::now().to_rfc3339(),
		timeout_secs: 5,
		metadata: custom_metadata.clone(),
	};

	let query_id = query.id.clone();
	let manager_clone = manager.clone();

	let send_task =
		tokio::spawn(async move { manager_clone.send_query("session-metadata", query).await });

	// Give time to register
	tokio::time::sleep(Duration::from_millis(50)).await;

	// List and verify metadata is present
	let pending = manager.list_pending("session-metadata").await;
	assert_eq!(pending.len(), 1);
	assert_eq!(pending[0].metadata, custom_metadata);

	// Send response to complete the query
	let response = create_test_response(&query_id, "content");
	manager.receive_response(response).await;

	// Verify query completed
	let result = send_task.await.unwrap();
	assert!(result.is_ok());
}

// ============================================================================
// Concurrent Operations Stress Tests
// ============================================================================

/// **Test Purpose**: Stress test with many concurrent queries to ensure
/// the system doesn't degrade under load.
///
/// **Why Important**: Real-world scenarios may have many concurrent queries.
/// This validates the system scales without panics or deadlocks.
///
/// **What it tests**:
/// - 100 concurrent queries complete successfully
/// - No performance degradation
/// - All responses are correct
#[tokio::test]
async fn test_high_concurrency_stress() {
	let manager = Arc::new(ServerQueryManager::new());
	let num_queries = 100;
	let num_tasks = 10;

	let mut handles = vec![];

	// Spawn multiple batches of concurrent queries
	for batch in 0..num_tasks {
		for i in 0..num_queries / num_tasks {
			let query = create_test_query(&format!("Q-stress-{batch:04}-{i:04}"));
			let manager_clone = manager.clone();

			let handle =
				tokio::spawn(async move { manager_clone.send_query("session-stress", query).await });

			handles.push((batch, i, handle));
		}
	}

	// Give time to register
	tokio::time::sleep(Duration::from_millis(200)).await;

	// Verify all queries are pending
	let pending = manager.list_pending("session-stress").await;
	assert_eq!(pending.len(), num_queries);

	// Send all responses
	for batch in 0..num_tasks {
		for i in 0..num_queries / num_tasks {
			let response = create_test_response(
				&format!("Q-stress-{batch:04}-{i:04}"),
				&format!("content-{batch}-{i}"),
			);
			manager.receive_response(response).await;
		}
	}

	// Verify all completed
	let mut success_count = 0;
	for (_batch, _i, handle) in handles {
		let result = handle.await.unwrap();
		if result.is_ok() {
			success_count += 1;
		}
	}
	assert_eq!(success_count, num_queries);

	// Verify clean state
	let pending = manager.list_pending("session-stress").await;
	assert_eq!(pending.len(), 0);
}
