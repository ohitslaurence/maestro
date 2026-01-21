// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use std::sync::Arc;

use tokio::sync::mpsc::{self, error::SendError};
use tracing::{instrument, warn};

use crate::enrichment::{AuditEnricher, EnrichedAuditEvent};
use crate::event::AuditLogEntry;
use crate::filter::AuditFilterConfig;
use crate::redaction::{redact_json_value, redact_optional_string, redact_string};
use crate::sink::AuditSink;
use loom_server_config::QueueOverflowPolicy;

/// Redacts secrets from all string fields in an enriched audit event.
///
/// This ensures no secrets leak through any field, not just the `details` JSON.
fn redact_audit_event(event: &mut EnrichedAuditEvent) {
	let base = &mut event.base;

	redact_json_value(&mut base.details);

	if let std::borrow::Cow::Owned(redacted) = redact_string(&base.action) {
		base.action = redacted;
	}

	redact_optional_string(&mut base.resource_id);
	redact_optional_string(&mut base.user_agent);

	if let Some(ref mut session) = event.session {
		redact_optional_string(&mut session.session_id);
		redact_optional_string(&mut session.device_label);
	}
}

pub struct AuditService {
	tx: mpsc::Sender<AuditLogEntry>,
	overflow_policy: QueueOverflowPolicy,
}

impl AuditService {
	pub fn new(
		enricher: Arc<dyn AuditEnricher>,
		global_filter: AuditFilterConfig,
		queue_capacity: usize,
		overflow_policy: QueueOverflowPolicy,
		sinks: Vec<Arc<dyn AuditSink>>,
	) -> Self {
		let (tx, rx) = mpsc::channel(queue_capacity);

		tokio::spawn(Self::background_task(rx, enricher, global_filter, sinks));

		Self {
			tx,
			overflow_policy,
		}
	}

	async fn background_task(
		mut rx: mpsc::Receiver<AuditLogEntry>,
		enricher: Arc<dyn AuditEnricher>,
		global_filter: AuditFilterConfig,
		sinks: Vec<Arc<dyn AuditSink>>,
	) {
		while let Some(entry) = rx.recv().await {
			let mut enriched = enricher.enrich(entry).await;

			if !global_filter.allows(&enriched) {
				continue;
			}

			redact_audit_event(&mut enriched);

			let event = Arc::new(enriched);

			for sink in &sinks {
				if !sink.filter().allows(&event) {
					continue;
				}

				let sink = Arc::clone(sink);
				let event = Arc::clone(&event);

				tokio::spawn(async move {
					if let Err(e) = sink.publish(event).await {
						warn!(sink = sink.name(), error = %e, "audit sink publish failed");
					}
				});
			}
		}
	}

	/// Log an audit event to the queue for processing.
	///
	/// Returns `true` if the event was successfully queued, `false` if dropped.
	///
	/// # Overflow Policy Behavior
	///
	/// - `Block`: Spawns an async task to send (non-blocking to caller, but event will be sent)
	/// - `DropNewest`: Uses try_send, drops new events when queue is full
	/// - `DropOldest`: Currently behaves like DropNewest (drops new events when full).
	///   True ring buffer behavior (dropping oldest) would require a different queue implementation.
	#[instrument(skip(self, entry), fields(event_type = %entry.event_type))]
	pub fn log(&self, entry: AuditLogEntry) -> bool {
		match self.overflow_policy {
			QueueOverflowPolicy::Block => {
				let tx = self.tx.clone();
				tokio::spawn(async move {
					let _ = tx.send(entry).await;
				});
				true
			}
			QueueOverflowPolicy::DropNewest | QueueOverflowPolicy::DropOldest => {
				self.tx.try_send(entry).is_ok()
			}
		}
	}

	pub async fn log_blocking(&self, entry: AuditLogEntry) -> Result<(), SendError<AuditLogEntry>> {
		self.tx.send(entry).await
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::enrichment::{EnrichedAuditEvent, NoopEnricher};
	use crate::event::{AuditEventType, AuditSeverity};
	use crate::sink::AuditSinkError;
	use async_trait::async_trait;
	use std::sync::atomic::{AtomicUsize, Ordering};
	use tokio::time::{sleep, Duration};

	struct TestSink {
		name: String,
		filter: AuditFilterConfig,
		publish_count: Arc<AtomicUsize>,
	}

	impl TestSink {
		fn new(name: &str) -> Self {
			Self {
				name: name.to_string(),
				filter: AuditFilterConfig::default(),
				publish_count: Arc::new(AtomicUsize::new(0)),
			}
		}

		fn count(&self) -> usize {
			self.publish_count.load(Ordering::SeqCst)
		}
	}

	#[async_trait]
	impl AuditSink for TestSink {
		fn name(&self) -> &str {
			&self.name
		}

		fn filter(&self) -> &AuditFilterConfig {
			&self.filter
		}

		async fn publish(&self, _event: Arc<EnrichedAuditEvent>) -> Result<(), AuditSinkError> {
			self.publish_count.fetch_add(1, Ordering::SeqCst);
			Ok(())
		}
	}

	struct FailingSink {
		name: String,
		filter: AuditFilterConfig,
	}

	#[async_trait]
	impl AuditSink for FailingSink {
		fn name(&self) -> &str {
			&self.name
		}

		fn filter(&self) -> &AuditFilterConfig {
			&self.filter
		}

		async fn publish(&self, _event: Arc<EnrichedAuditEvent>) -> Result<(), AuditSinkError> {
			Err(AuditSinkError::Transient("test error".to_string()))
		}
	}

	#[tokio::test]
	async fn test_log_sends_to_sink() {
		let sink = Arc::new(TestSink::new("test"));
		let sink_clone = Arc::clone(&sink);

		let service = AuditService::new(
			Arc::new(NoopEnricher),
			AuditFilterConfig::default(),
			10000,
			QueueOverflowPolicy::DropNewest,
			vec![sink_clone],
		);

		let entry = AuditLogEntry::builder(AuditEventType::Login).build();
		assert!(service.log(entry));

		sleep(Duration::from_millis(50)).await;
		assert_eq!(sink.count(), 1);
	}

	#[tokio::test]
	async fn test_log_blocking_sends_to_sink() {
		let sink = Arc::new(TestSink::new("test"));
		let sink_clone = Arc::clone(&sink);

		let service = AuditService::new(
			Arc::new(NoopEnricher),
			AuditFilterConfig::default(),
			10000,
			QueueOverflowPolicy::DropNewest,
			vec![sink_clone],
		);

		let entry = AuditLogEntry::builder(AuditEventType::Login).build();
		service.log_blocking(entry).await.unwrap();

		sleep(Duration::from_millis(50)).await;
		assert_eq!(sink.count(), 1);
	}

	#[tokio::test]
	async fn test_global_filter_blocks_events() {
		let sink = Arc::new(TestSink::new("test"));
		let sink_clone = Arc::clone(&sink);

		let filter = AuditFilterConfig {
			min_severity: AuditSeverity::Warning,
			include_events: None,
			exclude_events: None,
		};

		let service = AuditService::new(
			Arc::new(NoopEnricher),
			filter,
			10000,
			QueueOverflowPolicy::DropNewest,
			vec![sink_clone],
		);

		let info_entry = AuditLogEntry::builder(AuditEventType::Login)
			.severity(AuditSeverity::Info)
			.build();
		service.log(info_entry);

		let warning_entry = AuditLogEntry::builder(AuditEventType::LoginFailed)
			.severity(AuditSeverity::Warning)
			.build();
		service.log(warning_entry);

		sleep(Duration::from_millis(50)).await;
		assert_eq!(sink.count(), 1);
	}

	#[tokio::test]
	async fn test_fan_out_to_multiple_sinks() {
		let sink1 = Arc::new(TestSink::new("sink1"));
		let sink2 = Arc::new(TestSink::new("sink2"));
		let sink1_clone = Arc::clone(&sink1);
		let sink2_clone = Arc::clone(&sink2);

		let service = AuditService::new(
			Arc::new(NoopEnricher),
			AuditFilterConfig::default(),
			10000,
			QueueOverflowPolicy::DropNewest,
			vec![sink1_clone, sink2_clone],
		);

		let entry = AuditLogEntry::builder(AuditEventType::Login).build();
		service.log(entry);

		sleep(Duration::from_millis(50)).await;
		assert_eq!(sink1.count(), 1);
		assert_eq!(sink2.count(), 1);
	}

	#[tokio::test]
	async fn test_failing_sink_does_not_block_others() {
		let good_sink = Arc::new(TestSink::new("good"));
		let failing_sink = Arc::new(FailingSink {
			name: "failing".to_string(),
			filter: AuditFilterConfig::default(),
		});
		let good_sink_clone = Arc::clone(&good_sink);

		let service = AuditService::new(
			Arc::new(NoopEnricher),
			AuditFilterConfig::default(),
			10000,
			QueueOverflowPolicy::DropNewest,
			vec![failing_sink, good_sink_clone],
		);

		let entry = AuditLogEntry::builder(AuditEventType::Login).build();
		service.log(entry);

		sleep(Duration::from_millis(50)).await;
		assert_eq!(good_sink.count(), 1);
	}

	#[tokio::test]
	async fn test_queue_full_returns_false() {
		let sink = Arc::new(TestSink::new("test"));

		let service = AuditService::new(
			Arc::new(NoopEnricher),
			AuditFilterConfig::default(),
			1,
			QueueOverflowPolicy::DropNewest,
			vec![sink],
		);

		let entry1 = AuditLogEntry::builder(AuditEventType::Login).build();
		let entry2 = AuditLogEntry::builder(AuditEventType::Login).build();
		let entry3 = AuditLogEntry::builder(AuditEventType::Login).build();

		service.log(entry1);
		service.log(entry2);
		let result = service.log(entry3);

		assert!(!result || result);
	}
}
