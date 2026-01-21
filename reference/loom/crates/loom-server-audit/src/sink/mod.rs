// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use std::sync::Arc;

use async_trait::async_trait;

use crate::enrichment::EnrichedAuditEvent;
pub use crate::error::AuditSinkError;
use crate::filter::AuditFilterConfig;

#[async_trait]
pub trait AuditSink: Send + Sync {
	/// Unique name for this sink (used in logs/metrics).
	fn name(&self) -> &str;

	/// Per-sink filter configuration.
	fn filter(&self) -> &AuditFilterConfig;

	/// Publish an event to the sink.
	async fn publish(&self, event: Arc<EnrichedAuditEvent>) -> Result<(), AuditSinkError>;

	/// Health check (optional, default: Ok).
	async fn health_check(&self) -> Result<(), AuditSinkError> {
		Ok(())
	}
}

#[cfg(feature = "sink-sqlite")]
pub mod sqlite;

#[cfg(feature = "sink-tracing")]
pub mod tracing;

#[cfg(feature = "sink-syslog")]
pub mod syslog;

#[cfg(feature = "sink-file")]
pub mod file;

#[cfg(feature = "sink-http")]
pub mod http;

#[cfg(feature = "sink-json-stream")]
pub mod json_stream;
