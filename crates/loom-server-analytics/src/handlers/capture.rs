// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use axum::{
	http::{HeaderMap, StatusCode},
	response::IntoResponse,
	Json,
};
use chrono::Utc;
use loom_common_secret::SecretString;
use std::sync::Arc;
use tracing::instrument;

use loom_analytics_core::Event;
use loom_server_api::analytics::{
	AnalyticsErrorResponse, BatchCaptureRequest, CaptureEventRequest, CaptureResponse,
};

use crate::middleware::AnalyticsApiKeyContext;
use crate::repository::AnalyticsRepository;

const MAX_EVENT_NAME_LENGTH: usize = 200;
const MAX_DISTINCT_ID_LENGTH: usize = 200;
const MAX_PROPERTIES_SIZE: usize = 1024 * 1024; // 1MB

fn error_response(error: &str, message: &str) -> impl IntoResponse {
	(
		StatusCode::BAD_REQUEST,
		Json(AnalyticsErrorResponse {
			error: error.to_string(),
			message: message.to_string(),
		}),
	)
}

fn internal_error(message: &str) -> impl IntoResponse {
	(
		StatusCode::INTERNAL_SERVER_ERROR,
		Json(AnalyticsErrorResponse {
			error: "internal_error".to_string(),
			message: message.to_string(),
		}),
	)
}

fn validate_event_name(name: &str) -> Result<(), &'static str> {
	if name.is_empty() {
		return Err("Event name cannot be empty");
	}
	if name.len() > MAX_EVENT_NAME_LENGTH {
		return Err("Event name exceeds maximum length");
	}
	let valid = name
		.chars()
		.all(|c| c.is_alphanumeric() || c == '_' || c == '$' || c == '.');
	if !valid {
		return Err("Event name contains invalid characters");
	}
	Ok(())
}

fn validate_distinct_id(distinct_id: &str) -> Result<(), &'static str> {
	if distinct_id.is_empty() {
		return Err("Distinct ID cannot be empty");
	}
	if distinct_id.len() > MAX_DISTINCT_ID_LENGTH {
		return Err("Distinct ID exceeds maximum length");
	}
	Ok(())
}

fn validate_properties(properties: &serde_json::Value) -> Result<(), &'static str> {
	let serialized = serde_json::to_string(properties).unwrap_or_default();
	if serialized.len() > MAX_PROPERTIES_SIZE {
		return Err("Properties exceed maximum size");
	}
	Ok(())
}

fn extract_client_info(
	headers: &HeaderMap,
) -> (
	Option<SecretString>,
	Option<String>,
	Option<String>,
	Option<String>,
) {
	let ip_address = headers
		.get("x-forwarded-for")
		.and_then(|v| v.to_str().ok())
		.map(|s| s.split(',').next().unwrap_or(s).trim().to_string())
		.or_else(|| {
			headers
				.get("x-real-ip")
				.and_then(|v| v.to_str().ok())
				.map(|s| s.to_string())
		})
		.map(SecretString::new);

	let user_agent = headers
		.get("user-agent")
		.and_then(|v| v.to_str().ok())
		.map(|s| s.to_string());

	let lib = headers
		.get("x-loom-lib")
		.and_then(|v| v.to_str().ok())
		.map(|s| s.to_string());

	let lib_version = headers
		.get("x-loom-lib-version")
		.and_then(|v| v.to_str().ok())
		.map(|s| s.to_string());

	(ip_address, user_agent, lib, lib_version)
}

#[instrument(skip(state, headers, payload))]
pub async fn capture_event_impl<R: AnalyticsRepository>(
	state: Arc<AnalyticsState<R>>,
	api_key_ctx: AnalyticsApiKeyContext,
	headers: HeaderMap,
	payload: CaptureEventRequest,
) -> impl IntoResponse {
	if let Err(msg) = validate_event_name(&payload.event) {
		return error_response("invalid_event_name", msg).into_response();
	}

	if let Err(msg) = validate_distinct_id(&payload.distinct_id) {
		return error_response("invalid_distinct_id", msg).into_response();
	}

	if let Err(msg) = validate_properties(&payload.properties) {
		return error_response("invalid_properties", msg).into_response();
	}

	let (ip_address, user_agent, lib, lib_version) = extract_client_info(&headers);

	let mut event = Event::new(api_key_ctx.org_id, payload.distinct_id, payload.event);
	event.properties = payload.properties;
	event.timestamp = payload.timestamp.unwrap_or_else(Utc::now);
	event.ip_address = ip_address;
	event.user_agent = user_agent;
	event.lib = lib;
	event.lib_version = lib_version;

	// Resolve person for this distinct_id and set person_id
	match state
		.identity_service
		.resolve_person_for_distinct_id(api_key_ctx.org_id, &event.distinct_id)
		.await
	{
		Ok(person_with_identities) => {
			event.person_id = Some(person_with_identities.person.id);
		}
		Err(e) => {
			tracing::warn!(error = %e, "Failed to resolve person for event, continuing without person_id");
		}
	}

	let event_id = event.id.to_string();
	if let Err(e) = state.repository.insert_event(&event).await {
		tracing::error!(error = %e, "Failed to insert event");
		return internal_error("Failed to capture event").into_response();
	}

	// Update last_used_at for the API key
	if let Err(e) = state
		.repository
		.update_api_key_last_used(api_key_ctx.api_key_id)
		.await
	{
		tracing::warn!(error = %e, "Failed to update API key last_used_at");
	}

	(
		StatusCode::OK,
		Json(CaptureResponse {
			status: "ok".to_string(),
			event_id: Some(event_id),
			count: None,
		}),
	)
		.into_response()
}

#[instrument(skip(state, headers, payload))]
pub async fn batch_capture_impl<R: AnalyticsRepository>(
	state: Arc<AnalyticsState<R>>,
	api_key_ctx: AnalyticsApiKeyContext,
	headers: HeaderMap,
	payload: BatchCaptureRequest,
) -> impl IntoResponse {
	if payload.batch.is_empty() {
		return error_response("empty_batch", "Batch cannot be empty").into_response();
	}

	if payload.batch.len() > 100 {
		return error_response("batch_too_large", "Batch cannot exceed 100 events").into_response();
	}

	// Validate all events first
	for (i, event_req) in payload.batch.iter().enumerate() {
		if let Err(msg) = validate_event_name(&event_req.event) {
			return error_response("invalid_event_name", &format!("Event {}: {}", i, msg))
				.into_response();
		}
		if let Err(msg) = validate_distinct_id(&event_req.distinct_id) {
			return error_response("invalid_distinct_id", &format!("Event {}: {}", i, msg))
				.into_response();
		}
		if let Err(msg) = validate_properties(&event_req.properties) {
			return error_response("invalid_properties", &format!("Event {}: {}", i, msg))
				.into_response();
		}
	}

	let (ip_address, user_agent, lib, lib_version) = extract_client_info(&headers);

	let mut events = Vec::with_capacity(payload.batch.len());
	for event_req in payload.batch {
		let mut event = Event::new(
			api_key_ctx.org_id,
			event_req.distinct_id.clone(),
			event_req.event,
		);
		event.properties = event_req.properties;
		event.timestamp = event_req.timestamp.unwrap_or_else(Utc::now);
		event.ip_address = ip_address.clone();
		event.user_agent = user_agent.clone();
		event.lib = lib.clone();
		event.lib_version = lib_version.clone();

		// Resolve person for this distinct_id
		match state
			.identity_service
			.resolve_person_for_distinct_id(api_key_ctx.org_id, &event.distinct_id)
			.await
		{
			Ok(person_with_identities) => {
				event.person_id = Some(person_with_identities.person.id);
			}
			Err(e) => {
				tracing::warn!(error = %e, distinct_id = %event.distinct_id, "Failed to resolve person for event");
			}
		}

		events.push(event);
	}

	let count = match state.repository.insert_events(&events).await {
		Ok(count) => count,
		Err(e) => {
			tracing::error!(error = %e, "Failed to insert batch events");
			return internal_error("Failed to capture events").into_response();
		}
	};

	// Update last_used_at for the API key
	if let Err(e) = state
		.repository
		.update_api_key_last_used(api_key_ctx.api_key_id)
		.await
	{
		tracing::warn!(error = %e, "Failed to update API key last_used_at");
	}

	(
		StatusCode::OK,
		Json(CaptureResponse {
			status: "ok".to_string(),
			event_id: None,
			count: Some(count),
		}),
	)
		.into_response()
}

pub struct AnalyticsState<R: AnalyticsRepository> {
	pub repository: R,
	pub identity_service: crate::IdentityResolutionService<R>,
}

impl<R: AnalyticsRepository + Clone> AnalyticsState<R> {
	/// Creates a new analytics state without an audit hook.
	pub fn new(repository: R) -> Self {
		let identity_service = crate::IdentityResolutionService::new(repository.clone());
		Self {
			repository,
			identity_service,
		}
	}

	/// Creates a new analytics state with an audit hook for person merges.
	pub fn with_audit_hook(repository: R, hook: crate::SharedMergeAuditHook) -> Self {
		let identity_service =
			crate::IdentityResolutionService::with_audit_hook(repository.clone(), hook);
		Self {
			repository,
			identity_service,
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn validate_event_name_accepts_valid_names() {
		assert!(validate_event_name("button_clicked").is_ok());
		assert!(validate_event_name("$pageview").is_ok());
		assert!(validate_event_name("checkout.completed").is_ok());
		assert!(validate_event_name("User123").is_ok());
	}

	#[test]
	fn validate_event_name_rejects_invalid_names() {
		assert!(validate_event_name("").is_err());
		assert!(validate_event_name(&"a".repeat(201)).is_err());
		assert!(validate_event_name("event with spaces").is_err());
		assert!(validate_event_name("event-with-dashes").is_err());
	}

	#[test]
	fn validate_distinct_id_accepts_valid_ids() {
		assert!(validate_distinct_id("user123").is_ok());
		assert!(validate_distinct_id("550e8400-e29b-41d4-a716-446655440000").is_ok());
		assert!(validate_distinct_id("email@example.com").is_ok());
	}

	#[test]
	fn validate_distinct_id_rejects_invalid_ids() {
		assert!(validate_distinct_id("").is_err());
		assert!(validate_distinct_id(&"a".repeat(201)).is_err());
	}

	#[test]
	fn validate_properties_accepts_valid_json() {
		assert!(validate_properties(&serde_json::json!({})).is_ok());
		assert!(validate_properties(&serde_json::json!({"key": "value"})).is_ok());
		assert!(validate_properties(&serde_json::json!({"nested": {"key": 123}})).is_ok());
	}

	#[test]
	fn validate_properties_rejects_oversized_json() {
		let large_string = "x".repeat(MAX_PROPERTIES_SIZE + 1);
		assert!(validate_properties(&serde_json::json!({"data": large_string})).is_err());
	}
}
