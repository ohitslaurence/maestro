// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights
// reserved. SPDX-License-Identifier: Proprietary

//! Server log buffering and streaming for the Loom admin panel.
//!
//! This crate provides:
//! - [`LogEntry`] - A structured log entry with timestamp, level, target, and message
//! - [`LogBuffer`] - A thread-safe ring buffer that stores recent log entries
//! - [`BroadcastLogLayer`] - A tracing Layer that captures logs and broadcasts to subscribers
//!
//! # Usage
//!
//! ```ignore
//! use loom_server_logs::{LogBuffer, BroadcastLogLayer};
//! use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
//!
//! let log_buffer = LogBuffer::new(10_000);
//! let layer = BroadcastLogLayer::new(log_buffer.clone());
//!
//! tracing_subscriber::registry()
//!     .with(tracing_subscriber::fmt::layer())
//!     .with(layer)
//!     .init();
//! ```

mod buffer;
mod entry;
mod layer;
mod redacting_layer;
mod redacting_writer;

pub use buffer::LogBuffer;
pub use entry::{LogEntry, LogLevel};
pub use layer::BroadcastLogLayer;
pub use redacting_layer::RedactingLayer;
pub use redacting_writer::{RedactingMakeWriter, RedactingWriter};
