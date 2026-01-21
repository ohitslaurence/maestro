// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use prometheus::{
	Counter, CounterVec, Encoder, Gauge, GaugeVec, Histogram, HistogramOpts, HistogramVec, Opts,
	Registry, TextEncoder,
};

use crate::events::WeaverAuditEventType;

// Prometheus metrics - fields are registered with the registry but may not be read directly.
// The registry owns them and exposes them via encode().
#[allow(dead_code)]
pub struct Metrics {
	registry: Registry,

	pub events_captured: CounterVec,
	pub events_sent: CounterVec,
	pub events_dropped: CounterVec,
	pub events_buffered: CounterVec,

	pub batches_sent: CounterVec,
	pub batch_events: Histogram,
	pub batch_size_bytes: Histogram,
	pub pipeline_events_in_flight: Gauge,

	pub buffer_events: Gauge,
	pub buffer_bytes: Gauge,
	pub buffer_oldest_event_age_seconds: Gauge,

	pub ebpf_programs_attached: GaugeVec,
	pub ebpf_ring_buffer_dropped: Counter,
	pub ebpf_ring_buffer_utilization: Gauge,

	pub http_requests: CounterVec,
	pub http_request_duration: HistogramVec,
	pub http_retries: CounterVec,
}

impl Default for Metrics {
	fn default() -> Self {
		Self::new()
	}
}

impl Metrics {
	pub fn new() -> Self {
		let registry = Registry::new();

		let events_captured = CounterVec::new(
			Opts::new(
				"loom_weaver_audit_events_captured_total",
				"Total events captured",
			),
			&["event_type"],
		)
		.unwrap();
		registry
			.register(Box::new(events_captured.clone()))
			.unwrap();

		let events_sent = CounterVec::new(
			Opts::new(
				"loom_weaver_audit_events_sent_total",
				"Total events sent to server",
			),
			&["event_type"],
		)
		.unwrap();
		registry.register(Box::new(events_sent.clone())).unwrap();

		let events_dropped = CounterVec::new(
			Opts::new(
				"loom_weaver_audit_events_dropped_total",
				"Total events dropped",
			),
			&["stage", "reason"],
		)
		.unwrap();
		registry.register(Box::new(events_dropped.clone())).unwrap();

		let events_buffered = CounterVec::new(
			Opts::new(
				"loom_weaver_audit_events_buffered_total",
				"Total events buffered locally",
			),
			&["event_type"],
		)
		.unwrap();
		registry
			.register(Box::new(events_buffered.clone()))
			.unwrap();

		let batches_sent = CounterVec::new(
			Opts::new("loom_weaver_audit_batches_sent_total", "Total batches sent"),
			&["outcome"],
		)
		.unwrap();
		registry.register(Box::new(batches_sent.clone())).unwrap();

		let batch_events = Histogram::with_opts(
			HistogramOpts::new("loom_weaver_audit_batch_events", "Events per batch").buckets(vec![
				1.0, 5.0, 10.0, 25.0, 50.0, 100.0, 250.0, 500.0, 1000.0,
			]),
		)
		.unwrap();
		registry.register(Box::new(batch_events.clone())).unwrap();

		let batch_size_bytes = Histogram::with_opts(
			HistogramOpts::new("loom_weaver_audit_batch_size_bytes", "Batch size in bytes").buckets(
				vec![
					100.0, 500.0, 1000.0, 5000.0, 10000.0, 50000.0, 100000.0, 500000.0,
				],
			),
		)
		.unwrap();
		registry
			.register(Box::new(batch_size_bytes.clone()))
			.unwrap();

		let pipeline_events_in_flight = Gauge::new(
			"loom_weaver_audit_pipeline_events_in_flight",
			"Events currently in processing pipeline",
		)
		.unwrap();
		registry
			.register(Box::new(pipeline_events_in_flight.clone()))
			.unwrap();

		let buffer_events =
			Gauge::new("loom_weaver_audit_buffer_events", "Events in local buffer").unwrap();
		registry.register(Box::new(buffer_events.clone())).unwrap();

		let buffer_bytes = Gauge::new(
			"loom_weaver_audit_buffer_bytes",
			"Bytes used by local buffer",
		)
		.unwrap();
		registry.register(Box::new(buffer_bytes.clone())).unwrap();

		let buffer_oldest_event_age_seconds = Gauge::new(
			"loom_weaver_audit_buffer_oldest_event_age_seconds",
			"Age of oldest buffered event in seconds",
		)
		.unwrap();
		registry
			.register(Box::new(buffer_oldest_event_age_seconds.clone()))
			.unwrap();

		let ebpf_programs_attached = GaugeVec::new(
			Opts::new(
				"loom_weaver_audit_ebpf_programs_attached",
				"eBPF programs attached",
			),
			&["program"],
		)
		.unwrap();
		registry
			.register(Box::new(ebpf_programs_attached.clone()))
			.unwrap();

		let ebpf_ring_buffer_dropped = Counter::new(
			"loom_weaver_audit_ebpf_ring_buffer_dropped_events_total",
			"Events dropped from ring buffer",
		)
		.unwrap();
		registry
			.register(Box::new(ebpf_ring_buffer_dropped.clone()))
			.unwrap();

		let ebpf_ring_buffer_utilization = Gauge::new(
			"loom_weaver_audit_ebpf_ring_buffer_utilization_ratio",
			"Ring buffer utilization ratio",
		)
		.unwrap();
		registry
			.register(Box::new(ebpf_ring_buffer_utilization.clone()))
			.unwrap();

		let http_requests = CounterVec::new(
			Opts::new(
				"loom_weaver_audit_http_requests_total",
				"HTTP requests to server",
			),
			&["endpoint", "code"],
		)
		.unwrap();
		registry.register(Box::new(http_requests.clone())).unwrap();

		let http_request_duration = HistogramVec::new(
			HistogramOpts::new(
				"loom_weaver_audit_http_request_duration_seconds",
				"HTTP request duration",
			)
			.buckets(vec![
				0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0,
			]),
			&["endpoint"],
		)
		.unwrap();
		registry
			.register(Box::new(http_request_duration.clone()))
			.unwrap();

		let http_retries = CounterVec::new(
			Opts::new(
				"loom_weaver_audit_http_retries_total",
				"HTTP request retries",
			),
			&["reason"],
		)
		.unwrap();
		registry.register(Box::new(http_retries.clone())).unwrap();

		Metrics {
			registry,
			events_captured,
			events_sent,
			events_dropped,
			events_buffered,
			batches_sent,
			batch_events,
			batch_size_bytes,
			pipeline_events_in_flight,
			buffer_events,
			buffer_bytes,
			buffer_oldest_event_age_seconds,
			ebpf_programs_attached,
			ebpf_ring_buffer_dropped,
			ebpf_ring_buffer_utilization,
			http_requests,
			http_request_duration,
			http_retries,
		}
	}

	pub fn record_event_captured(&self, event_type: WeaverAuditEventType) {
		self
			.events_captured
			.with_label_values(&[event_type_label(event_type)])
			.inc();
	}

	pub fn record_event_sent(&self, event_type: WeaverAuditEventType) {
		self
			.events_sent
			.with_label_values(&[event_type_label(event_type)])
			.inc();
	}

	pub fn record_event_buffered(&self, event_type: WeaverAuditEventType) {
		self
			.events_buffered
			.with_label_values(&[event_type_label(event_type)])
			.inc();
	}

	pub fn record_batch_sent(&self, success: bool, event_count: usize, size_bytes: usize) {
		self
			.batches_sent
			.with_label_values(&[if success { "success" } else { "failure" }])
			.inc();
		self.batch_events.observe(event_count as f64);
		self.batch_size_bytes.observe(size_bytes as f64);
	}

	pub fn encode(&self) -> String {
		let encoder = TextEncoder::new();
		let metric_families = self.registry.gather();
		let mut buffer = Vec::new();
		encoder.encode(&metric_families, &mut buffer).unwrap();
		String::from_utf8(buffer).unwrap()
	}
}

fn event_type_label(event_type: WeaverAuditEventType) -> &'static str {
	match event_type {
		WeaverAuditEventType::ProcessExec => "process_exec",
		WeaverAuditEventType::ProcessFork => "process_fork",
		WeaverAuditEventType::ProcessExit => "process_exit",
		WeaverAuditEventType::FileWrite => "file_write",
		WeaverAuditEventType::FileRead => "file_read",
		WeaverAuditEventType::FileMetadata => "file_metadata",
		WeaverAuditEventType::FileOpen => "file_open",
		WeaverAuditEventType::NetworkSocket => "network_socket",
		WeaverAuditEventType::NetworkConnect => "network_connect",
		WeaverAuditEventType::NetworkListen => "network_listen",
		WeaverAuditEventType::NetworkAccept => "network_accept",
		WeaverAuditEventType::DnsQuery => "dns_query",
		WeaverAuditEventType::DnsResponse => "dns_response",
		WeaverAuditEventType::PrivilegeChange => "privilege_change",
		WeaverAuditEventType::MemoryExec => "memory_exec",
		WeaverAuditEventType::SandboxEscape => "sandbox_escape",
	}
}
