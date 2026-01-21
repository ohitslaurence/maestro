// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! Integration tests for query tracing with server query manager.
//!
//! These tests demonstrate the complete tracing infrastructure including
//! tracer creation, event recording, and retrieval through debug endpoints.

use loom_server::{QueryTraceStore, QueryTracer, TraceTimeline};
use std::time::Duration;

/// Test that a tracer can be created with correct initialization.
/// **Why Important**: Ensures tracers are properly initialized with trace_id,
/// query_id, and session_id for proper correlation and debugging.
#[test]
fn test_tracer_initialization() {
	let tracer = QueryTracer::new("Q-init-test", Some("session-123".to_string()));

	assert_eq!(tracer.query_id, "Q-init-test");
	assert_eq!(tracer.session_id, Some("session-123".to_string()));
	assert!(tracer.trace_id.as_str().starts_with("TRACE-"));
	assert_eq!(tracer.events.len(), 1); // Should have creation event
	assert_eq!(tracer.events[0].event_type, "created");
}

/// Test that events are recorded in chronological order with correct sequences.
/// **Why Important**: Event ordering is critical for reconstructing the query
/// lifecycle and identifying where performance bottlenecks occur.
#[tokio::test]
async fn test_event_recording_order() {
	let mut tracer = QueryTracer::new("Q-order-test", None);
	let initial_count = tracer.events.len();

	tracer.record_sent(5);
	assert_eq!(tracer.events.len(), initial_count + 1);

	tokio::time::sleep(Duration::from_millis(5)).await;
	tracer.record_response_received("ok");
	assert_eq!(tracer.events.len(), initial_count + 2);

	tracer.record_event("completed", serde_json::json!({}));
	assert_eq!(tracer.events.len(), initial_count + 3);

	// Verify sequences are correct
	assert_eq!(tracer.events[initial_count].sequence, 1);
	assert_eq!(tracer.events[initial_count + 1].sequence, 2);
	assert_eq!(tracer.events[initial_count + 2].sequence, 3);
}

/// Test that durations between events are calculated correctly.
/// **Why Important**: Duration calculations enable performance analysis and
/// identification of slow operations in the query lifecycle.
#[tokio::test]
async fn test_duration_calculation() {
	let mut tracer = QueryTracer::new("Q-duration-test", None);

	tokio::time::sleep(Duration::from_millis(20)).await;
	tracer.record_sent(10);

	tokio::time::sleep(Duration::from_millis(30)).await;
	tracer.record_response_received("ok");

	// Total duration should be roughly 50ms (20 + 30)
	let total = tracer.total_duration();
	assert!(
		total.as_millis() >= 45,
		"Expected >= 45ms, got {}ms",
		total.as_millis()
	);
	assert!(
		total.as_millis() <= 100,
		"Expected <= 100ms, got {}ms",
		total.as_millis()
	);

	// Event duration should be roughly 30ms
	let event_duration = tracer.event_duration(1, 2);
	assert!(event_duration.is_some());
	let event_duration = event_duration.unwrap();
	assert!(
		event_duration.as_millis() >= 20,
		"Expected >= 20ms, got {}ms",
		event_duration.as_millis()
	);
	assert!(
		event_duration.as_millis() <= 100,
		"Expected <= 100ms, got {}ms",
		event_duration.as_millis()
	);
}

/// Test that error events are correctly detected.
/// **Why Important**: Error detection enables filtering and alerting on failed queries.
#[test]
fn test_error_detection() {
	let mut tracer = QueryTracer::new("Q-error-test", None);
	assert!(!tracer.has_error());

	tracer.record_error("connection failed", None);
	assert!(tracer.has_error());
}

/// Test that timeout events are correctly detected.
/// **Why Important**: Timeout detection enables identifying slow or unresponsive queries.
#[test]
fn test_timeout_detection() {
	let mut tracer = QueryTracer::new("Q-timeout-test", None);
	assert!(!tracer.has_error());

	tracer.record_timeout(30);
	assert!(tracer.has_error());
}

/// Test that trace store correctly manages capacity and eviction.
/// **Why Important**: LRU eviction prevents unbounded memory growth in production
/// while ensuring recent traces are retained for debugging.
#[tokio::test]
async fn test_trace_store_capacity_management() {
	let store = QueryTraceStore::new(5);

	// Add 10 traces, but capacity is 5
	for i in 0..10 {
		let tracer = QueryTracer::new(format!("Q-cap-{i}"), None);
		store.store(tracer).await;
	}

	let stats = store.get_stats().await;
	assert_eq!(
		stats.total_traces, 5,
		"Store should evict old traces to maintain capacity"
	);
}

/// Test that trace store can filter by session ID.
/// **Why Important**: Session filtering enables isolating traces for specific
/// client sessions during debugging.
#[tokio::test]
async fn test_session_filtering() {
	let store = QueryTraceStore::new(100);

	// Create traces for different sessions
	let mut t1 = QueryTracer::new("Q-s1-1", Some("session-1".to_string()));
	let mut t2 = QueryTracer::new("Q-s1-2", Some("session-1".to_string()));
	let mut t3 = QueryTracer::new("Q-s2-1", Some("session-2".to_string()));

	t1.record_sent(5);
	t2.record_sent(5);
	t3.record_sent(5);

	store.store(t1).await;
	store.store(t2).await;
	store.store(t3).await;

	let s1_traces = store.get_session_traces("session-1").await;
	let s2_traces = store.get_session_traces("session-2").await;

	assert_eq!(s1_traces.len(), 2);
	assert_eq!(s2_traces.len(), 1);
}

/// Test that slow trace detection works correctly.
/// **Why Important**: Slow trace detection enables performance monitoring
/// and identification of bottlenecks.
#[tokio::test]
async fn test_slow_trace_detection() {
	let store = QueryTraceStore::new(100);

	// Create a slow trace
	let mut slow_tracer = QueryTracer::new("Q-slow", None);
	tokio::time::sleep(Duration::from_millis(100)).await;
	slow_tracer.record_sent(5);

	store.store(slow_tracer).await;

	let slow_traces = store.get_slow_traces(Duration::from_millis(50)).await;
	assert_eq!(slow_traces.len(), 1);
	assert_eq!(slow_traces[0].query_id, "Q-slow");
}

/// Test that timeline view correctly formats all trace data.
/// **Why Important**: Timeline view is used by debug endpoints to provide
/// human-readable trace information.
#[test]
fn test_timeline_formatting() {
	let mut tracer = QueryTracer::new("Q-timeline", Some("session-tl".to_string()));
	tracer.record_sent(10);
	tracer.record_response_received("success");
	tracer.record_event("validated", serde_json::json!({"passed": true}));

	let timeline = TraceTimeline::from_tracer(&tracer);

	assert_eq!(timeline.query_id, "Q-timeline");
	assert_eq!(timeline.session_id, Some("session-tl".to_string()));
	assert_eq!(timeline.events.len(), 4); // created + sent + response + validated
	assert!(!timeline.has_error);
	assert!(timeline.total_duration_ms >= 0);

	// Verify events have proper format
	for (i, event) in timeline.events.iter().enumerate() {
		assert_eq!(event.sequence as usize, i);
		assert!(!event.event_type.is_empty());
		assert!(!event.timestamp.is_empty());
		assert!(event.elapsed_ms >= 0);
	}
}

/// Test that trace store statistics are accurate.
/// **Why Important**: Statistics are essential for monitoring trace store
/// health and performance characteristics.
#[tokio::test]
async fn test_trace_statistics() {
	let store = QueryTraceStore::new(100);

	// Create traces with various characteristics
	let mut normal_tracer = QueryTracer::new("Q-normal", None);
	normal_tracer.record_sent(5);
	normal_tracer.record_response_received("ok");

	let mut error_tracer = QueryTracer::new("Q-error", None);
	error_tracer.record_sent(5);
	error_tracer.record_error("failed", None);

	let mut timeout_tracer = QueryTracer::new("Q-timeout", None);
	timeout_tracer.record_sent(5);
	timeout_tracer.record_timeout(5);

	store.store(normal_tracer).await;
	store.store(error_tracer).await;
	store.store(timeout_tracer).await;

	let stats = store.get_stats().await;

	assert_eq!(stats.total_traces, 3);
	assert_eq!(stats.traces_with_errors, 2);
	assert!(stats.avg_events_per_trace > 0);
}

/// Test that clear operation removes all traces.
/// **Why Important**: Clear functionality is essential for testing and
/// memory management during server operation.
#[tokio::test]
async fn test_trace_store_clear() {
	let store = QueryTraceStore::new(100);

	// Add several traces
	for i in 0..5 {
		let tracer = QueryTracer::new(format!("Q-clear-{i}"), None);
		store.store(tracer).await;
	}

	let stats_before = store.get_stats().await;
	assert!(stats_before.total_traces > 0);

	store.clear().await;

	let stats_after = store.get_stats().await;
	assert_eq!(stats_after.total_traces, 0);
}

/// Test complete query lifecycle tracing.
/// **Why Important**: This integration test demonstrates the entire query
/// tracing flow from creation through completion.
#[tokio::test]
async fn test_complete_query_lifecycle() {
	let store = QueryTraceStore::new(100);

	// Simulate complete query lifecycle
	let mut tracer = QueryTracer::new("Q-lifecycle-full", Some("session-lifecycle".to_string()));
	let trace_id = tracer.trace_id.as_str().to_string();

	// Phase 1: Query created and sent
	tracer.record_sent(10);
	tokio::time::sleep(Duration::from_millis(5)).await;

	// Phase 2: Response received
	tracer.record_response_received("ok");
	tokio::time::sleep(Duration::from_millis(5)).await;

	// Phase 3: Processing and validation
	tracer.record_event("processing", serde_json::json!({"stage": "validation"}));
	tokio::time::sleep(Duration::from_millis(5)).await;

	// Phase 4: Completion
	tracer.record_event("completed", serde_json::json!({"status": "success"}));

	// Store the complete trace
	store.store(tracer.clone()).await;

	// Verify retrieval
	let retrieved = store.get(&trace_id).await;
	assert!(retrieved.is_some());

	let retrieved = retrieved.unwrap();
	assert_eq!(retrieved.query_id, "Q-lifecycle-full");
	assert_eq!(retrieved.session_id, Some("session-lifecycle".to_string()));
	assert_eq!(retrieved.events.len(), 5); // created + sent + response + processing + completed
	assert!(!retrieved.has_error());

	// Verify timeline
	let timeline = TraceTimeline::from_tracer(&retrieved);
	assert_eq!(timeline.events.len(), 5);
	assert!(timeline.total_duration_ms >= 15); // At least 15ms of delays
}

/// Test that tracer handles custom events with arbitrary JSON details.
/// **Why Important**: Custom events allow recording application-specific
/// information for advanced debugging scenarios.
#[test]
fn test_custom_event_details() {
	let mut tracer = QueryTracer::new("Q-custom", None);

	tracer.record_event(
		"custom_processing",
		serde_json::json!({
				"stage": "optimization",
				"metrics": {
						"cache_hits": 42,
						"cache_misses": 8,
				},
				"duration_ms": 123,
		}),
	);

	assert_eq!(tracer.events.len(), 2); // created + custom
	let custom_event = &tracer.events[1];
	assert_eq!(custom_event.event_type, "custom_processing");

	// Verify details can be accessed
	let details = &custom_event.details;
	assert_eq!(details["stage"].as_str(), Some("optimization"));
	assert_eq!(details["metrics"]["cache_hits"].as_i64(), Some(42));
}

/// Test that multiple concurrent traces are stored independently.
/// **Why Important**: Ensures that multiple queries can be traced
/// simultaneously without interference.
#[tokio::test]
async fn test_concurrent_trace_storage() {
	let store = QueryTraceStore::new(100);

	let mut handles = vec![];

	// Create and store multiple traces concurrently
	for i in 0..10 {
		let store_clone = store.clone();
		let handle = tokio::spawn(async move {
			let mut tracer = QueryTracer::new(format!("Q-concurrent-{i}"), Some(format!("session-{i}")));
			tracer.record_sent(5);
			tokio::time::sleep(Duration::from_millis(5)).await;
			tracer.record_response_received("ok");
			store_clone.store(tracer).await;
		});
		handles.push(handle);
	}

	// Wait for all tasks to complete
	for handle in handles {
		handle.await.unwrap();
	}

	let stats = store.get_stats().await;
	assert_eq!(stats.total_traces, 10);
}

/// Test that trace ID uniqueness is guaranteed.
/// **Why Important**: Unique trace IDs are essential for correlating
/// logs and debugging distributed queries.
#[test]
fn test_trace_id_uniqueness() {
	let mut trace_ids = std::collections::HashSet::new();

	for _ in 0..100 {
		let tracer = QueryTracer::new("Q-unique", None);
		let id = tracer.trace_id.as_str().to_string();
		assert!(!trace_ids.contains(&id), "Duplicate trace ID found: {id}");
		trace_ids.insert(id);
	}

	assert_eq!(trace_ids.len(), 100);
}

/// Test that slow trace information includes all required fields.
/// **Why Important**: Complete slow trace info enables comprehensive
/// performance monitoring and alerting.
#[tokio::test]
async fn test_slow_trace_info_completeness() {
	let store = QueryTraceStore::new(100);

	let mut tracer = QueryTracer::new("Q-slow-info", Some("session-perf".to_string()));
	tokio::time::sleep(Duration::from_millis(100)).await;
	tracer.record_sent(30);
	tracer.record_response_received("ok");

	store.store(tracer).await;

	let slow_traces = store.get_slow_traces(Duration::from_millis(50)).await;
	assert_eq!(slow_traces.len(), 1);

	let info = &slow_traces[0];
	assert!(!info.trace_id.is_empty());
	assert_eq!(info.query_id, "Q-slow-info");
	assert_eq!(info.session_id, Some("session-perf".to_string()));
	assert!(info.total_duration_ms >= 100);
	assert!(!info.has_error);
	assert!(info.event_count > 0);
}
