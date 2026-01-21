// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::event::AuditLogEntry;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GeoIpInfo {
	pub city: Option<String>,
	pub country: Option<String>,
	pub country_code: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SessionContext {
	pub session_id: Option<String>,
	pub session_type: Option<String>,
	pub device_label: Option<String>,
	pub geo: Option<GeoIpInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct OrgContext {
	pub org_id: Option<String>,
	pub org_slug: Option<String>,
	pub org_role: Option<String>,
	pub team_id: Option<String>,
	pub team_role: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnrichedAuditEvent {
	pub base: AuditLogEntry,
	pub session: Option<SessionContext>,
	pub org: Option<OrgContext>,
}

#[async_trait]
pub trait AuditEnricher: Send + Sync {
	async fn enrich(&self, event: AuditLogEntry) -> EnrichedAuditEvent;
}

#[derive(Debug, Clone, Default)]
pub struct NoopEnricher;

#[async_trait]
impl AuditEnricher for NoopEnricher {
	async fn enrich(&self, event: AuditLogEntry) -> EnrichedAuditEvent {
		EnrichedAuditEvent {
			base: event,
			session: None,
			org: None,
		}
	}
}
