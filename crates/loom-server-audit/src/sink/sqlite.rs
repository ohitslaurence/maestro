// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use std::sync::Arc;

use async_trait::async_trait;
use sqlx::SqlitePool;

use crate::enrichment::EnrichedAuditEvent;
use crate::error::AuditSinkError;
use crate::filter::AuditFilterConfig;
use crate::sink::AuditSink;

pub struct SqliteAuditSink {
	pool: SqlitePool,
	filter: AuditFilterConfig,
	name: String,
}

impl SqliteAuditSink {
	pub fn new(pool: SqlitePool, filter: AuditFilterConfig) -> Self {
		Self {
			pool,
			filter,
			name: "sqlite".to_string(),
		}
	}
}

#[async_trait]
impl AuditSink for SqliteAuditSink {
	fn name(&self) -> &str {
		&self.name
	}

	fn filter(&self) -> &AuditFilterConfig {
		&self.filter
	}

	async fn publish(&self, event: Arc<EnrichedAuditEvent>) -> Result<(), AuditSinkError> {
		let details_json = serde_json::to_string(&event.base.details)
			.map_err(|e| AuditSinkError::Permanent(format!("failed to serialize details: {e}")))?;

		let session_context_json = event
			.session
			.as_ref()
			.map(serde_json::to_string)
			.transpose()
			.map_err(|e| {
				AuditSinkError::Permanent(format!("failed to serialize session_context: {e}"))
			})?;

		let org_context_json = event
			.org
			.as_ref()
			.map(serde_json::to_string)
			.transpose()
			.map_err(|e| AuditSinkError::Permanent(format!("failed to serialize org_context: {e}")))?;

		let now = chrono::Utc::now();

		sqlx::query(
			r#"
			INSERT INTO audit_logs (
				id, timestamp, event_type, severity, actor_user_id, impersonating_user_id,
				resource_type, resource_id, action, ip_address, user_agent,
				trace_id, span_id, request_id, session_context, org_context,
				details, created_at
			) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
			"#,
		)
		.bind(event.base.id.to_string())
		.bind(event.base.timestamp.to_rfc3339())
		.bind(event.base.event_type.to_string())
		.bind(event.base.severity.to_string())
		.bind(event.base.actor_user_id.as_ref().map(|u| u.to_string()))
		.bind(
			event
				.base
				.impersonating_user_id
				.as_ref()
				.map(|u| u.to_string()),
		)
		.bind(&event.base.resource_type)
		.bind(&event.base.resource_id)
		.bind(&event.base.action)
		.bind(&event.base.ip_address)
		.bind(&event.base.user_agent)
		.bind(&event.base.trace_id)
		.bind(&event.base.span_id)
		.bind(&event.base.request_id)
		.bind(&session_context_json)
		.bind(&org_context_json)
		.bind(&details_json)
		.bind(now.to_rfc3339())
		.execute(&self.pool)
		.await
		.map_err(|e| {
			if is_transient_error(&e) {
				AuditSinkError::Transient(format!("database error: {e}"))
			} else {
				AuditSinkError::Permanent(format!("database error: {e}"))
			}
		})?;

		Ok(())
	}

	async fn health_check(&self) -> Result<(), AuditSinkError> {
		sqlx::query("SELECT 1")
			.execute(&self.pool)
			.await
			.map_err(|e| AuditSinkError::Transient(format!("health check failed: {e}")))?;
		Ok(())
	}
}

fn is_transient_error(e: &sqlx::Error) -> bool {
	match e {
		sqlx::Error::Io(_) => true,
		sqlx::Error::PoolTimedOut => true,
		sqlx::Error::PoolClosed => true,
		sqlx::Error::Database(db_err) => {
			let msg = db_err.message().to_lowercase();
			msg.contains("busy") || msg.contains("locked") || msg.contains("timeout")
		}
		_ => false,
	}
}
