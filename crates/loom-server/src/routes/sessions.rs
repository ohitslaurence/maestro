// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights
// reserved. SPDX-License-Identifier: Proprietary

//! Session management HTTP handlers.
//!
//! Implements session endpoints per the auth-abac-system.md specification:
//! - List user's sessions
//! - Revoke a session

use axum::{
	extract::{Path, State},
	http::StatusCode,
	response::IntoResponse,
	Json,
};
use loom_server_audit::{AuditEventType, AuditLogBuilder, UserId as AuditUserId};
use loom_server_auth::SessionId;

pub use loom_server_api::sessions::*;

use crate::{
	api::AppState,
	api_response::id_parse_error,
	auth_middleware::RequireAuth,
	i18n::{resolve_user_locale, t},
	impl_api_error_response,
	validation::parse_uuid,
};

impl_api_error_response!(SessionErrorResponse);

#[utoipa::path(
    get,
    path = "/api/sessions",
    responses(
        (status = 200, description = "List of user's sessions", body = ListSessionsResponse),
        (status = 401, description = "Not authenticated", body = SessionErrorResponse)
    ),
    tag = "sessions"
)]
/// GET /api/sessions - List all sessions for the current user.
///
/// Returns all active sessions including device info, location, and whether
/// each session is the current one making the request.
pub async fn list_sessions(
	State(state): State<AppState>,
	RequireAuth(current_user): RequireAuth,
) -> impl IntoResponse {
	let locale = resolve_user_locale(&current_user, &state.default_locale);
	let current_session_id = current_user.session_id;

	match state
		.session_repo
		.get_sessions_for_user(&current_user.user.id)
		.await
	{
		Ok(sessions) => {
			let session_responses: Vec<SessionResponse> = sessions
				.into_iter()
				.map(|s| SessionResponse {
					id: s.id.to_string(),
					session_type: s.session_type.to_string(),
					created_at: s.created_at,
					last_used_at: s.last_used_at,
					expires_at: s.expires_at,
					ip_address: s.ip_address,
					user_agent: s.user_agent,
					geo_city: s.geo_city,
					geo_country: s.geo_country,
					is_current: current_session_id.as_ref() == Some(&s.id),
				})
				.collect();

			Json(ListSessionsResponse {
				sessions: session_responses,
			})
			.into_response()
		}
		Err(e) => {
			tracing::error!(error = %e, "Failed to list sessions");
			(
				StatusCode::INTERNAL_SERVER_ERROR,
				Json(SessionErrorResponse {
					error: "internal_error".to_string(),
					message: t(locale, "server.api.session.list_failed").to_string(),
				}),
			)
				.into_response()
		}
	}
}

#[utoipa::path(
    delete,
    path = "/api/sessions/{id}",
    params(
        ("id" = String, Path, description = "Session ID to revoke")
    ),
    responses(
        (status = 200, description = "Session revoked", body = SessionSuccessResponse),
        (status = 401, description = "Not authenticated", body = SessionErrorResponse),
        (status = 403, description = "Cannot revoke this session", body = SessionErrorResponse),
        (status = 404, description = "Session not found", body = SessionErrorResponse)
    ),
    tag = "sessions"
)]
/// DELETE /api/sessions/{id} - Revoke a specific session.
///
/// Users can revoke their own sessions. Revoking the current session
/// is equivalent to logging out.
pub async fn revoke_session(
	State(state): State<AppState>,
	RequireAuth(current_user): RequireAuth,
	Path(session_id): Path<String>,
) -> impl IntoResponse {
	let locale = resolve_user_locale(&current_user, &state.default_locale);

	let session_id = match parse_uuid(&session_id, &t(locale, "server.api.session.invalid_id")) {
		Ok(uuid) => SessionId::new(uuid),
		Err(e) => return id_parse_error::<SessionErrorResponse>(e).into_response(),
	};

	// Get all sessions for the user to verify ownership
	let sessions = match state
		.session_repo
		.get_sessions_for_user(&current_user.user.id)
		.await
	{
		Ok(sessions) => sessions,
		Err(e) => {
			tracing::error!(error = %e, "Failed to get sessions for ownership check");
			return (
				StatusCode::INTERNAL_SERVER_ERROR,
				Json(SessionErrorResponse {
					error: "internal_error".to_string(),
					message: t(locale, "server.api.session.ownership_check_failed").to_string(),
				}),
			)
				.into_response();
		}
	};

	// Check if the session belongs to the current user
	if !sessions.iter().any(|s| s.id == session_id) {
		return (
			StatusCode::NOT_FOUND,
			Json(SessionErrorResponse {
				error: "not_found".to_string(),
				message: t(locale, "server.api.session.not_found").to_string(),
			}),
		)
			.into_response();
	}

	// Delete the session
	match state.session_repo.delete_session(&session_id).await {
		Ok(deleted) => {
			if deleted {
				state.audit_service.log(
					AuditLogBuilder::new(AuditEventType::SessionRevoked)
						.actor(AuditUserId::new(current_user.user.id.into_inner()))
						.resource("session", session_id.to_string())
						.details(serde_json::json!({
							"revoked_by": current_user.user.id.to_string(),
						}))
						.build(),
				);

				Json(SessionSuccessResponse {
					message: t(locale, "server.api.session.revoked").to_string(),
				})
				.into_response()
			} else {
				(
					StatusCode::NOT_FOUND,
					Json(SessionErrorResponse {
						error: "not_found".to_string(),
						message: t(locale, "server.api.session.not_found").to_string(),
					}),
				)
					.into_response()
			}
		}
		Err(e) => {
			tracing::error!(error = %e, "Failed to delete session");
			(
				StatusCode::INTERNAL_SERVER_ERROR,
				Json(SessionErrorResponse {
					error: "internal_error".to_string(),
					message: t(locale, "server.api.session.revoke_failed").to_string(),
				}),
			)
				.into_response()
		}
	}
}
