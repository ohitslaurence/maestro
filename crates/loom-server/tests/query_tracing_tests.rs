// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! Tests for query tracing and debugging infrastructure.
//!
//! Tests the QueryTracer and QueryTraceStore components that provide
//! comprehensive tracing of server-to-client queries for debugging.

use loom_server::{QueryTraceStore, QueryTracer, TraceTimeline};
use std::time::Duration;

/// Test that trace IDs are created uniquely.
/// This test ensures each trace has a distinct identifier for proper correlation.
#[test]
fn test_trace_id_uniqueness() {
	let tracer1 = QueryTracer::new("Q-123", None);
	let tracer2 = QueryTracer::new("Q-456", None);
	assert_ne!(tracer1.trace_id, tracer2.trace_id);
}

/// Test that a tracer records the creation event.
/// This is important for timeline start point tracking.
#[test]
fn test_tracer_creation_event() {
	let tracer = QueryTracer::new("Q-123", Some("session-1".to_string()));
	assert_eq!(tracer.events.len(), 1);
	assert_eq!(tracer.events[0].event_type, "created");
}

/// Test that events are recorded in sequence with correct ordering.
/// This property is essential for tracing chronological query flow.
#[test]
fn test_event_sequence() {
	let mut tracer = QueryTracer::new("Q-123", None);
	tracer.record_sent(5);
	tracer.record_response_received("ok");
	tracer.record_event("completed", serde_json::json!({}));

	assert_eq!(tracer.events.len(), 4);
	assert_eq!(tracer.events[0].event_type, "created");
	assert_eq!(tracer.events[0].sequence, 0);
	assert_eq!(tracer.events[1].event_type, "sent");
	assert_eq!(tracer.events[1].sequence, 1);
	assert_eq!(tracer.events[2].event_type, "response_received");
	assert_eq!(tracer.events[2].sequence, 2);
	assert_eq!(tracer.events[3].event_type, "completed");
	assert_eq!(tracer.events[3].sequence, 3);
}

/// Test that error detection works correctly.
/// This enables identifying failed queries in the trace store.
#[test]
fn test_error_detection() {
	let mut tracer = QueryTracer::new("Q-123", None);
	assert!(!tracer.has_error());

	tracer.record_error("connection failed", None);
	assert!(tracer.has_error());
}

/// Test that timeout detection works correctly.
/// Important for identifying queries that exceed timeout limits.
#[test]
fn test_timeout_detection() {
	let mut tracer = QueryTracer::new("Q-123", None);
	assert!(!tracer.has_error());

	tracer.record_timeout(5);
	assert!(tracer.has_error());
}

/// Test that duration calculation is accurate between events.
/// Critical for performance analysis and bottleneck identification.
#[tokio::test]
async fn test_event_duration_calculation() {
	let mut tracer = QueryTracer::new("Q-123", None);
	tokio::time::sleep(Duration::from_millis(10)).await;
	tracer.record_sent(5);
	tokio::time::sleep(Duration::from_millis(10)).await;
	tracer.record_response_received("ok");

	let duration = tracer.event_duration(0, 1);
	assert!(duration.is_some());
	let duration = duration.unwrap();
	// Allow some timing variance
	assert!(duration.as_millis() >= 5);
	assert!(duration.as_millis() <= 100);
}

/// Test that trace store correctly stores and retrieves traces.
/// This is foundational for the debugging endpoint.
#[tokio::test]
async fn test_trace_store_basic() {
	let store = QueryTraceStore::new(100);
	let tracer = QueryTracer::new("Q-123", Some("session-1".to_string()));
	let trace_id = tracer.trace_id.as_str().to_string();

	store.store(tracer).await;

	let retrieved = store.get(&trace_id).await;
	assert!(retrieved.is_some());
	assert_eq!(retrieved.unwrap().query_id, "Q-123");
}

/// Test that trace store respects the max traces capacity.
/// Prevents unbounded memory growth in production.
#[tokio::test]
async fn test_trace_store_capacity() {
	let store = QueryTraceStore::new(3);

	// Add 5 traces, but capacity is 3
	for i in 0..5 {
		let tracer = QueryTracer::new(format!("Q-{i}"), None);
		store.store(tracer).await;
	}

	let stats = store.get_stats().await;
	assert_eq!(stats.total_traces, 3);
}

/// Test that trace store can filter by session ID.
/// Important for isolating traces by client session.
#[tokio::test]
async fn test_trace_store_session_filtering() {
	let store = QueryTraceStore::new(100);

	let tracer1 = QueryTracer::new("Q-1", Some("session-1".to_string()));
	let tracer2 = QueryTracer::new("Q-2", Some("session-1".to_string()));
	let tracer3 = QueryTracer::new("Q-3", Some("session-2".to_string()));

	store.store(tracer1).await;
	store.store(tracer2).await;
	store.store(tracer3).await;

	let session_1_traces = store.get_session_traces("session-1").await;
	assert_eq!(session_1_traces.len(), 2);

	let session_2_traces = store.get_session_traces("session-2").await;
	assert_eq!(session_2_traces.len(), 1);
}

/// Test that slow trace detection works correctly.
/// Enables identifying performance bottlenecks.
#[tokio::test]
async fn test_slow_trace_detection() {
	let store = QueryTraceStore::new(100);

	let mut slow_tracer = QueryTracer::new("Q-slow", None);
	tokio::time::sleep(Duration::from_millis(100)).await;
	slow_tracer.record_sent(5);
	tokio::time::sleep(Duration::from_millis(50)).await;
	slow_tracer.record_response_received("ok");

	store.store(slow_tracer).await;

	let slow_traces = store.get_slow_traces(Duration::from_millis(50)).await;
	assert_eq!(slow_traces.len(), 1);
}

/// Test that timeline view correctly formats trace data.
/// This is used by the debug endpoint for human-readable output.
#[test]
fn test_timeline_generation() {
	let mut tracer = QueryTracer::new("Q-123", Some("session-1".to_string()));
	tracer.record_sent(5);
	tracer.record_response_received("ok");

	let timeline = TraceTimeline::from_tracer(&tracer);
	assert_eq!(timeline.query_id, "Q-123");
	assert_eq!(timeline.session_id, Some("session-1".to_string()));
	assert_eq!(timeline.events.len(), 3);
	assert!(!timeline.has_error);
}

/// Test that trace store statistics are computed correctly.
/// Critical for monitoring trace store health.
#[tokio::test]
async fn test_trace_store_stats() {
	let store = QueryTraceStore::new(100);

	let mut tracer1 = QueryTracer::new("Q-1", None);
	tracer1.record_sent(5);
	tracer1.record_response_received("ok");

	let mut tracer2 = QueryTracer::new("Q-2", None);
	tracer2.record_error("failed", None);

	store.store(tracer1).await;
	store.store(tracer2).await;

	let stats = store.get_stats().await;
	assert_eq!(stats.total_traces, 2);
	assert_eq!(stats.traces_with_errors, 1);
	assert!(stats.avg_events_per_trace > 0);
}

/// Test that traces with multiple events maintain correct sequence.
/// This is important for complex query lifecycles.
#[test]
fn test_multi_event_trace() {
	let mut tracer = QueryTracer::new("Q-123", None);
	for i in 0..5 {
		tracer.record_event(format!("event-{i}"), serde_json::json!({ "index": i }));
	}

	assert_eq!(tracer.events.len(), 6); // 1 creation + 5 custom
	for (i, event) in tracer.events.iter().enumerate() {
		assert_eq!(event.sequence as usize, i);
	}
}

/// Test that trace ID can be accessed as a string.
/// Required for HTTP endpoint path matching.
#[test]
fn test_trace_id_string_access() {
	let tracer = QueryTracer::new("Q-123", None);
	let id_str = tracer.trace_id.as_str();
	assert!(id_str.starts_with("TRACE-"));
}

/// Test complete trace lifecycle from creation to completion.
/// This demonstrates the typical query tracing workflow.
#[tokio::test]
async fn test_complete_trace_lifecycle() {
	let store = QueryTraceStore::new(100);

	let mut tracer = QueryTracer::new("Q-lifecycle", Some("session-test".to_string()));
	let trace_id = tracer.trace_id.as_str().to_string();

	// Simulate query lifecycle
	tracer.record_sent(10);
	tokio::time::sleep(Duration::from_millis(5)).await;
	tracer.record_response_received("success");
	tracer.record_event("validated", serde_json::json!({"status": "ok"}));
	tracer.record_event("completed", serde_json::json!({}));

	// Store the trace
	store.store(tracer.clone()).await;

	// Retrieve and verify
	let retrieved = store.get(&trace_id).await;
	assert!(retrieved.is_some());
	let retrieved = retrieved.unwrap();
	assert_eq!(retrieved.query_id, "Q-lifecycle");
	assert_eq!(retrieved.session_id, Some("session-test".to_string()));
	assert_eq!(retrieved.events.len(), 5); // created + sent + response + validated + completed
	assert!(!retrieved.has_error());

	// Verify timeline generation
	let timeline = TraceTimeline::from_tracer(&retrieved);
	assert_eq!(timeline.events.len(), 5);
	assert!(timeline.total_duration_ms >= 5);
}

/// Test that trace overhead is minimal (near zero overhead).
/// Ensures tracing doesn't significantly impact performance.
#[tokio::test]
async fn test_trace_overhead_minimal() {
	let store = QueryTraceStore::new(100);
	let start = std::time::Instant::now();

	for i in 0..1000 {
		let mut tracer = QueryTracer::new(format!("Q-perf-{i}"), Some("perf-test".to_string()));
		tracer.record_sent(5);
		tracer.record_response_received("ok");
		store.store(tracer).await;
	}

	let elapsed = start.elapsed();
	// Creating and storing 1000 traces should be fast
	// Allow up to 100ms total (0.1ms per trace)
	assert!(elapsed.as_millis() < 100);
}

/// Test clear functionality removes all traces.
/// Important for memory management and testing.
#[tokio::test]
async fn test_trace_store_clear() {
	let store = QueryTraceStore::new(100);

	for i in 0..5 {
		let tracer = QueryTracer::new(format!("Q-clear-{i}"), None);
		store.store(tracer).await;
	}

	let stats_before = store.get_stats().await;
	assert_eq!(stats_before.total_traces, 5);

	store.clear().await;

	let stats_after = store.get_stats().await;
	assert_eq!(stats_after.total_traces, 0);
}

/// Test that trace events include proper timestamps.
/// Needed for generating accurate timelines.
#[test]
fn test_trace_event_timestamps() {
	let mut tracer = QueryTracer::new("Q-time", None);
	assert!(tracer.events[0].timestamp <= chrono::Utc::now());

	tracer.record_sent(5);
	assert!(tracer.events[1].timestamp <= chrono::Utc::now());
	assert!(tracer.events[1].timestamp >= tracer.events[0].timestamp);
}
