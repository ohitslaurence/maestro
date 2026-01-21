// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use std::sync::Arc;

use async_trait::async_trait;
use tracing::Level;

use super::{AuditSink, AuditSinkError};
use crate::enrichment::EnrichedAuditEvent;
use crate::event::AuditSeverity;
use crate::filter::AuditFilterConfig;

pub struct TracingAuditSink {
	filter: AuditFilterConfig,
}

impl TracingAuditSink {
	pub fn new(filter: AuditFilterConfig) -> Self {
		Self { filter }
	}
}

pub fn severity_to_level(severity: AuditSeverity) -> Level {
	match severity {
		AuditSeverity::Debug => Level::DEBUG,
		AuditSeverity::Info | AuditSeverity::Notice => Level::INFO,
		AuditSeverity::Warning => Level::WARN,
		AuditSeverity::Error | AuditSeverity::Critical => Level::ERROR,
	}
}

#[async_trait]
impl AuditSink for TracingAuditSink {
	fn name(&self) -> &str {
		"tracing"
	}

	fn filter(&self) -> &AuditFilterConfig {
		&self.filter
	}

	async fn publish(&self, event: Arc<EnrichedAuditEvent>) -> Result<(), AuditSinkError> {
		let base = &event.base;
		let level = severity_to_level(base.severity);

		let event_type = base.event_type.to_string();
		let severity = base.severity.to_string();
		let id = base.id.to_string();
		let timestamp = base.timestamp.to_rfc3339();
		let action = &base.action;

		let actor_user_id = base.actor_user_id.map(|u| u.to_string());
		let impersonating_user_id = base.impersonating_user_id.map(|u| u.to_string());
		let resource_type = base.resource_type.as_deref();
		let resource_id = base.resource_id.as_deref();
		let ip_address = base.ip_address.as_deref();
		let user_agent = base.user_agent.as_deref();
		let trace_id = base.trace_id.as_deref();
		let span_id = base.span_id.as_deref();
		let request_id = base.request_id.as_deref();

		let session_id = event.session.as_ref().and_then(|s| s.session_id.as_deref());
		let session_type = event
			.session
			.as_ref()
			.and_then(|s| s.session_type.as_deref());
		let device_label = event
			.session
			.as_ref()
			.and_then(|s| s.device_label.as_deref());
		let geo_city = event
			.session
			.as_ref()
			.and_then(|s| s.geo.as_ref())
			.and_then(|g| g.city.as_deref());
		let geo_country = event
			.session
			.as_ref()
			.and_then(|s| s.geo.as_ref())
			.and_then(|g| g.country.as_deref());

		let org_id = event.org.as_ref().and_then(|o| o.org_id.as_deref());
		let org_slug = event.org.as_ref().and_then(|o| o.org_slug.as_deref());
		let org_role = event.org.as_ref().and_then(|o| o.org_role.as_deref());

		let details = if base.details.is_null() {
			None
		} else {
			Some(base.details.to_string())
		};

		match level {
			Level::DEBUG => {
				tracing::debug!(
					target: "loom_audit",
					event_type,
					severity,
					id,
					timestamp,
					action,
					actor_user_id,
					impersonating_user_id,
					resource_type,
					resource_id,
					ip_address,
					user_agent,
					trace_id,
					span_id,
					request_id,
					session_id,
					session_type,
					device_label,
					geo_city,
					geo_country,
					org_id,
					org_slug,
					org_role,
					details,
					"audit event"
				);
			}
			Level::INFO => {
				tracing::info!(
					target: "loom_audit",
					event_type,
					severity,
					id,
					timestamp,
					action,
					actor_user_id,
					impersonating_user_id,
					resource_type,
					resource_id,
					ip_address,
					user_agent,
					trace_id,
					span_id,
					request_id,
					session_id,
					session_type,
					device_label,
					geo_city,
					geo_country,
					org_id,
					org_slug,
					org_role,
					details,
					"audit event"
				);
			}
			Level::WARN => {
				tracing::warn!(
					target: "loom_audit",
					event_type,
					severity,
					id,
					timestamp,
					action,
					actor_user_id,
					impersonating_user_id,
					resource_type,
					resource_id,
					ip_address,
					user_agent,
					trace_id,
					span_id,
					request_id,
					session_id,
					session_type,
					device_label,
					geo_city,
					geo_country,
					org_id,
					org_slug,
					org_role,
					details,
					"audit event"
				);
			}
			Level::ERROR => {
				tracing::error!(
					target: "loom_audit",
					event_type,
					severity,
					id,
					timestamp,
					action,
					actor_user_id,
					impersonating_user_id,
					resource_type,
					resource_id,
					ip_address,
					user_agent,
					trace_id,
					span_id,
					request_id,
					session_id,
					session_type,
					device_label,
					geo_city,
					geo_country,
					org_id,
					org_slug,
					org_role,
					details,
					"audit event"
				);
			}
			Level::TRACE => {
				tracing::trace!(
					target: "loom_audit",
					event_type,
					severity,
					id,
					timestamp,
					action,
					actor_user_id,
					impersonating_user_id,
					resource_type,
					resource_id,
					ip_address,
					user_agent,
					trace_id,
					span_id,
					request_id,
					session_id,
					session_type,
					device_label,
					geo_city,
					geo_country,
					org_id,
					org_slug,
					org_role,
					details,
					"audit event"
				);
			}
		}

		Ok(())
	}

	async fn health_check(&self) -> Result<(), AuditSinkError> {
		Ok(())
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn severity_to_level_mappings() {
		assert_eq!(severity_to_level(AuditSeverity::Debug), Level::DEBUG);
		assert_eq!(severity_to_level(AuditSeverity::Info), Level::INFO);
		assert_eq!(severity_to_level(AuditSeverity::Notice), Level::INFO);
		assert_eq!(severity_to_level(AuditSeverity::Warning), Level::WARN);
		assert_eq!(severity_to_level(AuditSeverity::Error), Level::ERROR);
		assert_eq!(severity_to_level(AuditSeverity::Critical), Level::ERROR);
	}

	#[test]
	fn tracing_sink_name() {
		let sink = TracingAuditSink::new(AuditFilterConfig::default());
		assert_eq!(sink.name(), "tracing");
	}

	#[test]
	fn tracing_sink_filter() {
		let filter = AuditFilterConfig {
			min_severity: AuditSeverity::Warning,
			include_events: None,
			exclude_events: None,
		};
		let sink = TracingAuditSink::new(filter.clone());
		assert_eq!(sink.filter().min_severity, AuditSeverity::Warning);
	}
}
