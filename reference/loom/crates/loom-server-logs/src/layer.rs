// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights
// reserved. SPDX-License-Identifier: Proprietary

//! Tracing layer that captures logs into the buffer.

use std::fmt;

use tracing::field::{Field, Visit};
use tracing::span::Attributes;
use tracing::{Event, Id, Subscriber};
use tracing_subscriber::layer::Context;
use tracing_subscriber::registry::LookupSpan;
use tracing_subscriber::Layer;

use crate::buffer::LogBuffer;
use crate::entry::LogLevel;

/// A tracing Layer that captures log events into a [`LogBuffer`].
///
/// This layer can be composed with other layers (like `fmt::layer()`) to
/// capture logs for the admin UI while still outputting to stdout.
#[derive(Clone)]
pub struct BroadcastLogLayer {
	buffer: LogBuffer,
}

impl BroadcastLogLayer {
	/// Create a new broadcast log layer with the given buffer.
	pub fn new(buffer: LogBuffer) -> Self {
		Self { buffer }
	}

	/// Get a reference to the underlying buffer.
	pub fn buffer(&self) -> &LogBuffer {
		&self.buffer
	}
}

impl<S> Layer<S> for BroadcastLogLayer
where
	S: Subscriber + for<'a> LookupSpan<'a>,
{
	fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
		let metadata = event.metadata();
		let level = LogLevel::from_tracing(metadata.level());
		let target = metadata.target().to_string();

		let mut visitor = FieldVisitor::new();
		event.record(&mut visitor);

		let message = visitor.message.unwrap_or_default();
		let fields = visitor.fields;

		self.buffer.push(level, target, message, fields);
	}

	fn on_new_span(&self, _attrs: &Attributes<'_>, _id: &Id, _ctx: Context<'_, S>) {
		// We don't track spans, only events
	}
}

/// Visitor that extracts fields from a tracing event.
struct FieldVisitor {
	message: Option<String>,
	fields: Vec<(String, String)>,
}

impl FieldVisitor {
	fn new() -> Self {
		Self {
			message: None,
			fields: Vec::new(),
		}
	}
}

impl Visit for FieldVisitor {
	fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
		let name = field.name();
		let value_str = format!("{:?}", value);

		if name == "message" {
			self.message = Some(value_str);
		} else {
			self.fields.push((name.to_string(), value_str));
		}
	}

	fn record_str(&mut self, field: &Field, value: &str) {
		let name = field.name();

		if name == "message" {
			self.message = Some(value.to_string());
		} else {
			self.fields.push((name.to_string(), value.to_string()));
		}
	}

	fn record_i64(&mut self, field: &Field, value: i64) {
		self
			.fields
			.push((field.name().to_string(), value.to_string()));
	}

	fn record_u64(&mut self, field: &Field, value: u64) {
		self
			.fields
			.push((field.name().to_string(), value.to_string()));
	}

	fn record_bool(&mut self, field: &Field, value: bool) {
		self
			.fields
			.push((field.name().to_string(), value.to_string()));
	}

	fn record_f64(&mut self, field: &Field, value: f64) {
		self
			.fields
			.push((field.name().to_string(), value.to_string()));
	}

	fn record_error(&mut self, field: &Field, value: &(dyn std::error::Error + 'static)) {
		self
			.fields
			.push((field.name().to_string(), value.to_string()));
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

	#[test]
	fn test_layer_creation() {
		let buffer = LogBuffer::new(100);
		let layer = BroadcastLogLayer::new(buffer.clone());

		assert!(layer.buffer().is_empty());
		assert_eq!(layer.buffer().capacity(), 100);
	}

	#[tokio::test]
	async fn test_layer_captures_events() {
		let buffer = LogBuffer::new(100);
		let layer = BroadcastLogLayer::new(buffer.clone());

		let subscriber = tracing_subscriber::registry().with(layer);

		tracing::subscriber::with_default(subscriber, || {
			tracing::info!(test_field = "value", "Test log message");
		});

		let entries = buffer.get_entries(10, None, None, None);
		assert_eq!(entries.len(), 1);
		assert_eq!(entries[0].level, LogLevel::Info);
		assert!(entries[0].message.contains("Test log message"));
	}
}
