// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights
// reserved. SPDX-License-Identifier: Proprietary

//! Thread-safe ring buffer for log entries.

use std::collections::VecDeque;
use std::sync::Arc;

use parking_lot::RwLock;
use tokio::sync::broadcast;

use crate::entry::{LogEntry, LogLevel};

/// Default buffer capacity.
pub const DEFAULT_CAPACITY: usize = 10_000;

/// A thread-safe ring buffer that stores recent log entries.
///
/// When the buffer is full, oldest entries are evicted to make room for new ones.
/// Supports both snapshot queries and real-time streaming via broadcast channel.
#[derive(Clone)]
pub struct LogBuffer {
	inner: Arc<LogBufferInner>,
}

struct LogBufferInner {
	/// Ring buffer of log entries.
	entries: RwLock<VecDeque<LogEntry>>,
	/// Maximum capacity.
	capacity: usize,
	/// Next entry ID (monotonically increasing).
	next_id: RwLock<u64>,
	/// Broadcast channel for real-time streaming.
	sender: broadcast::Sender<LogEntry>,
}

impl LogBuffer {
	/// Create a new log buffer with the specified capacity.
	pub fn new(capacity: usize) -> Self {
		let (sender, _) = broadcast::channel(1024);
		Self {
			inner: Arc::new(LogBufferInner {
				entries: RwLock::new(VecDeque::with_capacity(capacity)),
				capacity,
				next_id: RwLock::new(1),
				sender,
			}),
		}
	}

	/// Create a new log buffer with default capacity.
	pub fn with_default_capacity() -> Self {
		Self::new(DEFAULT_CAPACITY)
	}

	/// Push a new log entry into the buffer.
	///
	/// If the buffer is full, the oldest entry is evicted.
	/// The entry is also broadcast to all subscribers.
	pub fn push(
		&self,
		level: LogLevel,
		target: String,
		message: String,
		fields: Vec<(String, String)>,
	) {
		let id = {
			let mut next_id = self.inner.next_id.write();
			let id = *next_id;
			*next_id += 1;
			id
		};

		let entry = LogEntry::new(id, level, target, message, fields);

		{
			let mut entries = self.inner.entries.write();
			if entries.len() >= self.inner.capacity {
				entries.pop_front();
			}
			entries.push_back(entry.clone());
		}

		// Broadcast to subscribers (ignore errors if no subscribers)
		let _ = self.inner.sender.send(entry);
	}

	/// Get recent log entries.
	///
	/// Returns up to `limit` entries, optionally filtered by minimum level and target prefix.
	/// If `after_id` is provided, only returns entries with ID greater than that value.
	pub fn get_entries(
		&self,
		limit: usize,
		min_level: Option<LogLevel>,
		target_prefix: Option<&str>,
		after_id: Option<u64>,
	) -> Vec<LogEntry> {
		let entries = self.inner.entries.read();

		entries
			.iter()
			.rev()
			.filter(|e| {
				if let Some(min) = min_level {
					if e.level < min {
						return false;
					}
				}
				if let Some(prefix) = target_prefix {
					if !e.target.starts_with(prefix) {
						return false;
					}
				}
				if let Some(after) = after_id {
					if e.id <= after {
						return false;
					}
				}
				true
			})
			.take(limit)
			.cloned()
			.collect::<Vec<_>>()
			.into_iter()
			.rev()
			.collect()
	}

	/// Get the total number of entries currently in the buffer.
	pub fn len(&self) -> usize {
		self.inner.entries.read().len()
	}

	/// Check if the buffer is empty.
	pub fn is_empty(&self) -> bool {
		self.inner.entries.read().is_empty()
	}

	/// Get the buffer capacity.
	pub fn capacity(&self) -> usize {
		self.inner.capacity
	}

	/// Get the current entry ID counter (next ID to be assigned).
	pub fn current_id(&self) -> u64 {
		*self.inner.next_id.read()
	}

	/// Subscribe to real-time log entries.
	///
	/// Returns a broadcast receiver that will receive new log entries as they are pushed.
	pub fn subscribe(&self) -> broadcast::Receiver<LogEntry> {
		self.inner.sender.subscribe()
	}

	/// Clear all entries from the buffer.
	pub fn clear(&self) {
		self.inner.entries.write().clear();
	}
}

impl Default for LogBuffer {
	fn default() -> Self {
		Self::with_default_capacity()
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_push_and_get() {
		let buffer = LogBuffer::new(100);

		buffer.push(LogLevel::Info, "test".into(), "message 1".into(), vec![]);
		buffer.push(LogLevel::Warn, "test".into(), "message 2".into(), vec![]);

		let entries = buffer.get_entries(10, None, None, None);
		assert_eq!(entries.len(), 2);
		assert_eq!(entries[0].message, "message 1");
		assert_eq!(entries[1].message, "message 2");
	}

	#[test]
	fn test_capacity_eviction() {
		let buffer = LogBuffer::new(3);

		buffer.push(LogLevel::Info, "test".into(), "msg 1".into(), vec![]);
		buffer.push(LogLevel::Info, "test".into(), "msg 2".into(), vec![]);
		buffer.push(LogLevel::Info, "test".into(), "msg 3".into(), vec![]);
		buffer.push(LogLevel::Info, "test".into(), "msg 4".into(), vec![]);

		assert_eq!(buffer.len(), 3);
		let entries = buffer.get_entries(10, None, None, None);
		assert_eq!(entries[0].message, "msg 2");
		assert_eq!(entries[2].message, "msg 4");
	}

	#[test]
	fn test_level_filter() {
		let buffer = LogBuffer::new(100);

		buffer.push(LogLevel::Debug, "test".into(), "debug".into(), vec![]);
		buffer.push(LogLevel::Info, "test".into(), "info".into(), vec![]);
		buffer.push(LogLevel::Warn, "test".into(), "warn".into(), vec![]);
		buffer.push(LogLevel::Error, "test".into(), "error".into(), vec![]);

		let entries = buffer.get_entries(10, Some(LogLevel::Warn), None, None);
		assert_eq!(entries.len(), 2);
		assert_eq!(entries[0].message, "warn");
		assert_eq!(entries[1].message, "error");
	}

	#[test]
	fn test_target_filter() {
		let buffer = LogBuffer::new(100);

		buffer.push(
			LogLevel::Info,
			"loom_server::api".into(),
			"api log".into(),
			vec![],
		);
		buffer.push(
			LogLevel::Info,
			"loom_server::db".into(),
			"db log".into(),
			vec![],
		);
		buffer.push(
			LogLevel::Info,
			"hyper::client".into(),
			"hyper log".into(),
			vec![],
		);

		let entries = buffer.get_entries(10, None, Some("loom_server"), None);
		assert_eq!(entries.len(), 2);
	}

	#[test]
	fn test_after_id_filter() {
		let buffer = LogBuffer::new(100);

		buffer.push(LogLevel::Info, "test".into(), "msg 1".into(), vec![]);
		buffer.push(LogLevel::Info, "test".into(), "msg 2".into(), vec![]);
		buffer.push(LogLevel::Info, "test".into(), "msg 3".into(), vec![]);

		let entries = buffer.get_entries(10, None, None, Some(1));
		assert_eq!(entries.len(), 2);
		assert_eq!(entries[0].id, 2);
		assert_eq!(entries[1].id, 3);
	}

	#[tokio::test]
	async fn test_broadcast_subscription() {
		let buffer = LogBuffer::new(100);
		let mut rx = buffer.subscribe();

		buffer.push(
			LogLevel::Info,
			"test".into(),
			"broadcast test".into(),
			vec![],
		);

		let entry = rx.recv().await.unwrap();
		assert_eq!(entry.message, "broadcast test");
	}
}
