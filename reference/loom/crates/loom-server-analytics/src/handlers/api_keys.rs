// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use axum::{http::StatusCode, response::IntoResponse, Json};
use chrono::Utc;
use tracing::instrument;

use loom_analytics_core::{AnalyticsApiKey, AnalyticsApiKeyId, AnalyticsKeyType, OrgId, UserId};
use loom_server_api::analytics::{
	AnalyticsApiKeyResponse, AnalyticsErrorResponse, AnalyticsKeyTypeApi, AnalyticsSuccessResponse,
	CreateAnalyticsApiKeyRequest, CreateAnalyticsApiKeyResponse, ListAnalyticsApiKeysResponse,
};

use crate::api_key::hash_api_key;
use crate::repository::AnalyticsRepository;

use super::capture::AnalyticsState;

fn error_response(
	status: StatusCode,
	error: &str,
	message: &str,
) -> (StatusCode, Json<AnalyticsErrorResponse>) {
	(
		status,
		Json(AnalyticsErrorResponse {
			error: error.to_string(),
			message: message.to_string(),
		}),
	)
}

fn not_found(message: &str) -> (StatusCode, Json<AnalyticsErrorResponse>) {
	error_response(StatusCode::NOT_FOUND, "not_found", message)
}

fn internal_error(message: &str) -> (StatusCode, Json<AnalyticsErrorResponse>) {
	error_response(StatusCode::INTERNAL_SERVER_ERROR, "internal_error", message)
}

pub fn generate_api_key(key_type: AnalyticsKeyType) -> String {
	AnalyticsApiKey::generate_key(key_type)
}

pub fn api_key_type_to_api(key_type: AnalyticsKeyType) -> AnalyticsKeyTypeApi {
	match key_type {
		AnalyticsKeyType::Write => AnalyticsKeyTypeApi::Write,
		AnalyticsKeyType::ReadWrite => AnalyticsKeyTypeApi::ReadWrite,
	}
}

pub fn api_key_type_from_api(key_type: AnalyticsKeyTypeApi) -> AnalyticsKeyType {
	match key_type {
		AnalyticsKeyTypeApi::Write => AnalyticsKeyType::Write,
		AnalyticsKeyTypeApi::ReadWrite => AnalyticsKeyType::ReadWrite,
	}
}

pub fn api_key_to_response(key: &AnalyticsApiKey) -> AnalyticsApiKeyResponse {
	AnalyticsApiKeyResponse {
		id: key.id.to_string(),
		org_id: key.org_id.to_string(),
		name: key.name.clone(),
		key_type: api_key_type_to_api(key.key_type),
		created_by: key.created_by.to_string(),
		created_at: key.created_at,
		last_used_at: key.last_used_at,
		revoked_at: key.revoked_at,
	}
}

#[derive(Debug, Clone)]
pub struct UserAuthContext {
	pub user_id: UserId,
	pub org_id: OrgId,
}

#[instrument(skip(state))]
pub async fn list_api_keys_impl<R: AnalyticsRepository>(
	state: std::sync::Arc<AnalyticsState<R>>,
	user_ctx: UserAuthContext,
) -> impl IntoResponse {
	let keys = match state.repository.list_api_keys(user_ctx.org_id).await {
		Ok(k) => k,
		Err(e) => {
			tracing::error!(error = %e, "Failed to list API keys");
			return internal_error("Failed to list API keys").into_response();
		}
	};

	let key_responses: Vec<AnalyticsApiKeyResponse> = keys.iter().map(api_key_to_response).collect();

	(
		StatusCode::OK,
		Json(ListAnalyticsApiKeysResponse {
			api_keys: key_responses,
		}),
	)
		.into_response()
}

#[instrument(skip(state, payload))]
pub async fn create_api_key_impl<R: AnalyticsRepository>(
	state: std::sync::Arc<AnalyticsState<R>>,
	user_ctx: UserAuthContext,
	payload: CreateAnalyticsApiKeyRequest,
) -> impl IntoResponse {
	if payload.name.is_empty() {
		return error_response(
			StatusCode::BAD_REQUEST,
			"invalid_name",
			"Name cannot be empty",
		)
		.into_response();
	}

	if payload.name.len() > 100 {
		return error_response(
			StatusCode::BAD_REQUEST,
			"invalid_name",
			"Name exceeds maximum length",
		)
		.into_response();
	}

	let key_type = api_key_type_from_api(payload.key_type);
	let raw_key = generate_api_key(key_type);

	let key_hash = match hash_api_key(&raw_key) {
		Ok(h) => h,
		Err(e) => {
			tracing::error!(error = %e, "Failed to hash API key");
			return internal_error("Failed to create API key").into_response();
		}
	};

	let api_key = AnalyticsApiKey {
		id: AnalyticsApiKeyId::new(),
		org_id: user_ctx.org_id,
		name: payload.name.clone(),
		key_type,
		key_hash,
		created_by: user_ctx.user_id,
		created_at: Utc::now(),
		last_used_at: None,
		revoked_at: None,
	};

	if let Err(e) = state.repository.create_api_key(&api_key).await {
		tracing::error!(error = %e, "Failed to create API key");
		return internal_error("Failed to create API key").into_response();
	}

	(
		StatusCode::CREATED,
		Json(CreateAnalyticsApiKeyResponse {
			id: api_key.id.to_string(),
			key: raw_key,
			name: api_key.name,
			key_type: api_key_type_to_api(api_key.key_type),
			created_at: api_key.created_at,
		}),
	)
		.into_response()
}

#[instrument(skip(state))]
pub async fn revoke_api_key_impl<R: AnalyticsRepository>(
	state: std::sync::Arc<AnalyticsState<R>>,
	user_ctx: UserAuthContext,
	key_id: String,
) -> impl IntoResponse {
	let key_id: AnalyticsApiKeyId = match key_id.parse() {
		Ok(id) => id,
		Err(_) => {
			return error_response(
				StatusCode::BAD_REQUEST,
				"invalid_id",
				"Invalid API key ID format",
			)
			.into_response();
		}
	};

	// Verify the key belongs to the user's org
	let key = match state.repository.get_api_key_by_id(key_id).await {
		Ok(Some(k)) => k,
		Ok(None) => {
			return not_found("API key not found").into_response();
		}
		Err(e) => {
			tracing::error!(error = %e, "Failed to get API key");
			return internal_error("Failed to revoke API key").into_response();
		}
	};

	if key.org_id != user_ctx.org_id {
		return not_found("API key not found").into_response();
	}

	if key.revoked_at.is_some() {
		return error_response(
			StatusCode::BAD_REQUEST,
			"already_revoked",
			"API key is already revoked",
		)
		.into_response();
	}

	match state.repository.revoke_api_key(key_id).await {
		Ok(true) => {}
		Ok(false) => {
			return not_found("API key not found").into_response();
		}
		Err(e) => {
			tracing::error!(error = %e, "Failed to revoke API key");
			return internal_error("Failed to revoke API key").into_response();
		}
	}

	(
		StatusCode::OK,
		Json(AnalyticsSuccessResponse {
			message: "API key revoked successfully".to_string(),
		}),
	)
		.into_response()
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn generate_api_key_has_correct_prefix() {
		let write_key = generate_api_key(AnalyticsKeyType::Write);
		assert!(write_key.starts_with(AnalyticsApiKey::WRITE_PREFIX));
		// Key format: prefix + 32 hex chars
		assert_eq!(write_key.len(), AnalyticsApiKey::WRITE_PREFIX.len() + 32);

		let rw_key = generate_api_key(AnalyticsKeyType::ReadWrite);
		assert!(rw_key.starts_with(AnalyticsApiKey::READ_WRITE_PREFIX));
		assert_eq!(rw_key.len(), AnalyticsApiKey::READ_WRITE_PREFIX.len() + 32);
	}

	#[test]
	fn generate_api_key_is_unique() {
		let key1 = generate_api_key(AnalyticsKeyType::Write);
		let key2 = generate_api_key(AnalyticsKeyType::Write);
		assert_ne!(key1, key2);
	}

	#[test]
	fn api_key_type_conversion_roundtrips() {
		assert_eq!(
			api_key_type_from_api(api_key_type_to_api(AnalyticsKeyType::Write)),
			AnalyticsKeyType::Write
		);
		assert_eq!(
			api_key_type_from_api(api_key_type_to_api(AnalyticsKeyType::ReadWrite)),
			AnalyticsKeyType::ReadWrite
		);
	}
}
