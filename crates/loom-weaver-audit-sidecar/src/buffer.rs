// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, BufWriter, Write};
#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;
use std::path::PathBuf;

use thiserror::Error;

use crate::events::WeaverAuditEvent;

#[derive(Error, Debug)]
pub enum BufferError {
	#[error("IO error: {0}")]
	Io(#[from] std::io::Error),

	#[error("serialization error: {0}")]
	Serialization(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, BufferError>;

pub struct EventBuffer {
	path: PathBuf,
	max_bytes: u64,
	current_bytes: u64,
}

impl EventBuffer {
	pub fn new(path: PathBuf, max_bytes: u64) -> Result<Self> {
		let current_bytes = if path.exists() {
			std::fs::metadata(&path)?.len()
		} else {
			0
		};

		Ok(EventBuffer {
			path,
			max_bytes,
			current_bytes,
		})
	}

	pub fn append(&mut self, event: &WeaverAuditEvent) -> Result<()> {
		let line = serde_json::to_string(event)?;
		let line_bytes = line.len() as u64 + 1;

		if self.current_bytes + line_bytes > self.max_bytes {
			self.truncate_oldest(line_bytes)?;
		}

		let mut opts = OpenOptions::new();
		opts.create(true).append(true);
		#[cfg(unix)]
		opts.mode(0o600); // Only owner can read/write
		let mut file = opts.open(&self.path)?;

		writeln!(file, "{}", line)?;
		self.current_bytes += line_bytes;

		Ok(())
	}

	pub fn read_all(&self) -> Result<Vec<WeaverAuditEvent>> {
		if !self.path.exists() {
			return Ok(Vec::new());
		}

		let file = File::open(&self.path)?;
		let reader = BufReader::new(file);
		let mut events = Vec::new();

		for line in reader.lines() {
			let line = line?;
			if line.trim().is_empty() {
				continue;
			}
			match serde_json::from_str(&line) {
				Ok(event) => events.push(event),
				Err(e) => {
					tracing::warn!("Failed to parse buffered event: {}", e);
				}
			}
		}

		Ok(events)
	}

	pub fn clear(&mut self) -> Result<()> {
		if self.path.exists() {
			std::fs::remove_file(&self.path)?;
		}
		self.current_bytes = 0;
		Ok(())
	}

	pub fn len(&self) -> u64 {
		self.current_bytes
	}

	pub fn is_empty(&self) -> bool {
		self.current_bytes == 0
	}

	fn truncate_oldest(&mut self, needed_bytes: u64) -> Result<()> {
		if !self.path.exists() {
			return Ok(());
		}

		let file = File::open(&self.path)?;
		let reader = BufReader::new(file);
		let mut lines: Vec<String> = reader.lines().collect::<std::io::Result<_>>()?;

		let mut freed_bytes = 0u64;
		while freed_bytes < needed_bytes && !lines.is_empty() {
			let removed = lines.remove(0);
			freed_bytes += removed.len() as u64 + 1;
		}

		let temp_path = self.path.with_extension("tmp");
		{
			let file = File::create(&temp_path)?;
			let mut writer = BufWriter::new(file);
			for line in &lines {
				writeln!(writer, "{}", line)?;
			}
		}

		std::fs::rename(&temp_path, &self.path)?;
		self.current_bytes = self.current_bytes.saturating_sub(freed_bytes);

		Ok(())
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use tempfile::TempDir;

	fn create_test_event(id: u32) -> WeaverAuditEvent {
		WeaverAuditEvent {
			weaver_id: "test-weaver".to_string(),
			org_id: "test-org".to_string(),
			owner_user_id: "test-user".to_string(),
			timestamp_ns: id as u64 * 1000000,
			pid: id,
			tid: id,
			comm: "test".to_string(),
			event_type: crate::events::WeaverAuditEventType::ProcessExec,
			details: serde_json::json!({"test": id}),
		}
	}

	#[test]
	fn test_append_and_read() {
		let tmp = TempDir::new().unwrap();
		let path = tmp.path().join("buffer.jsonl");
		let mut buffer = EventBuffer::new(path, 1024 * 1024).unwrap();

		buffer.append(&create_test_event(1)).unwrap();
		buffer.append(&create_test_event(2)).unwrap();

		let events = buffer.read_all().unwrap();
		assert_eq!(events.len(), 2);
		assert_eq!(events[0].pid, 1);
		assert_eq!(events[1].pid, 2);
	}

	#[test]
	fn test_clear() {
		let tmp = TempDir::new().unwrap();
		let path = tmp.path().join("buffer.jsonl");
		let mut buffer = EventBuffer::new(path, 1024 * 1024).unwrap();

		buffer.append(&create_test_event(1)).unwrap();
		buffer.clear().unwrap();

		let events = buffer.read_all().unwrap();
		assert!(events.is_empty());
		assert!(buffer.is_empty());
	}

	#[test]
	fn test_size_limit_truncates_oldest() {
		let tmp = TempDir::new().unwrap();
		let path = tmp.path().join("buffer.jsonl");
		let mut buffer = EventBuffer::new(path, 500).unwrap();

		for i in 0..10 {
			buffer.append(&create_test_event(i)).unwrap();
		}

		assert!(buffer.len() <= 500);

		let events = buffer.read_all().unwrap();
		assert!(!events.is_empty());
		assert!(events[0].pid > 0);
	}
}
