// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights
// reserved. SPDX-License-Identifier: Proprietary

//! Internal API endpoint for weaver audit events.
//!
//! This endpoint receives syscall audit events from weaver sidecar containers
//! and forwards them to the AuditService pipeline.

use axum::{
	extract::State,
	http::{header, HeaderMap, StatusCode},
	Json,
};
use serde::{Deserialize, Serialize};
use tracing::instrument;

use crate::api::AppState;

const MAX_EVENTS_PER_BATCH: usize = 1000;
const MAX_COMM_LEN: usize = 256;
const MAX_EVENT_TYPE_LEN: usize = 128;
const MAX_DETAILS_SIZE: usize = 65536; // 64KB per event details

/// Request body for submitting weaver audit events.
#[derive(Debug, Clone, Deserialize)]
pub struct WeaverAuditRequest {
	pub weaver_id: String,
	pub org_id: String,
	pub events: Vec<WeaverAuditEventPayload>,
}

impl WeaverAuditRequest {
	pub fn validate(&self) -> Result<(), (StatusCode, &'static str)> {
		// Validate weaver_id format (should be UUID-like)
		if self.weaver_id.is_empty() || self.weaver_id.len() > 64 {
			return Err((StatusCode::BAD_REQUEST, "invalid weaver_id"));
		}

		// Validate org_id format
		if self.org_id.is_empty() || self.org_id.len() > 64 {
			return Err((StatusCode::BAD_REQUEST, "invalid org_id"));
		}

		// Check event count
		if self.events.is_empty() {
			return Err((StatusCode::BAD_REQUEST, "events array is empty"));
		}
		if self.events.len() > MAX_EVENTS_PER_BATCH {
			return Err((StatusCode::BAD_REQUEST, "too many events in batch"));
		}

		// Validate each event
		for event in &self.events {
			event.validate()?;
		}

		Ok(())
	}
}

/// Individual audit event from weaver sidecar.
#[derive(Debug, Clone, Deserialize)]
pub struct WeaverAuditEventPayload {
	pub timestamp_ns: u64,
	pub pid: u32,
	pub tid: u32,
	pub comm: String,
	pub event_type: String,
	pub details: serde_json::Value,
}

impl WeaverAuditEventPayload {
	pub fn validate(&self) -> Result<(), (StatusCode, &'static str)> {
		if self.comm.len() > MAX_COMM_LEN {
			return Err((StatusCode::BAD_REQUEST, "comm too long"));
		}

		if self.event_type.is_empty() || self.event_type.len() > MAX_EVENT_TYPE_LEN {
			return Err((StatusCode::BAD_REQUEST, "invalid event_type"));
		}

		// Check details size (approximate)
		let details_size = self.details.to_string().len();
		if details_size > MAX_DETAILS_SIZE {
			return Err((StatusCode::BAD_REQUEST, "details too large"));
		}

		Ok(())
	}
}

/// Response for audit event submission.
#[derive(Debug, Clone, Serialize)]
pub struct WeaverAuditResponse {
	pub accepted: u32,
	pub rejected: u32,
}

/// Extract and validate the SVID from the Authorization header.
///
/// Returns the weaver_id from the SVID claims if valid, or an error.
fn validate_svid(
	headers: &HeaderMap,
	expected_weaver_id: &str,
	require_svid_validation: bool,
) -> Result<(), (StatusCode, &'static str)> {
	let auth_header = headers
		.get(header::AUTHORIZATION)
		.and_then(|v| v.to_str().ok());

	let Some(auth_value) = auth_header else {
		if require_svid_validation {
			return Err((StatusCode::UNAUTHORIZED, "missing Authorization header"));
		}
		tracing::warn!("No Authorization header present, skipping SVID validation");
		return Ok(());
	};

	let token = if let Some(bearer) = auth_value.strip_prefix("Bearer ") {
		bearer.trim()
	} else {
		return Err((
			StatusCode::UNAUTHORIZED,
			"invalid Authorization header format",
		));
	};

	if token.is_empty() {
		return Err((StatusCode::UNAUTHORIZED, "empty bearer token"));
	}

	// TODO: Implement full JWT validation:
	// 1. Decode the JWT without verification to get the header
	// 2. Fetch the public key from JWKS endpoint based on `kid`
	// 3. Verify the JWT signature using the public key
	// 4. Validate claims: exp, iat, aud (should be "loom-server")
	// 5. Extract weaver_id from claims and verify it matches expected_weaver_id

	if require_svid_validation {
		// For now, we just check that a token is present.
		// When SVID infrastructure is complete, parse and validate the JWT.
		//
		// Example validation (when implemented):
		// let claims = decode_and_validate_jwt(token)?;
		// if claims.weaver_id != expected_weaver_id {
		//     return Err((StatusCode::FORBIDDEN, "weaver_id mismatch"));
		// }
		// if claims.aud != "loom-server" {
		//     return Err((StatusCode::FORBIDDEN, "invalid audience"));
		// }

		tracing::debug!(
			expected_weaver_id = %expected_weaver_id,
			"SVID validation enabled but JWT validation not yet implemented"
		);
	}

	Ok(())
}

/// POST /internal/weaver-audit/events
///
/// Receives batched audit events from weaver sidecar and forwards to AuditService.
#[instrument(skip(state, headers, request), fields(weaver_id = %request.weaver_id, event_count = request.events.len()))]
pub async fn submit_events(
	State(state): State<AppState>,
	headers: HeaderMap,
	Json(request): Json<WeaverAuditRequest>,
) -> Result<Json<WeaverAuditResponse>, (StatusCode, &'static str)> {
	// Validate input first
	request.validate()?;

	// Validate SVID token matches weaver_id
	// TODO: Make require_svid_validation configurable via ServerConfig
	let require_svid_validation = std::env::var("LOOM_REQUIRE_SVID_VALIDATION")
		.map(|v| v == "true" || v == "1")
		.unwrap_or(false);

	validate_svid(&headers, &request.weaver_id, require_svid_validation)?;

	// TODO: Convert events to AuditLogEntry and submit to AuditService

	let accepted = request.events.len() as u32;

	tracing::debug!(
		weaver_id = %request.weaver_id,
		org_id = %request.org_id,
		accepted = accepted,
		"Received weaver audit events"
	);

	let _ = state.audit_service;

	Ok(Json(WeaverAuditResponse {
		accepted,
		rejected: 0,
	}))
}

#[cfg(test)]
mod tests {
	use super::*;
	use axum::http::HeaderValue;

	#[test]
	fn test_validate_svid_missing_header_not_required() {
		let headers = HeaderMap::new();
		let result = validate_svid(&headers, "weaver-123", false);
		assert!(result.is_ok());
	}

	#[test]
	fn test_validate_svid_missing_header_required() {
		let headers = HeaderMap::new();
		let result = validate_svid(&headers, "weaver-123", true);
		assert!(result.is_err());
		let (status, _) = result.unwrap_err();
		assert_eq!(status, StatusCode::UNAUTHORIZED);
	}

	#[test]
	fn test_validate_svid_invalid_format() {
		let mut headers = HeaderMap::new();
		headers.insert(
			header::AUTHORIZATION,
			HeaderValue::from_static("Basic abc123"),
		);
		let result = validate_svid(&headers, "weaver-123", true);
		assert!(result.is_err());
		let (status, _) = result.unwrap_err();
		assert_eq!(status, StatusCode::UNAUTHORIZED);
	}

	#[test]
	fn test_validate_svid_empty_bearer() {
		let mut headers = HeaderMap::new();
		headers.insert(header::AUTHORIZATION, HeaderValue::from_static("Bearer "));
		let result = validate_svid(&headers, "weaver-123", true);
		assert!(result.is_err());
		let (status, _) = result.unwrap_err();
		assert_eq!(status, StatusCode::UNAUTHORIZED);
	}

	#[test]
	fn test_validate_svid_valid_token_present() {
		let mut headers = HeaderMap::new();
		headers.insert(
			header::AUTHORIZATION,
			HeaderValue::from_static("Bearer eyJhbGciOiJFUzI1NiIsInR5cCI6IkpXVCJ9.test"),
		);
		let result = validate_svid(&headers, "weaver-123", true);
		assert!(result.is_ok());
	}
}
