// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights
// reserved. SPDX-License-Identifier: Proprietary

//! Query tracing and debugging infrastructure.
//!
//! Provides comprehensive tracing of server-to-client queries for debugging and
//! performance analysis. Each query gets a unique trace ID and events are
//! recorded chronologically.
//!
//! # Example
//!
//! ```
//! use loom_server::query_tracing::{QueryTraceStore, QueryTracer};
//! use std::time::Duration;
//!
//! # #[tokio::main]
//! # async fn main() {
//! // Create a tracer for a query
//! let mut tracer = QueryTracer::new("Q-123", Some("session-1".to_string()));
//!
//! // Record events during query lifecycle
//! tracer.record_sent(10); // timeout of 10 seconds
//! tracer.record_response_received("ok");
//!
//! // Store traces for debugging
//! let store = QueryTraceStore::default();
//! let trace_id = tracer.trace_id.as_str().to_string();
//! store.store(tracer).await;
//!
//! // Retrieve traces for inspection
//! let retrieved = store.get(&trace_id).await;
//! assert!(retrieved.is_some());
//! # }
//! ```

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use tracing::instrument;
use uuid::Uuid;

/// Unique trace ID for query correlation
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TraceId(pub(crate) String);

impl TraceId {
	/// Generate a new trace ID
	pub fn new() -> Self {
		Self(format!("TRACE-{}", Uuid::new_v4()))
	}

	/// Get the trace ID as a string
	pub fn as_str(&self) -> &str {
		&self.0
	}
}

impl Default for TraceId {
	fn default() -> Self {
		Self::new()
	}
}

/// A single event in the query trace timeline
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceEvent {
	/// Timestamp of the event
	pub timestamp: DateTime<Utc>,

	/// Event type (e.g., "created", "sent", "response_received", "error",
	/// "timeout")
	pub event_type: String,

	/// Event sequence number for ordering
	pub sequence: u32,

	/// Event details (JSON object)
	pub details: serde_json::Value,
}

impl TraceEvent {
	/// Create a new trace event
	pub fn new(event_type: impl Into<String>, sequence: u32, details: serde_json::Value) -> Self {
		Self {
			timestamp: Utc::now(),
			event_type: event_type.into(),
			sequence,
			details,
		}
	}
}

/// Traces a single server query lifecycle
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryTracer {
	/// Unique trace ID for correlating all logs
	pub trace_id: TraceId,

	/// Query ID being traced
	pub query_id: String,

	/// Session ID for context
	pub session_id: Option<String>,

	/// Events recorded in order
	pub events: Vec<TraceEvent>,

	/// Next sequence number for events
	pub(crate) next_sequence: u32,
}

impl QueryTracer {
	/// Create a new query tracer
	pub fn new(query_id: impl Into<String>, session_id: Option<String>) -> Self {
		let query_id = query_id.into();
		let trace_id = TraceId::new();

		let mut tracer = Self {
			trace_id: trace_id.clone(),
			query_id: query_id.clone(),
			session_id: session_id.clone(),
			events: Vec::new(),
			next_sequence: 0,
		};

		// Record creation event
		let details = serde_json::json!({
				"query_id": query_id,
				"session_id": session_id,
		});
		tracer.record_event("created", details);

		tracer
	}

	/// Record an event in the trace
	pub fn record_event(&mut self, event_type: impl Into<String>, details: serde_json::Value) {
		let event_type_str: String = event_type.into();
		let event = TraceEvent::new(event_type_str.clone(), self.next_sequence, details);
		self.events.push(event);
		self.next_sequence += 1;

		tracing::debug!(
				trace_id = %self.trace_id.0,
				query_id = %self.query_id,
				event_type = %event_type_str,
				"trace event recorded"
		);
	}

	/// Record a "sent" event
	pub fn record_sent(&mut self, timeout_secs: u64) {
		let details = serde_json::json!({
				"timeout_secs": timeout_secs,
		});
		self.record_event("sent", details);
	}

	/// Record a "response_received" event
	pub fn record_response_received(&mut self, status: &str) {
		let details = serde_json::json!({
				"status": status,
		});
		self.record_event("response_received", details);
	}

	/// Record an "error" event
	pub fn record_error(&mut self, error: &str, details: Option<serde_json::Value>) {
		let details = details.unwrap_or_else(|| {
			serde_json::json!({
					"error": error,
			})
		});
		self.record_event("error", details);
	}

	/// Record a "timeout" event
	pub fn record_timeout(&mut self, timeout_secs: u64) {
		let details = serde_json::json!({
				"timeout_secs": timeout_secs,
		});
		self.record_event("timeout", details);
	}

	/// Get the total duration from creation to last event
	pub fn total_duration(&self) -> Duration {
		if self.events.len() < 2 {
			Duration::from_secs(0)
		} else {
			let first = &self.events[0].timestamp;
			let last = &self.events[self.events.len() - 1].timestamp;
			last
				.signed_duration_since(*first)
				.to_std()
				.unwrap_or(Duration::from_secs(0))
		}
	}

	/// Get the duration between two events
	pub fn event_duration(&self, from_index: usize, to_index: usize) -> Option<Duration> {
		if from_index >= self.events.len() || to_index >= self.events.len() || from_index >= to_index {
			return None;
		}

		let from = &self.events[from_index].timestamp;
		let to = &self.events[to_index].timestamp;
		to.signed_duration_since(*from).to_std().ok()
	}

	/// Check if this trace has any errors
	pub fn has_error(&self) -> bool {
		self
			.events
			.iter()
			.any(|e| e.event_type == "error" || e.event_type == "timeout")
	}

	/// Check if this trace is slow (exceeds threshold)
	pub fn is_slow(&self, threshold: Duration) -> bool {
		self.total_duration() > threshold
	}
}

/// Manages query traces for debugging
pub struct QueryTraceStore {
	traces: Arc<Mutex<std::collections::HashMap<String, QueryTracer>>>,
	max_traces: usize,
}

impl QueryTraceStore {
	/// Create a new trace store
	pub fn new(max_traces: usize) -> Self {
		Self {
			traces: Arc::new(Mutex::new(std::collections::HashMap::new())),
			max_traces,
		}
	}

	/// Store a trace
	#[instrument(skip(self, tracer), fields(trace_id = %tracer.trace_id.0))]
	pub async fn store(&self, tracer: QueryTracer) {
		let trace_id = tracer.trace_id.as_str().to_string();
		let mut traces = self.traces.lock().await;

		// Enforce max traces with simple LRU eviction
		if traces.len() >= self.max_traces {
			// Remove the oldest trace (first entry)
			if let Some(first_key) = traces.keys().next().cloned() {
				traces.remove(&first_key);
				tracing::debug!(
						evicted_trace = %first_key,
						"evicted oldest trace due to capacity"
				);
			}
		}

		traces.insert(trace_id, tracer);
		tracing::debug!(total_traces = traces.len(), "trace stored");
	}

	/// Retrieve a trace by ID
	#[instrument(skip(self), fields(trace_id = %trace_id))]
	pub async fn get(&self, trace_id: &str) -> Option<QueryTracer> {
		let traces = self.traces.lock().await;
		traces.get(trace_id).cloned()
	}

	/// List all trace IDs
	#[instrument(skip(self))]
	pub async fn list_trace_ids(&self) -> Vec<String> {
		let traces = self.traces.lock().await;
		traces.keys().cloned().collect()
	}

	/// Get traces for a specific session
	#[instrument(skip(self), fields(session_id = %session_id))]
	pub async fn get_session_traces(&self, session_id: &str) -> Vec<QueryTracer> {
		let traces = self.traces.lock().await;
		traces
			.values()
			.filter(|t| t.session_id.as_deref() == Some(session_id))
			.cloned()
			.collect()
	}

	/// Get all slow traces (with detailed information)
	#[instrument(skip(self))]
	pub async fn get_slow_traces(&self, threshold: Duration) -> Vec<SlowTraceInfo> {
		let traces = self.traces.lock().await;
		traces
			.values()
			.filter(|t| t.is_slow(threshold))
			.map(|t| SlowTraceInfo {
				trace_id: t.trace_id.0.clone(),
				query_id: t.query_id.clone(),
				session_id: t.session_id.clone(),
				total_duration_ms: t.total_duration().as_millis() as u64,
				event_count: t.events.len(),
				has_error: t.has_error(),
			})
			.collect()
	}

	/// Clear all traces
	#[instrument(skip(self))]
	pub async fn clear(&self) {
		let mut traces = self.traces.lock().await;
		let count = traces.len();
		traces.clear();
		tracing::info!(cleared_count = count, "all traces cleared");
	}

	/// Get statistics about stored traces
	#[instrument(skip(self))]
	pub async fn get_stats(&self) -> TraceStoreStats {
		let traces = self.traces.lock().await;
		let total_traces = traces.len();
		let traces_with_errors = traces.values().filter(|t| t.has_error()).count();
		let slow_traces = traces
			.values()
			.filter(|t| t.is_slow(Duration::from_secs(5)))
			.count();
		let avg_events = if total_traces > 0 {
			traces.values().map(|t| t.events.len()).sum::<usize>() / total_traces
		} else {
			0
		};

		TraceStoreStats {
			total_traces,
			traces_with_errors,
			slow_traces,
			avg_events_per_trace: avg_events,
		}
	}
}

impl Default for QueryTraceStore {
	fn default() -> Self {
		Self::new(10000) // Store up to 10k traces
	}
}

impl Clone for QueryTraceStore {
	fn clone(&self) -> Self {
		Self {
			traces: Arc::clone(&self.traces),
			max_traces: self.max_traces,
		}
	}
}

/// Information about a slow trace
#[derive(Debug, Serialize, Deserialize)]
pub struct SlowTraceInfo {
	pub trace_id: String,
	pub query_id: String,
	pub session_id: Option<String>,
	pub total_duration_ms: u64,
	pub event_count: usize,
	pub has_error: bool,
}

/// Statistics about the trace store
#[derive(Debug, Serialize, Deserialize)]
pub struct TraceStoreStats {
	pub total_traces: usize,
	pub traces_with_errors: usize,
	pub slow_traces: usize,
	pub avg_events_per_trace: usize,
}

/// Timeline view of a query trace
#[derive(Debug, Serialize, Deserialize)]
pub struct TraceTimeline {
	pub trace_id: String,
	pub query_id: String,
	pub session_id: Option<String>,
	pub total_duration_ms: u64,
	pub events: Vec<TimelineEvent>,
	pub has_error: bool,
	pub is_slow: bool,
}

/// A single event in the timeline
#[derive(Debug, Serialize, Deserialize)]
pub struct TimelineEvent {
	pub sequence: u32,
	pub event_type: String,
	pub timestamp: String,
	pub elapsed_ms: u64,
	pub details: serde_json::Value,
}

impl TraceTimeline {
	/// Create a timeline from a tracer
	pub fn from_tracer(tracer: &QueryTracer) -> Self {
		let total_duration = tracer.total_duration();
		let has_error = tracer.has_error();
		let is_slow = tracer.is_slow(Duration::from_secs(5));

		let mut elapsed_ms = 0u64;
		let events = tracer
			.events
			.iter()
			.enumerate()
			.map(|(i, event)| {
				let event_elapsed = if i == 0 {
					0
				} else {
					tracer
						.event_duration(0, i)
						.unwrap_or(Duration::from_secs(0))
						.as_millis() as u64
				};
				elapsed_ms = event_elapsed;

				TimelineEvent {
					sequence: event.sequence,
					event_type: event.event_type.clone(),
					timestamp: event.timestamp.to_rfc3339(),
					elapsed_ms: event_elapsed,
					details: event.details.clone(),
				}
			})
			.collect();

		Self {
			trace_id: tracer.trace_id.0.clone(),
			query_id: tracer.query_id.clone(),
			session_id: tracer.session_id.clone(),
			total_duration_ms: total_duration.as_millis() as u64,
			events,
			has_error,
			is_slow,
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	/// Test that trace IDs are created uniquely.
	/// This test ensures each trace has a distinct identifier for proper
	/// correlation.
	#[test]
	fn test_trace_id_uniqueness() {
		let id1 = TraceId::new();
		let id2 = TraceId::new();
		assert_ne!(id1, id2);
	}

	/// Test that a tracer records the creation event.
	/// This is important for timeline start point tracking.
	#[test]
	fn test_tracer_creation_event() {
		let tracer = QueryTracer::new("Q-123", Some("session-1".to_string()));
		assert_eq!(tracer.events.len(), 1);
		assert_eq!(tracer.events[0].event_type, "created");
		assert_eq!(tracer.next_sequence, 1);
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
		let trace_id = tracer.trace_id.0.clone();

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
		let id = TraceId::new();
		let s = id.as_str();
		assert!(s.starts_with("TRACE-"));
	}
}
