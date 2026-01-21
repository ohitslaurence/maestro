// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use axum::{http::StatusCode, response::IntoResponse, Json};
use std::sync::Arc;
use tracing::instrument;

use loom_analytics_core::{AliasPayload, IdentifyPayload};
use loom_server_api::analytics::{
	AliasRequest, AnalyticsErrorResponse, IdentifyRequest, IdentifyResponse, SetPropertiesRequest,
};

use crate::middleware::AnalyticsApiKeyContext;
use crate::repository::AnalyticsRepository;

use super::capture::AnalyticsState;

const MAX_USER_ID_LENGTH: usize = 200;
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

fn validate_distinct_id(distinct_id: &str) -> Result<(), &'static str> {
	if distinct_id.is_empty() {
		return Err("Distinct ID cannot be empty");
	}
	if distinct_id.len() > MAX_DISTINCT_ID_LENGTH {
		return Err("Distinct ID exceeds maximum length");
	}
	Ok(())
}

fn validate_user_id(user_id: &str) -> Result<(), &'static str> {
	if user_id.is_empty() {
		return Err("User ID cannot be empty");
	}
	if user_id.len() > MAX_USER_ID_LENGTH {
		return Err("User ID exceeds maximum length");
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

#[instrument(skip(state, payload))]
pub async fn identify_impl<R: AnalyticsRepository>(
	state: Arc<AnalyticsState<R>>,
	api_key_ctx: AnalyticsApiKeyContext,
	payload: IdentifyRequest,
) -> impl IntoResponse {
	if let Err(msg) = validate_distinct_id(&payload.distinct_id) {
		return error_response("invalid_distinct_id", msg).into_response();
	}

	if let Err(msg) = validate_user_id(&payload.user_id) {
		return error_response("invalid_user_id", msg).into_response();
	}

	if let Err(msg) = validate_properties(&payload.properties) {
		return error_response("invalid_properties", msg).into_response();
	}

	let identify_payload =
		IdentifyPayload::new(payload.distinct_id, payload.user_id).with_properties(payload.properties);

	let result = match state
		.identity_service
		.identify(api_key_ctx.org_id, identify_payload)
		.await
	{
		Ok(person) => person,
		Err(e) => {
			tracing::error!(error = %e, "Failed to identify user");
			return internal_error("Failed to identify user").into_response();
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
		Json(IdentifyResponse {
			status: "ok".to_string(),
			person_id: result.person.id.to_string(),
		}),
	)
		.into_response()
}

#[instrument(skip(state, payload))]
pub async fn alias_impl<R: AnalyticsRepository>(
	state: Arc<AnalyticsState<R>>,
	api_key_ctx: AnalyticsApiKeyContext,
	payload: AliasRequest,
) -> impl IntoResponse {
	if let Err(msg) = validate_distinct_id(&payload.distinct_id) {
		return error_response("invalid_distinct_id", msg).into_response();
	}

	if let Err(msg) = validate_distinct_id(&payload.alias) {
		return error_response("invalid_alias", msg).into_response();
	}

	let alias_payload = AliasPayload::new(payload.distinct_id, payload.alias);

	let result = match state
		.identity_service
		.alias(api_key_ctx.org_id, alias_payload)
		.await
	{
		Ok(person) => person,
		Err(e) => {
			tracing::error!(error = %e, "Failed to create alias");
			return internal_error("Failed to create alias").into_response();
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
		Json(IdentifyResponse {
			status: "ok".to_string(),
			person_id: result.person.id.to_string(),
		}),
	)
		.into_response()
}

#[instrument(skip(state, payload))]
pub async fn set_properties_impl<R: AnalyticsRepository>(
	state: Arc<AnalyticsState<R>>,
	api_key_ctx: AnalyticsApiKeyContext,
	payload: SetPropertiesRequest,
) -> impl IntoResponse {
	if let Err(msg) = validate_distinct_id(&payload.distinct_id) {
		return error_response("invalid_distinct_id", msg).into_response();
	}

	if let Err(msg) = validate_properties(&payload.properties) {
		return error_response("invalid_properties", msg).into_response();
	}

	// Resolve person for this distinct_id
	let person_with_identities = match state
		.identity_service
		.resolve_person_for_distinct_id(api_key_ctx.org_id, &payload.distinct_id)
		.await
	{
		Ok(p) => p,
		Err(e) => {
			tracing::error!(error = %e, "Failed to resolve person");
			return internal_error("Failed to resolve person").into_response();
		}
	};

	let mut person = person_with_identities.person;

	if payload.set_once {
		// Only set properties that don't already exist
		if let (serde_json::Value::Object(existing), serde_json::Value::Object(new)) =
			(&mut person.properties, &payload.properties)
		{
			for (key, value) in new {
				existing.entry(key.clone()).or_insert(value.clone());
			}
		}
	} else {
		// Overwrite properties
		person.set_properties(payload.properties);
	}

	if let Err(e) = state.repository.update_person(&person).await {
		tracing::error!(error = %e, "Failed to update person properties");
		return internal_error("Failed to set properties").into_response();
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
		Json(IdentifyResponse {
			status: "ok".to_string(),
			person_id: person.id.to_string(),
		}),
	)
		.into_response()
}

#[cfg(test)]
mod tests {
	use super::*;

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
	fn validate_user_id_accepts_valid_ids() {
		assert!(validate_user_id("user@example.com").is_ok());
		assert!(validate_user_id("user123").is_ok());
		assert!(validate_user_id("550e8400-e29b-41d4-a716-446655440000").is_ok());
	}

	#[test]
	fn validate_user_id_rejects_invalid_ids() {
		assert!(validate_user_id("").is_err());
		assert!(validate_user_id(&"a".repeat(201)).is_err());
	}

	#[test]
	fn validate_properties_accepts_valid_json() {
		assert!(validate_properties(&serde_json::json!({})).is_ok());
		assert!(validate_properties(&serde_json::json!({"name": "Alice"})).is_ok());
		assert!(validate_properties(&serde_json::json!({"plan": "pro", "company": "Acme"})).is_ok());
	}

	#[test]
	fn validate_properties_rejects_oversized_json() {
		let large_string = "x".repeat(MAX_PROPERTIES_SIZE + 1);
		assert!(validate_properties(&serde_json::json!({"data": large_string})).is_err());
	}
}
