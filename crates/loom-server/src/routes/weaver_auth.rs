// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights
// reserved. SPDX-License-Identifier: Proprietary

//! Weaver authentication routes for SVID issuance.
//!
//! These routes allow weavers to exchange their K8s service account JWT
//! for a Weaver SVID (SPIFFE Verifiable Identity Document).
//!
//! Endpoints:
//! - `POST /internal/weaver-auth/token` - Exchange K8s SA JWT for Weaver SVID
//! - `GET /internal/weaver-auth/.well-known/jwks.json` - JWKS discovery endpoint

use axum::{
	extract::State,
	http::{HeaderMap, StatusCode},
	response::IntoResponse,
	Json,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tracing::{info, instrument, warn};
use utoipa::ToSchema;

use crate::api::AppState;

const MANAGED_LABEL: &str = "loom.dev/managed";
const WEAVER_ID_LABEL: &str = "loom.dev/weaver-id";
const ORG_ID_LABEL: &str = "loom.dev/org-id";
const REPO_ID_LABEL: &str = "loom.dev/repo-id";
const OWNER_USER_ID_LABEL: &str = "loom.dev/owner-user-id";

#[derive(Debug, Deserialize, ToSchema)]
pub struct TokenRequest {
	pub pod_name: String,
	pub pod_namespace: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct SvidResponse {
	pub token: String,
	pub token_type: String,
	pub expires_at: DateTime<Utc>,
	pub spiffe_id: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ErrorResponse {
	pub error: String,
	pub message: String,
}

#[utoipa::path(
    post,
    path = "/internal/weaver-auth/token",
    request_body = TokenRequest,
    responses(
        (status = 200, description = "SVID issued successfully", body = SvidResponse),
        (status = 400, description = "Invalid request", body = ErrorResponse),
        (status = 401, description = "K8s token validation failed", body = ErrorResponse),
        (status = 403, description = "Pod not managed by Loom", body = ErrorResponse),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    ),
    tag = "weaver-auth"
)]
#[instrument(skip(state, headers, payload), fields(pod_name = %payload.pod_name, pod_namespace = %payload.pod_namespace))]
pub async fn exchange_token(
	State(state): State<AppState>,
	headers: HeaderMap,
	Json(payload): Json<TokenRequest>,
) -> impl IntoResponse {
	let k8s_client = match state.k8s_client.as_ref() {
		Some(client) => client,
		None => {
			warn!("K8s client not configured");
			return (
				StatusCode::SERVICE_UNAVAILABLE,
				Json(ErrorResponse {
					error: "service_unavailable".to_string(),
					message: "K8s client not configured".to_string(),
				}),
			)
				.into_response();
		}
	};

	let svid_issuer = match state.svid_issuer.as_ref() {
		Some(issuer) => issuer,
		None => {
			warn!("SVID issuer not configured");
			return (
				StatusCode::SERVICE_UNAVAILABLE,
				Json(ErrorResponse {
					error: "service_unavailable".to_string(),
					message: "SVID issuance not configured".to_string(),
				}),
			)
				.into_response();
		}
	};

	let auth_header = match headers.get("authorization") {
		Some(h) => h,
		None => {
			return (
				StatusCode::UNAUTHORIZED,
				Json(ErrorResponse {
					error: "missing_token".to_string(),
					message: "Authorization header required".to_string(),
				}),
			)
				.into_response();
		}
	};

	let auth_str = match auth_header.to_str() {
		Ok(s) => s,
		Err(_) => {
			return (
				StatusCode::BAD_REQUEST,
				Json(ErrorResponse {
					error: "invalid_header".to_string(),
					message: "Invalid authorization header encoding".to_string(),
				}),
			)
				.into_response();
		}
	};

	let k8s_token = match auth_str.strip_prefix("Bearer ") {
		Some(token) => token,
		None => {
			return (
				StatusCode::BAD_REQUEST,
				Json(ErrorResponse {
					error: "invalid_token_format".to_string(),
					message: "Authorization header must be 'Bearer <token>'".to_string(),
				}),
			)
				.into_response();
		}
	};

	let token_review_result = match k8s_client.validate_token(k8s_token, &[]).await {
		Ok(result) => result,
		Err(e) => {
			warn!(error = %e, "Token validation failed");
			return (
				StatusCode::UNAUTHORIZED,
				Json(ErrorResponse {
					error: "token_validation_failed".to_string(),
					message: "K8s token validation failed".to_string(),
				}),
			)
				.into_response();
		}
	};

	if !token_review_result.authenticated {
		return (
			StatusCode::UNAUTHORIZED,
			Json(ErrorResponse {
				error: "unauthenticated".to_string(),
				message: token_review_result
					.error
					.unwrap_or_else(|| "Token not authenticated".to_string()),
			}),
		)
			.into_response();
	}

	let pod = match k8s_client
		.get_pod(&payload.pod_name, &payload.pod_namespace)
		.await
	{
		Ok(pod) => pod,
		Err(loom_server_k8s::K8sError::PodNotFound { .. }) => {
			warn!(
				pod_name = %payload.pod_name,
				pod_namespace = %payload.pod_namespace,
				"Pod not found"
			);
			return (
				StatusCode::NOT_FOUND,
				Json(ErrorResponse {
					error: "pod_not_found".to_string(),
					message: format!(
						"Pod '{}' not found in namespace '{}'",
						payload.pod_name, payload.pod_namespace
					),
				}),
			)
				.into_response();
		}
		Err(e) => {
			warn!(error = %e, "Failed to fetch pod");
			return (
				StatusCode::INTERNAL_SERVER_ERROR,
				Json(ErrorResponse {
					error: "pod_fetch_failed".to_string(),
					message: "Failed to fetch pod metadata".to_string(),
				}),
			)
				.into_response();
		}
	};

	let labels = pod.metadata.labels.as_ref().cloned().unwrap_or_default();

	let is_managed = labels
		.get(MANAGED_LABEL)
		.map(|v| v == "true")
		.unwrap_or(false);
	if !is_managed {
		warn!(
			pod_name = %payload.pod_name,
			"Pod not managed by Loom (missing or false loom.dev/managed label)"
		);
		return (
			StatusCode::FORBIDDEN,
			Json(ErrorResponse {
				error: "not_managed".to_string(),
				message: "Pod is not managed by Loom".to_string(),
			}),
		)
			.into_response();
	}

	let weaver_id = match labels.get(WEAVER_ID_LABEL) {
		Some(id) => id.clone(),
		None => {
			warn!(pod_name = %payload.pod_name, "Pod missing weaver-id label");
			return (
				StatusCode::FORBIDDEN,
				Json(ErrorResponse {
					error: "missing_weaver_id".to_string(),
					message: "Pod missing loom.dev/weaver-id label".to_string(),
				}),
			)
				.into_response();
		}
	};

	let org_id = match labels.get(ORG_ID_LABEL) {
		Some(id) => id.clone(),
		None => {
			warn!(pod_name = %payload.pod_name, "Pod missing org-id label");
			return (
				StatusCode::FORBIDDEN,
				Json(ErrorResponse {
					error: "missing_org_id".to_string(),
					message: "Pod missing loom.dev/org-id label".to_string(),
				}),
			)
				.into_response();
		}
	};

	let owner_user_id = match labels.get(OWNER_USER_ID_LABEL) {
		Some(id) => id.clone(),
		None => {
			warn!(pod_name = %payload.pod_name, "Pod missing owner-user-id label");
			return (
				StatusCode::FORBIDDEN,
				Json(ErrorResponse {
					error: "missing_owner_user_id".to_string(),
					message: "Pod missing loom.dev/owner-user-id label".to_string(),
				}),
			)
				.into_response();
		}
	};

	let repo_id = labels.get(REPO_ID_LABEL).cloned();
	let pod_uid = pod.metadata.uid.clone().unwrap_or_default();

	let validated_token = loom_server_secrets::ValidatedSaToken::new(
		payload.pod_name.clone(),
		payload.pod_namespace.clone(),
		token_review_result
			.service_account_name()
			.unwrap_or("unknown")
			.to_string(),
	);

	let svid_request = loom_server_secrets::SvidRequest {
		pod_name: payload.pod_name.clone(),
		pod_namespace: payload.pod_namespace.clone(),
	};

	let pod_metadata = loom_server_secrets::PodMetadata {
		name: payload.pod_name.clone(),
		namespace: payload.pod_namespace.clone(),
		uid: pod_uid,
		weaver_id: Some(weaver_id),
		org_id: Some(org_id),
		repo_id,
		owner_user_id: Some(owner_user_id),
		is_managed: true,
	};

	let svid = match svid_issuer
		.issue_svid(&validated_token, &svid_request, &pod_metadata)
		.await
	{
		Ok(svid) => svid,
		Err(e) => {
			warn!(error = %e, "SVID issuance failed");
			return (
				StatusCode::INTERNAL_SERVER_ERROR,
				Json(ErrorResponse {
					error: "svid_issuance_failed".to_string(),
					message: "Failed to issue SVID".to_string(),
				}),
			)
				.into_response();
		}
	};

	info!(
		spiffe_id = %svid.spiffe_id,
		expires_at = %svid.expires_at,
		"Weaver SVID issued"
	);

	(
		StatusCode::OK,
		Json(SvidResponse {
			token: svid.token,
			token_type: svid.token_type,
			expires_at: svid.expires_at,
			spiffe_id: svid.spiffe_id,
		}),
	)
		.into_response()
}

#[utoipa::path(
    get,
    path = "/internal/weaver-auth/.well-known/jwks.json",
    responses(
        (status = 200, description = "JWKS for SVID verification"),
        (status = 503, description = "SVID issuance not configured", body = ErrorResponse)
    ),
    tag = "weaver-auth"
)]
#[instrument(skip(state))]
pub async fn get_jwks(State(state): State<AppState>) -> impl IntoResponse {
	let svid_issuer = match state.svid_issuer.as_ref() {
		Some(issuer) => issuer,
		None => {
			return (
				StatusCode::SERVICE_UNAVAILABLE,
				Json(ErrorResponse {
					error: "service_unavailable".to_string(),
					message: "SVID issuance not configured".to_string(),
				}),
			)
				.into_response();
		}
	};

	let jwks = svid_issuer.jwks();
	(StatusCode::OK, Json(jwks)).into_response()
}
