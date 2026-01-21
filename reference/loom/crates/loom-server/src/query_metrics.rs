// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights
// reserved. SPDX-License-Identifier: Proprietary

//! Query metrics collection and export for monitoring and observability.
//!
//! This module provides Prometheus metrics for tracking server query
//! operations, including throughput, latency, success/failure rates, and
//! pending query count.

use prometheus::{
	Counter, CounterVec, Encoder, Gauge, Histogram, HistogramVec, IntGaugeVec, Registry, TextEncoder,
};
use std::sync::Arc;
use tracing::error;

/// Query metrics collector for monitoring query operations.
///
/// Purpose: Track query lifecycle metrics including sent, succeeded, failed,
/// latency, pending count, and timeouts with labels for query type, session,
/// and error classification.
pub struct QueryMetrics {
	/// Counter: total queries sent to client
	queries_sent_total: Counter,

	/// Counter: total queries succeeded (client returned result)
	queries_succeeded_total: Counter,

	/// Counter: total queries failed (client returned error)
	queries_failed_total: Counter,

	/// Histogram: query latency in seconds (send to response)
	query_latency_seconds: Histogram,

	/// Gauge: number of queries currently pending awaiting response
	queries_pending: Gauge,

	/// Counter with labels: timeouts by query type
	query_timeouts_total: CounterVec,

	/// Counter with labels: succeeded queries by type and session
	queries_success_by_type: CounterVec,

	/// Counter with labels: failed queries by type and error
	queries_failure_by_type: CounterVec,

	/// Histogram with labels: latency by query type
	query_latency_by_type: HistogramVec,

	/// Gauge with labels: pending queries by type
	pending_by_type: IntGaugeVec,

	/// Registry for metrics
	registry: Arc<Registry>,
}

impl QueryMetrics {
	/// Create a new QueryMetrics instance with Prometheus descriptors.
	///
	/// # Errors
	/// Returns an error if metric registration fails.
	pub fn new() -> Result<Self, prometheus::Error> {
		let registry = Arc::new(Registry::new());

		// Counter: total queries sent
		let queries_sent_total = Counter::with_opts(
			prometheus::Opts::new(
				"loom_queries_sent_total",
				"Total number of queries sent from server to client",
			)
			.namespace("loom")
			.subsystem("server"),
		)?;
		registry.register(Box::new(queries_sent_total.clone()))?;

		// Counter: total queries succeeded
		let queries_succeeded_total = Counter::with_opts(
			prometheus::Opts::new(
				"loom_queries_succeeded_total",
				"Total number of queries that received successful response",
			)
			.namespace("loom")
			.subsystem("server"),
		)?;
		registry.register(Box::new(queries_succeeded_total.clone()))?;

		// Counter: total queries failed
		let queries_failed_total = Counter::with_opts(
			prometheus::Opts::new(
				"loom_queries_failed_total",
				"Total number of queries that failed or timed out",
			)
			.namespace("loom")
			.subsystem("server"),
		)?;
		registry.register(Box::new(queries_failed_total.clone()))?;

		// Histogram: query latency
		let query_latency_seconds = Histogram::with_opts(
			prometheus::HistogramOpts::new(
				"loom_query_latency_seconds",
				"Query latency in seconds from send to response",
			)
			.namespace("loom")
			.subsystem("server")
			.buckets(vec![0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0]),
		)?;
		registry.register(Box::new(query_latency_seconds.clone()))?;

		// Gauge: pending queries
		let queries_pending = Gauge::with_opts(
			prometheus::Opts::new(
				"loom_queries_pending",
				"Number of queries currently pending response from client",
			)
			.namespace("loom")
			.subsystem("server"),
		)?;
		registry.register(Box::new(queries_pending.clone()))?;

		// CounterVec: timeouts by query type
		let query_timeouts_total = CounterVec::new(
			prometheus::Opts::new(
				"loom_query_timeouts_total",
				"Total number of query timeouts by type",
			)
			.namespace("loom")
			.subsystem("server"),
			&["query_type"],
		)?;
		registry.register(Box::new(query_timeouts_total.clone()))?;

		// CounterVec: success by type
		let queries_success_by_type = CounterVec::new(
			prometheus::Opts::new(
				"loom_queries_success_by_type",
				"Successful queries by type and session",
			)
			.namespace("loom")
			.subsystem("server"),
			&["query_type", "session_id"],
		)?;
		registry.register(Box::new(queries_success_by_type.clone()))?;

		// CounterVec: failure by type and error
		let queries_failure_by_type = CounterVec::new(
			prometheus::Opts::new(
				"loom_queries_failure_by_type",
				"Failed queries by type and error type",
			)
			.namespace("loom")
			.subsystem("server"),
			&["query_type", "error_type"],
		)?;
		registry.register(Box::new(queries_failure_by_type.clone()))?;

		// HistogramVec: latency by type
		let query_latency_by_type = HistogramVec::new(
			prometheus::HistogramOpts::new(
				"loom_query_latency_by_type_seconds",
				"Query latency by type",
			)
			.namespace("loom")
			.subsystem("server")
			.buckets(vec![0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0]),
			&["query_type"],
		)?;
		registry.register(Box::new(query_latency_by_type.clone()))?;

		// IntGaugeVec: pending by type
		let pending_by_type = IntGaugeVec::new(
			prometheus::Opts::new("loom_queries_pending_by_type", "Pending queries by type")
				.namespace("loom")
				.subsystem("server"),
			&["query_type"],
		)?;
		registry.register(Box::new(pending_by_type.clone()))?;

		Ok(Self {
			queries_sent_total,
			queries_succeeded_total,
			queries_failed_total,
			query_latency_seconds,
			queries_pending,
			query_timeouts_total,
			queries_success_by_type,
			queries_failure_by_type,
			query_latency_by_type,
			pending_by_type,
			registry,
		})
	}

	/// Record a query being sent to the client.
	///
	/// # Arguments
	/// * `query_type` - Type of query (read_file, env, workspace, etc.)
	/// * `session_id` - Session identifier for tracing
	pub fn record_sent(&self, query_type: &str, session_id: &str) {
		self.queries_sent_total.inc();
		self.queries_pending.inc();
		self.pending_by_type.with_label_values(&[query_type]).inc();

		tracing::debug!(
			query_type = query_type,
			session_id = session_id,
			"recorded query sent"
		);
	}

	/// Record a successful query response.
	///
	/// # Arguments
	/// * `query_type` - Type of query
	/// * `session_id` - Session identifier
	/// * `latency_secs` - Latency from send to response in seconds
	pub fn record_success(&self, query_type: &str, session_id: &str, latency_secs: f64) {
		self.queries_succeeded_total.inc();
		self.queries_pending.dec();
		self.pending_by_type.with_label_values(&[query_type]).dec();

		self.query_latency_seconds.observe(latency_secs);
		self
			.query_latency_by_type
			.with_label_values(&[query_type])
			.observe(latency_secs);

		self
			.queries_success_by_type
			.with_label_values(&[query_type, session_id])
			.inc();

		tracing::debug!(
			query_type = query_type,
			session_id = session_id,
			latency_secs = latency_secs,
			"recorded query success"
		);
	}

	/// Record a failed query (error or timeout).
	///
	/// # Arguments
	/// * `query_type` - Type of query
	/// * `error_type` - Type of error (timeout, network, invalid_response, etc.)
	/// * `session_id` - Session identifier
	pub fn record_failure(&self, query_type: &str, error_type: &str, session_id: &str) {
		self.queries_failed_total.inc();
		self.queries_pending.dec();
		self.pending_by_type.with_label_values(&[query_type]).dec();

		self
			.queries_failure_by_type
			.with_label_values(&[query_type, error_type])
			.inc();

		if error_type == "timeout" {
			self
				.query_timeouts_total
				.with_label_values(&[query_type])
				.inc();
		}

		tracing::debug!(
			query_type = query_type,
			error_type = error_type,
			session_id = session_id,
			"recorded query failure"
		);
	}

	/// Record latency for a query.
	///
	/// # Arguments
	/// * `query_type` - Type of query
	/// * `latency_secs` - Latency in seconds
	pub fn record_latency(&self, query_type: &str, latency_secs: f64) {
		self.query_latency_seconds.observe(latency_secs);
		self
			.query_latency_by_type
			.with_label_values(&[query_type])
			.observe(latency_secs);
	}

	/// Set the number of pending queries.
	///
	/// # Arguments
	/// * `count` - Number of pending queries
	pub fn set_pending_count(&self, count: i64) {
		self.queries_pending.set(count as f64);
	}

	/// Get Prometheus metrics in text format for export.
	///
	/// # Returns
	/// Text-formatted Prometheus metrics
	pub fn gather_metrics(&self) -> Result<String, prometheus::Error> {
		let metrics = self.registry.gather();
		let encoder = TextEncoder::new();
		let mut buf = Vec::new();
		encoder.encode(&metrics, &mut buf).map_err(|e| {
			error!(error = %e, "failed to encode metrics");
			prometheus::Error::Msg(format!("Failed to encode metrics: {e}"))
		})?;
		Ok(String::from_utf8_lossy(&buf).to_string())
	}
}

impl Default for QueryMetrics {
	fn default() -> Self {
		Self::new().expect("failed to create default QueryMetrics")
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_metrics_creation() {
		let metrics = QueryMetrics::new();
		assert!(metrics.is_ok());
	}

	#[test]
	fn test_record_sent_increments_counters() {
		let metrics = QueryMetrics::new().unwrap();

		metrics.record_sent("read_file", "session-123");
		metrics.record_sent("env", "session-123");

		// Verify by encoding metrics and checking output
		let output = metrics.gather_metrics().unwrap();
		assert!(output.contains("loom_queries_sent_total 2"));
		assert!(output.contains("loom_queries_pending 2"));
	}

	#[test]
	fn test_record_success_updates_metrics() {
		let metrics = QueryMetrics::new().unwrap();

		metrics.record_sent("read_file", "session-123");
		metrics.record_success("read_file", "session-123", 0.5);

		let output = metrics.gather_metrics().unwrap();
		assert!(output.contains("loom_queries_succeeded_total 1"));
		assert!(output.contains("loom_queries_pending 0"));
		assert!(output.contains("loom_queries_success_by_type"));
	}

	#[test]
	fn test_record_failure_updates_metrics() {
		let metrics = QueryMetrics::new().unwrap();

		metrics.record_sent("env", "session-123");
		metrics.record_failure("env", "network", "session-123");

		let output = metrics.gather_metrics().unwrap();
		assert!(output.contains("loom_queries_failed_total 1"));
		assert!(output.contains("loom_queries_pending 0"));
		assert!(output.contains("loom_queries_failure_by_type"));
	}

	#[test]
	fn test_timeout_counter() {
		let metrics = QueryMetrics::new().unwrap();

		metrics.record_sent("workspace", "session-123");
		metrics.record_failure("workspace", "timeout", "session-123");

		let output = metrics.gather_metrics().unwrap();
		assert!(output.contains("loom_query_timeouts_total"));
	}

	#[test]
	fn test_latency_recorded_in_histogram() {
		let metrics = QueryMetrics::new().unwrap();

		metrics.record_latency("read_file", 0.25);
		metrics.record_latency("read_file", 0.75);

		let output = metrics.gather_metrics().unwrap();
		assert!(output.contains("loom_query_latency_seconds"));
		assert!(output.contains("loom_query_latency_by_type_seconds"));
	}

	#[test]
	fn test_pending_by_type() {
		let metrics = QueryMetrics::new().unwrap();

		metrics.record_sent("read_file", "session-1");
		metrics.record_sent("read_file", "session-1");
		metrics.record_sent("env", "session-1");

		let output = metrics.gather_metrics().unwrap();
		assert!(output.contains("loom_queries_pending_by_type"));
	}

	#[test]
	fn test_labels_set_properly() {
		let metrics = QueryMetrics::new().unwrap();

		// Use unique session IDs to avoid flakiness from parallel test runs
		let session_1 = format!(
			"session-123-{}",
			std::time::SystemTime::now()
				.duration_since(std::time::UNIX_EPOCH)
				.unwrap()
				.as_nanos()
		);
		let session_2 = format!(
			"session-456-{}",
			std::time::SystemTime::now()
				.duration_since(std::time::UNIX_EPOCH)
				.unwrap()
				.as_nanos()
		);

		metrics.record_sent("workspace", &session_1);
		metrics.record_success("workspace", &session_1, 1.0);
		metrics.record_sent("env", &session_2);
		metrics.record_failure("env", "timeout", &session_2);

		let output = metrics.gather_metrics().unwrap();
		// Check that the metrics output contains the expected labels
		// We verify query_type labels are always present
		assert!(
			output.contains("query_type=\"workspace\""),
			"Output should contain workspace query type label"
		);
		assert!(
			output.contains("query_type=\"env\""),
			"Output should contain env query type label"
		);
		// Check error_type label (always present for failures)
		assert!(
			output.contains("error_type=\"timeout\""),
			"Output should contain timeout error type label"
		);
		// Verify success metrics with session label are recorded
		assert!(
			output.contains("loom_queries_success_by_type"),
			"Output should contain success metrics with labels"
		);
		assert!(
			output.contains("loom_queries_failure_by_type"),
			"Output should contain failure metrics with labels"
		);
	}

	#[test]
	fn test_set_pending_count() {
		let metrics = QueryMetrics::new().unwrap();
		metrics.set_pending_count(5);

		let output = metrics.gather_metrics().unwrap();
		assert!(output.contains("loom_queries_pending 5"));
	}

	#[test]
	fn test_multiple_error_types() {
		let metrics = QueryMetrics::new().unwrap();

		metrics.record_sent("read_file", "session-1");
		metrics.record_failure("read_file", "network", "session-1");

		metrics.record_sent("read_file", "session-2");
		metrics.record_failure("read_file", "invalid_response", "session-2");

		let output = metrics.gather_metrics().unwrap();
		assert!(output.contains("error_type=\"network\""));
		assert!(output.contains("error_type=\"invalid_response\""));
	}

	#[test]
	fn test_concurrent_metric_updates() {
		let metrics = Arc::new(QueryMetrics::new().unwrap());
		let mut handles = vec![];

		for i in 0..10 {
			let metrics_clone = Arc::clone(&metrics);
			let handle = std::thread::spawn(move || {
				let query_type = if i % 2 == 0 { "read_file" } else { "env" };
				metrics_clone.record_sent(query_type, &format!("session-{i}"));
				metrics_clone.record_success(query_type, &format!("session-{i}"), 0.1 * (i as f64));
			});
			handles.push(handle);
		}

		for handle in handles {
			handle.join().unwrap();
		}

		let output = metrics.gather_metrics().unwrap();
		assert!(output.contains("loom_queries_sent_total 10"));
		assert!(output.contains("loom_queries_succeeded_total 10"));
	}
}
