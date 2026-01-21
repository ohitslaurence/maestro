// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

pub mod enrichment;
pub mod error;
pub mod event;
pub mod filter;
pub mod pipeline;
pub mod redaction;
pub mod sink;

pub use enrichment::{
	AuditEnricher, EnrichedAuditEvent, GeoIpInfo, NoopEnricher, OrgContext, SessionContext,
};
pub use error::{AuditError, AuditResult, AuditSinkError};
pub use event::{
	AuditEventType, AuditLogBuilder, AuditLogEntry, AuditSeverity, UserId,
	DEFAULT_AUDIT_RETENTION_DAYS,
};
pub use filter::AuditFilterConfig;
pub use pipeline::AuditService;
pub use sink::AuditSink;

pub use loom_server_config::{
	AuditConfig, FileFormat, FileSinkConfig, HttpSinkConfig, JsonStreamConfig, QueueOverflowPolicy,
	StreamProtocol, SyslogConfig, SyslogProtocol,
};

#[cfg(feature = "sink-sqlite")]
pub use sink::sqlite::SqliteAuditSink;

#[cfg(feature = "sink-tracing")]
pub use sink::tracing::TracingAuditSink;

#[cfg(feature = "sink-http")]
pub use sink::http::HttpAuditSink;

#[cfg(feature = "sink-json-stream")]
pub use sink::json_stream::JsonStreamSink;
