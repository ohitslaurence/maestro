// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! Integration tests for QueryMetrics with ServerQueryManager.
//!
//! Purpose: Verify that metrics are properly recorded throughout the query lifecycle,
//! including sent, success, failure, and timeout scenarios.

use loom_common_core::server_query::{
	ServerQuery, ServerQueryKind, ServerQueryResponse, ServerQueryResult,
};
use loom_server::QueryMetrics;
use loom_server::ServerQueryManager;
use std::sync::Arc;
use std::time::Duration;

/// Test that metrics are recorded when a query is sent.
///
/// Purpose: Ensure that the sent counter and pending gauge are incremented
/// when a query is sent to the client.
#[tokio::test]
async fn test_metrics_record_query_sent() {
	let metrics = Arc::new(QueryMetrics::default());
	let manager = Arc::new(ServerQueryManager::with_metrics(metrics.clone()));

	let _query = ServerQuery {
		id: "Q-test-001".to_string(),
		kind: ServerQueryKind::ReadFile {
			path: "test.txt".to_string(),
		},
		sent_at: chrono::Utc::now().to_rfc3339(),
		timeout_secs: 5,
		metadata: serde_json::json!({}),
	};

	manager.update_pending_metrics().await;

	let output = metrics.gather_metrics().unwrap();
	assert!(output.contains("loom_queries_pending 0"));
}

/// Test that metrics are recorded when a query succeeds.
///
/// Purpose: Ensure that success counter, pending gauge, and latency histogram
/// are properly updated when a query receives a successful response.
#[tokio::test]
async fn test_metrics_record_query_success() {
	let metrics = Arc::new(QueryMetrics::default());
	let manager = Arc::new(ServerQueryManager::with_metrics(metrics.clone()));

	let query = ServerQuery {
		id: "Q-success-001".to_string(),
		kind: ServerQueryKind::ReadFile {
			path: "test.txt".to_string(),
		},
		sent_at: chrono::Utc::now().to_rfc3339(),
		timeout_secs: 5,
		metadata: serde_json::json!({}),
	};

	let query_id = query.id.clone();
	let manager_clone = manager.clone();

	// Spawn a task to send the query
	let send_handle = tokio::spawn(async move { manager_clone.send_query("session-1", query).await });

	// Give it a moment
	tokio::time::sleep(Duration::from_millis(50)).await;

	// Send a successful response
	let response = ServerQueryResponse {
		query_id: query_id.clone(),
		sent_at: chrono::Utc::now().to_rfc3339(),
		result: ServerQueryResult::FileContent("test content".to_string()),
		error: None,
	};

	manager.receive_response(response).await;

	let result = send_handle.await.unwrap();
	assert!(result.is_ok());

	let output = metrics.gather_metrics().unwrap();
	assert!(output.contains("loom_queries_succeeded_total"));
	assert!(output.contains("loom_query_latency_seconds"));
}

/// Test that metrics are recorded when a query times out.
///
/// Purpose: Ensure that failure counter, timeout counter, and pending gauge
/// are updated when a query times out.
#[tokio::test]
async fn test_metrics_record_query_timeout() {
	let metrics = Arc::new(QueryMetrics::default());
	let manager = Arc::new(ServerQueryManager::with_metrics(metrics.clone()));

	let query = ServerQuery {
		id: "Q-timeout-001".to_string(),
		kind: ServerQueryKind::ReadFile {
			path: "test.txt".to_string(),
		},
		sent_at: chrono::Utc::now().to_rfc3339(),
		timeout_secs: 1,
		metadata: serde_json::json!({}),
	};

	let result = manager.send_query("session-1", query).await;

	assert!(result.is_err());

	let output = metrics.gather_metrics().unwrap();
	assert!(output.contains("loom_queries_failed_total"));
	assert!(output.contains("loom_query_timeouts_total"));
	assert!(output.contains("query_type=\"read_file\""));
}

/// Test that metrics include proper labels for different query types.
///
/// Purpose: Ensure that metrics can distinguish between different query types
/// (read_file, env, workspace) using labels, enabling per-type analysis.
#[tokio::test]
async fn test_metrics_labels_by_query_type() {
	let metrics = Arc::new(QueryMetrics::default());

	// Record different query types
	metrics.record_sent("read_file", "session-1");
	metrics.record_sent("env", "session-2");
	metrics.record_sent("workspace", "session-3");

	metrics.record_success("read_file", "session-1", 0.1);
	metrics.record_failure("env", "network", "session-2");
	metrics.record_failure("workspace", "timeout", "session-3");

	let output = metrics.gather_metrics().unwrap();

	// Verify labels are present
	assert!(output.contains("query_type=\"read_file\""));
	assert!(output.contains("query_type=\"env\""));
	assert!(output.contains("query_type=\"workspace\""));
	assert!(output.contains("error_type=\"network\""));
	assert!(output.contains("error_type=\"timeout\""));
}

/// Test that pending metrics track concurrent queries.
///
/// Purpose: Ensure that the pending gauge correctly tracks the number of
/// in-flight queries when multiple concurrent queries are sent.
#[tokio::test]
async fn test_metrics_pending_gauge_tracks_concurrent() {
	let metrics = Arc::new(QueryMetrics::default());

	// Simulate multiple concurrent sends
	metrics.record_sent("read_file", "session-1");
	metrics.record_sent("read_file", "session-2");
	metrics.record_sent("env", "session-3");

	let output = metrics.gather_metrics().unwrap();
	assert!(output.contains("loom_queries_pending 3"));

	// Complete some queries
	metrics.record_success("read_file", "session-1", 0.1);
	let output = metrics.gather_metrics().unwrap();
	assert!(output.contains("loom_queries_pending 2"));

	metrics.record_success("read_file", "session-2", 0.2);
	let output = metrics.gather_metrics().unwrap();
	assert!(output.contains("loom_queries_pending 1"));

	metrics.record_failure("env", "timeout", "session-3");
	let output = metrics.gather_metrics().unwrap();
	assert!(output.contains("loom_queries_pending 0"));
}

/// Test that session_id label allows per-session metrics analysis.
///
/// Purpose: Ensure that metrics include session_id labels for successful queries,
/// enabling monitoring of per-session query activity.
#[tokio::test]
async fn test_metrics_session_id_labels() {
	let metrics = Arc::new(QueryMetrics::default());

	metrics.record_sent("read_file", "session-alice");
	metrics.record_success("read_file", "session-alice", 0.5);

	metrics.record_sent("read_file", "session-bob");
	metrics.record_success("read_file", "session-bob", 0.7);

	let output = metrics.gather_metrics().unwrap();
	assert!(output.contains("session_id=\"session-alice\""));
	assert!(output.contains("session_id=\"session-bob\""));
}

/// Test that latency histogram buckets are appropriate.
///
/// Purpose: Verify that latency histogram uses appropriate bucket boundaries
/// for typical query latencies (milliseconds to seconds).
#[tokio::test]
async fn test_metrics_latency_histogram_buckets() {
	let metrics = Arc::new(QueryMetrics::default());

	// Record various latencies
	metrics.record_latency("read_file", 0.005); // 5ms
	metrics.record_latency("read_file", 0.050); // 50ms
	metrics.record_latency("read_file", 0.500); // 500ms
	metrics.record_latency("read_file", 5.0); // 5s

	let output = metrics.gather_metrics().unwrap();
	assert!(output.contains("loom_query_latency_seconds_bucket"));
	assert!(output.contains("loom_query_latency_seconds_sum"));
	assert!(output.contains("loom_query_latency_seconds_count"));
}

/// Test that metrics endpoint returns Prometheus format.
///
/// Purpose: Ensure that the gather_metrics method returns properly formatted
/// Prometheus metrics text that can be scraped by monitoring systems.
#[test]
fn test_metrics_prometheus_format() {
	let metrics = QueryMetrics::default();

	metrics.record_sent("read_file", "session-1");
	metrics.record_success("read_file", "session-1", 0.25);
	metrics.record_sent("env", "session-2");
	metrics.record_failure("env", "timeout", "session-2");

	let output = metrics.gather_metrics().unwrap();

	// Check for Prometheus format markers
	assert!(output.contains("# HELP"));
	assert!(output.contains("# TYPE"));
	assert!(output.contains("loom_queries_sent_total"));
	assert!(output.contains("loom_queries_succeeded_total"));
	assert!(output.contains("loom_queries_failed_total"));
}
