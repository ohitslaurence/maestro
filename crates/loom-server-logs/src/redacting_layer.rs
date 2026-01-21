// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights
// reserved. SPDX-License-Identifier: Proprietary

//! Tracing layer that redacts secrets from log messages and fields.

use std::fmt;

use tracing::field::{Field, Visit};
use tracing::{Event, Subscriber};
use tracing_subscriber::layer::Context;
use tracing_subscriber::registry::LookupSpan;
use tracing_subscriber::Layer;

use crate::buffer::LogBuffer;
use crate::entry::LogLevel;

/// A tracing Layer that redacts secrets from log events before storing them.
///
/// This layer intercepts all log events, applies secret redaction using
/// `loom_redact::redact()` to both the message and all field values, then
/// stores the redacted content in the buffer.
#[derive(Clone)]
pub struct RedactingLayer {
	buffer: LogBuffer,
}

impl RedactingLayer {
	/// Create a new redacting layer with the given buffer.
	pub fn new(buffer: LogBuffer) -> Self {
		Self { buffer }
	}

	/// Get a reference to the underlying buffer.
	pub fn buffer(&self) -> &LogBuffer {
		&self.buffer
	}
}

impl<S> Layer<S> for RedactingLayer
where
	S: Subscriber + for<'a> LookupSpan<'a>,
{
	fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
		let metadata = event.metadata();
		let level = LogLevel::from_tracing(metadata.level());
		let target = metadata.target().to_string();

		let mut visitor = RedactingVisitor::new();
		event.record(&mut visitor);

		let message = visitor.message.unwrap_or_default();
		let fields = visitor.fields;

		self.buffer.push(level, target, message, fields);
	}
}

struct RedactingVisitor {
	message: Option<String>,
	fields: Vec<(String, String)>,
}

impl RedactingVisitor {
	fn new() -> Self {
		Self {
			message: None,
			fields: Vec::new(),
		}
	}

	fn redact_value(&self, value: &str) -> String {
		loom_redact::redact(value).into_owned()
	}
}

impl Visit for RedactingVisitor {
	fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
		let name = field.name();
		let value_str = format!("{:?}", value);
		let redacted = self.redact_value(&value_str);

		if name == "message" {
			self.message = Some(redacted);
		} else {
			self.fields.push((name.to_string(), redacted));
		}
	}

	fn record_str(&mut self, field: &Field, value: &str) {
		let name = field.name();
		let redacted = self.redact_value(value);

		if name == "message" {
			self.message = Some(redacted);
		} else {
			self.fields.push((name.to_string(), redacted));
		}
	}

	fn record_i64(&mut self, field: &Field, value: i64) {
		let s = value.to_string();
		let redacted = self.redact_value(&s);
		self.fields.push((field.name().to_string(), redacted));
	}

	fn record_u64(&mut self, field: &Field, value: u64) {
		let s = value.to_string();
		let redacted = self.redact_value(&s);
		self.fields.push((field.name().to_string(), redacted));
	}

	fn record_bool(&mut self, field: &Field, value: bool) {
		let s = value.to_string();
		let redacted = self.redact_value(&s);
		self.fields.push((field.name().to_string(), redacted));
	}

	fn record_f64(&mut self, field: &Field, value: f64) {
		let s = value.to_string();
		let redacted = self.redact_value(&s);
		self.fields.push((field.name().to_string(), redacted));
	}

	fn record_error(&mut self, field: &Field, value: &(dyn std::error::Error + 'static)) {
		let redacted = self.redact_value(&value.to_string());
		self.fields.push((field.name().to_string(), redacted));
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use tracing_subscriber::layer::SubscriberExt;

	fn github_pat() -> String {
		format!("ghp_{}", "A1b2C3d4E5f6G7h8I9j0K1l2M3n4O5p6Q7r8")
	}

	fn aws_key() -> String {
		format!("AKIA{}", "Z7VRSQ5TJN2XMPLQ")
	}

	#[test]
	fn test_redacting_layer_creation() {
		let buffer = LogBuffer::new(100);
		let layer = RedactingLayer::new(buffer.clone());

		assert!(layer.buffer().is_empty());
		assert_eq!(layer.buffer().capacity(), 100);
	}

	#[tokio::test]
	async fn test_secrets_in_messages_are_redacted() {
		let buffer = LogBuffer::new(100);
		let layer = RedactingLayer::new(buffer.clone());

		let subscriber = tracing_subscriber::registry().with(layer);

		let secret = format!("GITHUB_TOKEN={}", github_pat());
		tracing::subscriber::with_default(subscriber, || {
			tracing::info!("{}", secret);
		});

		let entries = buffer.get_entries(10, None, None, None);
		assert_eq!(entries.len(), 1);

		let message = &entries[0].message;
		assert!(
			!message.contains("ghp_"),
			"Secret should be redacted: {}",
			message
		);
		assert!(
			message.contains("[REDACTED:"),
			"Should contain redaction marker: {}",
			message
		);
	}

	#[tokio::test]
	async fn test_secrets_in_fields_are_redacted() {
		let buffer = LogBuffer::new(100);
		let layer = RedactingLayer::new(buffer.clone());

		let subscriber = tracing_subscriber::registry().with(layer);

		let secret = aws_key();
		tracing::subscriber::with_default(subscriber, || {
			tracing::info!(aws_key = %secret, "Processing request");
		});

		let entries = buffer.get_entries(10, None, None, None);
		assert_eq!(entries.len(), 1);

		let has_redacted = entries[0]
			.fields
			.iter()
			.any(|(_, v)| v.contains("[REDACTED:"));
		assert!(
			has_redacted,
			"Field value should be redacted: {:?}",
			entries[0].fields
		);
	}

	#[tokio::test]
	async fn test_normal_text_passes_through() {
		let buffer = LogBuffer::new(100);
		let layer = RedactingLayer::new(buffer.clone());

		let subscriber = tracing_subscriber::registry().with(layer);

		tracing::subscriber::with_default(subscriber, || {
			tracing::info!(user = "alice", "Hello, world!");
		});

		let entries = buffer.get_entries(10, None, None, None);
		assert_eq!(entries.len(), 1);
		assert!(entries[0].message.contains("Hello, world!"));
		assert!(entries[0]
			.fields
			.iter()
			.any(|(k, v)| k == "user" && v == "alice"));
	}

	#[test]
	fn test_redacting_visitor_redacts_secrets() {
		let visitor = RedactingVisitor::new();
		let input = format!("token: {}", github_pat());
		let redacted = visitor.redact_value(&input);

		assert!(
			!redacted.contains("ghp_"),
			"Should not contain ghp_: {}",
			redacted
		);
		assert!(
			redacted.contains("[REDACTED:"),
			"Should contain redaction marker: {}",
			redacted
		);
	}

	#[test]
	fn test_redacting_visitor_preserves_normal_text() {
		let visitor = RedactingVisitor::new();
		let input = "Hello, world!";
		let redacted = visitor.redact_value(input);

		assert_eq!(redacted, input);
	}
}
