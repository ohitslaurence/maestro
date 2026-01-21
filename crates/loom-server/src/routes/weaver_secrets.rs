// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights
// reserved. SPDX-License-Identifier: Proprietary

//! Weaver secrets access routes.
//!
//! These routes allow weavers to fetch secrets using their Weaver SVID.
//!
//! Endpoints:
//! - `GET /internal/weaver-secrets/v1/secrets/{scope}/{name}` - Fetch a secret

use axum::{
	extract::{Path, State},
	http::{HeaderMap, StatusCode},
	response::IntoResponse,
	Json,
};
use serde::Serialize;
use tracing::{info, instrument, warn};
use utoipa::ToSchema;

use crate::api::AppState;

use super::weaver_auth::ErrorResponse;

#[derive(Debug, Serialize, ToSchema)]
pub struct SecretResponse {
	pub name: String,
	pub scope: String,
	pub version: i32,
	pub value: String,
}

fn extract_bearer_token(headers: &HeaderMap) -> Result<&str, (StatusCode, Json<ErrorResponse>)> {
	let auth_header = headers.get("authorization").ok_or_else(|| {
		(
			StatusCode::UNAUTHORIZED,
			Json(ErrorResponse {
				error: "missing_token".to_string(),
				message: "Authorization header required".to_string(),
			}),
		)
	})?;

	let auth_str = auth_header.to_str().map_err(|_| {
		(
			StatusCode::BAD_REQUEST,
			Json(ErrorResponse {
				error: "invalid_header".to_string(),
				message: "Invalid authorization header encoding".to_string(),
			}),
		)
	})?;

	auth_str.strip_prefix("Bearer ").ok_or_else(|| {
		(
			StatusCode::BAD_REQUEST,
			Json(ErrorResponse {
				error: "invalid_token_format".to_string(),
				message: "Authorization header must be 'Bearer <token>'".to_string(),
			}),
		)
	})
}

fn scope_from_path(scope_str: &str) -> Option<&'static str> {
	match scope_str {
		"org" => Some("org"),
		"repo" => Some("repo"),
		"weaver" => Some("weaver"),
		_ => None,
	}
}

#[utoipa::path(
    get,
    path = "/internal/weaver-secrets/v1/secrets/{scope}/{name}",
    params(
        ("scope" = String, Path, description = "Secret scope: org, repo, or weaver"),
        ("name" = String, Path, description = "Secret name (uppercase with underscores)")
    ),
    responses(
        (status = 200, description = "Secret retrieved successfully", body = SecretResponse),
        (status = 400, description = "Invalid request", body = ErrorResponse),
        (status = 401, description = "Invalid or expired SVID", body = ErrorResponse),
        (status = 403, description = "Access denied", body = ErrorResponse),
        (status = 404, description = "Secret not found", body = ErrorResponse),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    ),
    tag = "weaver-secrets"
)]
#[instrument(skip(state, headers), fields(scope = %scope, name = %name))]
pub async fn get_secret(
	State(state): State<AppState>,
	headers: HeaderMap,
	Path((scope, name)): Path<(String, String)>,
) -> impl IntoResponse {
	let svid_issuer = match state.svid_issuer.as_ref() {
		Some(issuer) => issuer,
		None => {
			warn!("SVID issuer not configured");
			return (
				StatusCode::SERVICE_UNAVAILABLE,
				Json(ErrorResponse {
					error: "service_unavailable".to_string(),
					message: "Secrets service not configured".to_string(),
				}),
			)
				.into_response();
		}
	};

	let secrets_service = match state.secrets_service.as_ref() {
		Some(service) => service,
		None => {
			warn!("Secrets service not configured");
			return (
				StatusCode::SERVICE_UNAVAILABLE,
				Json(ErrorResponse {
					error: "service_unavailable".to_string(),
					message: "Secrets service not configured".to_string(),
				}),
			)
				.into_response();
		}
	};

	let token = match extract_bearer_token(&headers) {
		Ok(t) => t,
		Err(resp) => return resp.into_response(),
	};

	let claims = match svid_issuer.verify_svid(token).await {
		Ok(c) => c,
		Err(e) => {
			warn!(error = %e, "SVID validation failed");
			let status = match &e {
				loom_server_secrets::SecretsError::SvidExpired => StatusCode::UNAUTHORIZED,
				loom_server_secrets::SecretsError::SvidInvalidSignature => StatusCode::UNAUTHORIZED,
				loom_server_secrets::SecretsError::SvidInvalidIssuer => StatusCode::UNAUTHORIZED,
				loom_server_secrets::SecretsError::SvidInvalidAudience => StatusCode::UNAUTHORIZED,
				loom_server_secrets::SecretsError::SvidNotYetValid => StatusCode::UNAUTHORIZED,
				loom_server_secrets::SecretsError::SvidValidation(_) => StatusCode::UNAUTHORIZED,
				_ => StatusCode::INTERNAL_SERVER_ERROR,
			};
			return (
				status,
				Json(ErrorResponse {
					error: "svid_validation_failed".to_string(),
					message: "SVID validation failed".to_string(),
				}),
			)
				.into_response();
		}
	};

	let scope_type = match scope_from_path(&scope) {
		Some(s) => s,
		None => {
			return (
				StatusCode::BAD_REQUEST,
				Json(ErrorResponse {
					error: "invalid_scope".to_string(),
					message: "Scope must be one of: org, repo, weaver".to_string(),
				}),
			)
				.into_response();
		}
	};

	let secret_scope = match build_secret_scope(scope_type, &claims) {
		Ok(s) => s,
		Err(resp) => return resp.into_response(),
	};

	let secret_value = match secrets_service
		.get_secret_for_weaver(&claims, secret_scope, &name)
		.await
	{
		Ok(v) => v,
		Err(e) => {
			let status =
				StatusCode::from_u16(e.status_code()).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
			let error_code = match &e {
				loom_server_secrets::SecretsError::SecretNotFound(_) => "secret_not_found",
				loom_server_secrets::SecretsError::AccessDenied(_) => "access_denied",
				loom_server_secrets::SecretsError::SecretDisabled(_) => "secret_disabled",
				_ => "internal_error",
			};

			if e.is_internal() {
				warn!(error = %e, "Internal error fetching secret");
			}

			return (
				status,
				Json(ErrorResponse {
					error: error_code.to_string(),
					message: e.to_string(),
				}),
			)
				.into_response();
		}
	};

	info!(
		weaver_id = %claims.weaver_id,
		secret_name = %name,
		version = secret_value.version,
		"Secret accessed by weaver"
	);

	(
		StatusCode::OK,
		Json(SecretResponse {
			name: secret_value.name,
			scope: secret_value.scope.as_str().to_string(),
			version: secret_value.version,
			value: secret_value.value.expose().to_string(),
		}),
	)
		.into_response()
}

fn build_secret_scope(
	scope_type: &str,
	claims: &loom_server_secrets::WeaverClaims,
) -> Result<loom_server_secrets::types::SecretScope, (StatusCode, Json<ErrorResponse>)> {
	use loom_server_auth::types::OrgId;
	use loom_server_secrets::types::{SecretScope, WeaverId};

	match scope_type {
		"org" => {
			let org_id = uuid::Uuid::parse_str(&claims.org_id).map_err(|_| {
				(
					StatusCode::BAD_REQUEST,
					Json(ErrorResponse {
						error: "invalid_claim".to_string(),
						message: "Invalid org_id in SVID claims".to_string(),
					}),
				)
			})?;
			Ok(SecretScope::Org {
				org_id: OrgId::new(org_id),
			})
		}
		"repo" => {
			let org_id = uuid::Uuid::parse_str(&claims.org_id).map_err(|_| {
				(
					StatusCode::BAD_REQUEST,
					Json(ErrorResponse {
						error: "invalid_claim".to_string(),
						message: "Invalid org_id in SVID claims".to_string(),
					}),
				)
			})?;
			let repo_id = claims.repo_id.as_ref().ok_or_else(|| {
				(
					StatusCode::FORBIDDEN,
					Json(ErrorResponse {
						error: "no_repo_context".to_string(),
						message: "Weaver has no repository context for repo-scoped secrets".to_string(),
					}),
				)
			})?;
			Ok(SecretScope::Repo {
				org_id: OrgId::new(org_id),
				repo_id: repo_id.clone(),
			})
		}
		"weaver" => {
			let weaver_uuid = claims.weaver_id.parse::<uuid7::Uuid>().map_err(|_| {
				(
					StatusCode::BAD_REQUEST,
					Json(ErrorResponse {
						error: "invalid_claim".to_string(),
						message: "Invalid weaver_id in SVID claims".to_string(),
					}),
				)
			})?;
			Ok(SecretScope::Weaver {
				weaver_id: WeaverId::new(weaver_uuid),
			})
		}
		_ => Err((
			StatusCode::BAD_REQUEST,
			Json(ErrorResponse {
				error: "invalid_scope".to_string(),
				message: "Scope must be one of: org, repo, weaver".to_string(),
			}),
		)),
	}
}
