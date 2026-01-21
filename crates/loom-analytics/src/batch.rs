// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! Event batching and background flush for the analytics SDK.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::{mpsc, Mutex, Notify};
use tracing::{debug, error, info, warn};

use crate::error::{AnalyticsError, Result};

/// Configuration for the event batch queue.
#[derive(Debug, Clone)]
pub struct BatchConfig {
	/// Maximum number of events to batch before flushing.
	pub max_batch_size: usize,
	/// Interval between automatic flushes.
	pub flush_interval: Duration,
	/// Maximum number of events to queue before dropping oldest.
	pub max_queue_size: usize,
}

impl Default for BatchConfig {
	fn default() -> Self {
		Self {
			max_batch_size: 10,
			flush_interval: Duration::from_secs(10),
			max_queue_size: 1000,
		}
	}
}

/// A queued event waiting to be sent.
#[derive(Debug, Clone)]
pub struct QueuedEvent {
	pub distinct_id: String,
	pub event_name: String,
	pub properties: serde_json::Value,
	pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// Command sent to the background flush task.
#[derive(Debug)]
pub enum BatchCommand {
	/// Queue a new event.
	Enqueue(QueuedEvent),
	/// Force an immediate flush.
	Flush,
	/// Shutdown the batch processor.
	Shutdown,
}

/// Handler for sending batched events to the server.
#[async_trait::async_trait]
pub trait BatchSender: Send + Sync {
	/// Send a batch of events to the server.
	async fn send_batch(&self, events: Vec<QueuedEvent>) -> Result<()>;
}

/// The batch processor that runs in the background.
pub struct BatchProcessor {
	config: BatchConfig,
	sender: Arc<dyn BatchSender>,
	queue: Mutex<Vec<QueuedEvent>>,
	shutdown: AtomicBool,
	flush_notify: Notify,
}

impl BatchProcessor {
	/// Creates a new batch processor.
	pub fn new(config: BatchConfig, sender: Arc<dyn BatchSender>) -> Self {
		Self {
			config,
			sender,
			queue: Mutex::new(Vec::new()),
			shutdown: AtomicBool::new(false),
			flush_notify: Notify::new(),
		}
	}

	/// Enqueues an event for batched sending.
	pub async fn enqueue(&self, event: QueuedEvent) -> Result<()> {
		if self.shutdown.load(Ordering::SeqCst) {
			return Err(AnalyticsError::ClientShutdown);
		}

		let mut queue = self.queue.lock().await;

		// If queue is at max, drop oldest events
		while queue.len() >= self.config.max_queue_size {
			let dropped = queue.remove(0);
			warn!(
				event_name = %dropped.event_name,
				distinct_id = %dropped.distinct_id,
				"Dropped event due to queue overflow"
			);
		}

		queue.push(event);

		// Check if we should flush based on batch size
		if queue.len() >= self.config.max_batch_size {
			drop(queue);
			self.flush_notify.notify_one();
		}

		Ok(())
	}

	/// Forces an immediate flush of queued events.
	pub async fn flush(&self) -> Result<()> {
		let events = {
			let mut queue = self.queue.lock().await;
			std::mem::take(&mut *queue)
		};

		if events.is_empty() {
			return Ok(());
		}

		debug!(count = events.len(), "Flushing event batch");
		self.sender.send_batch(events).await
	}

	/// Returns the number of events currently queued.
	pub async fn queue_len(&self) -> usize {
		self.queue.lock().await.len()
	}

	/// Signals the processor to shut down.
	pub fn shutdown(&self) {
		self.shutdown.store(true, Ordering::SeqCst);
		self.flush_notify.notify_one();
	}

	/// Returns true if shutdown has been requested.
	pub fn is_shutdown(&self) -> bool {
		self.shutdown.load(Ordering::SeqCst)
	}

	/// Runs the background flush loop.
	pub async fn run(&self) {
		info!(
			flush_interval_secs = self.config.flush_interval.as_secs(),
			max_batch_size = self.config.max_batch_size,
			"Starting analytics batch processor"
		);

		loop {
			tokio::select! {
				_ = tokio::time::sleep(self.config.flush_interval) => {
					if self.shutdown.load(Ordering::SeqCst) {
						break;
					}

					if let Err(e) = self.flush().await {
						error!(error = %e, "Failed to flush analytics batch");
					}
				}
				_ = self.flush_notify.notified() => {
					if self.shutdown.load(Ordering::SeqCst) {
						// Final flush before shutdown
						if let Err(e) = self.flush().await {
							error!(error = %e, "Failed to flush analytics batch on shutdown");
						}
						break;
					}

					if let Err(e) = self.flush().await {
						error!(error = %e, "Failed to flush analytics batch");
					}
				}
			}
		}

		info!("Analytics batch processor stopped");
	}
}

/// A channel-based batch queue for sending commands to the processor.
pub struct BatchQueue {
	tx: mpsc::Sender<BatchCommand>,
}

impl BatchQueue {
	/// Creates a new batch queue with the given channel sender.
	pub fn new(tx: mpsc::Sender<BatchCommand>) -> Self {
		Self { tx }
	}

	/// Enqueues an event.
	pub async fn enqueue(&self, event: QueuedEvent) -> Result<()> {
		self
			.tx
			.send(BatchCommand::Enqueue(event))
			.await
			.map_err(|_| AnalyticsError::ClientShutdown)
	}

	/// Forces a flush.
	pub async fn flush(&self) -> Result<()> {
		self
			.tx
			.send(BatchCommand::Flush)
			.await
			.map_err(|_| AnalyticsError::ClientShutdown)
	}

	/// Signals shutdown.
	pub async fn shutdown(&self) -> Result<()> {
		self
			.tx
			.send(BatchCommand::Shutdown)
			.await
			.map_err(|_| AnalyticsError::ClientShutdown)
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use proptest::prelude::*;
	use std::sync::atomic::AtomicUsize;

	struct MockSender {
		sent_count: AtomicUsize,
		sent_events: Mutex<Vec<Vec<QueuedEvent>>>,
		should_fail: AtomicBool,
	}

	impl MockSender {
		fn new() -> Self {
			Self {
				sent_count: AtomicUsize::new(0),
				sent_events: Mutex::new(Vec::new()),
				should_fail: AtomicBool::new(false),
			}
		}

		async fn get_sent_batches(&self) -> Vec<Vec<QueuedEvent>> {
			self.sent_events.lock().await.clone()
		}

		fn set_should_fail(&self, fail: bool) {
			self.should_fail.store(fail, Ordering::SeqCst);
		}
	}

	#[async_trait::async_trait]
	impl BatchSender for MockSender {
		async fn send_batch(&self, events: Vec<QueuedEvent>) -> Result<()> {
			if self.should_fail.load(Ordering::SeqCst) {
				return Err(AnalyticsError::ServerError {
					status: 500,
					message: "mock failure".to_string(),
				});
			}
			self.sent_count.fetch_add(events.len(), Ordering::SeqCst);
			self.sent_events.lock().await.push(events);
			Ok(())
		}
	}

	fn create_test_event(name: &str) -> QueuedEvent {
		QueuedEvent {
			distinct_id: "test_user".to_string(),
			event_name: name.to_string(),
			properties: serde_json::json!({}),
			timestamp: chrono::Utc::now(),
		}
	}

	#[tokio::test]
	async fn test_enqueue_single_event() {
		let sender = Arc::new(MockSender::new());
		let config = BatchConfig {
			max_batch_size: 10,
			flush_interval: Duration::from_secs(60),
			max_queue_size: 100,
		};
		let processor = BatchProcessor::new(config, sender.clone());

		processor.enqueue(create_test_event("test")).await.unwrap();

		assert_eq!(processor.queue_len().await, 1);
	}

	#[tokio::test]
	async fn test_flush_sends_events() {
		let sender = Arc::new(MockSender::new());
		let config = BatchConfig::default();
		let processor = BatchProcessor::new(config, sender.clone());

		processor
			.enqueue(create_test_event("event1"))
			.await
			.unwrap();
		processor
			.enqueue(create_test_event("event2"))
			.await
			.unwrap();

		processor.flush().await.unwrap();

		let batches = sender.get_sent_batches().await;
		assert_eq!(batches.len(), 1);
		assert_eq!(batches[0].len(), 2);
		assert_eq!(processor.queue_len().await, 0);
	}

	#[tokio::test]
	async fn test_flush_empty_queue_succeeds() {
		let sender = Arc::new(MockSender::new());
		let config = BatchConfig::default();
		let processor = BatchProcessor::new(config, sender.clone());

		processor.flush().await.unwrap();

		let batches = sender.get_sent_batches().await;
		assert!(batches.is_empty());
	}

	#[tokio::test]
	async fn test_queue_overflow_drops_oldest() {
		let sender = Arc::new(MockSender::new());
		let config = BatchConfig {
			max_batch_size: 100,
			flush_interval: Duration::from_secs(60),
			max_queue_size: 3,
		};
		let processor = BatchProcessor::new(config, sender.clone());

		for i in 0..5 {
			processor
				.enqueue(create_test_event(&format!("event{i}")))
				.await
				.unwrap();
		}

		assert_eq!(processor.queue_len().await, 3);

		processor.flush().await.unwrap();
		let batches = sender.get_sent_batches().await;
		assert_eq!(batches[0].len(), 3);
		assert_eq!(batches[0][0].event_name, "event2");
		assert_eq!(batches[0][1].event_name, "event3");
		assert_eq!(batches[0][2].event_name, "event4");
	}

	#[tokio::test]
	async fn test_shutdown_prevents_enqueue() {
		let sender = Arc::new(MockSender::new());
		let config = BatchConfig::default();
		let processor = BatchProcessor::new(config, sender.clone());

		processor.shutdown();

		let result = processor.enqueue(create_test_event("test")).await;
		assert!(matches!(result, Err(AnalyticsError::ClientShutdown)));
	}

	#[tokio::test]
	async fn test_flush_failure_returns_error() {
		let sender = Arc::new(MockSender::new());
		sender.set_should_fail(true);
		let config = BatchConfig::default();
		let processor = BatchProcessor::new(config, sender.clone());

		processor.enqueue(create_test_event("test")).await.unwrap();

		let result = processor.flush().await;
		assert!(matches!(result, Err(AnalyticsError::ServerError { .. })));
	}

	proptest! {
		#[test]
		fn test_batch_config_defaults_are_reasonable(
			max_batch in 1..100usize,
			max_queue in 100..10000usize,
		) {
			let config = BatchConfig {
				max_batch_size: max_batch,
				flush_interval: Duration::from_secs(10),
				max_queue_size: max_queue,
			};

			prop_assert!(config.max_batch_size > 0);
			prop_assert!(config.max_queue_size >= config.max_batch_size);
		}
	}
}
