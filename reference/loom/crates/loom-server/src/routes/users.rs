// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights
// reserved. SPDX-License-Identifier: Proprietary

//! User profile HTTP handlers.
//!
//! Implements user endpoints per the auth-abac-system.md specification:
//! - Get user profile
//! - Update own profile
//! - Account deletion and restoration

use axum::{
	extract::{Path, State},
	http::StatusCode,
	response::IntoResponse,
	Json,
};
use chrono::Utc;
use loom_server_audit::{AuditEventType, AuditLogBuilder, UserId as AuditUserId};
use loom_server_auth::{validate_username, Action, ACCOUNT_DELETION_GRACE_DAYS};

pub use loom_server_api::users::*;

use crate::{
	abac_middleware::{build_subject_attrs, check_authorization, user_resource},
	api::AppState,
	auth_middleware::RequireAuth,
	i18n::{resolve_user_locale, t},
	impl_api_error_response, parse_id,
	validation::parse_user_id as shared_parse_user_id,
};

impl_api_error_response!(UserErrorResponse);

#[utoipa::path(
    get,
    path = "/api/users/{id}",
    params(
        ("id" = String, Path, description = "User ID")
    ),
    responses(
        (status = 200, description = "User profile", body = UserProfileResponse),
        (status = 401, description = "Not authenticated", body = UserErrorResponse),
        (status = 403, description = "Access denied", body = UserErrorResponse),
        (status = 404, description = "User not found", body = UserErrorResponse)
    ),
    tag = "users"
)]
/// Get a user's public profile.
///
/// Returns the public profile of a user. Email is only included if the user
/// has `email_visible` enabled or the viewer is the user themselves.
///
/// # Authorization
/// Requires authentication. ABAC check for `Action::Read` on the user resource.
/// All authenticated users can view public profiles.
///
/// # Path Parameters
/// - `id`: User UUID
///
/// # Response
/// Returns user profile with display name, optional email, and avatar URL.
///
/// # Errors
/// - 400: Invalid user ID format
/// - 401: Not authenticated
/// - 403: Access denied (ABAC check failed)
/// - 404: User not found
/// - 500: Internal server error
#[tracing::instrument(skip(state), fields(%user_id))]
pub async fn get_user_profile(
	RequireAuth(current_user): RequireAuth,
	State(state): State<AppState>,
	Path(user_id): Path<String>,
) -> impl IntoResponse {
	let locale = resolve_user_locale(&current_user, &state.default_locale);

	let target_user_id = parse_id!(
		UserErrorResponse,
		shared_parse_user_id(&user_id, &t(locale, "server.api.user.invalid_id"))
	);

	let target_user = match state.user_repo.get_user_by_id(&target_user_id).await {
		Ok(Some(user)) => user,
		Ok(None) => {
			return (
				StatusCode::NOT_FOUND,
				Json(UserErrorResponse {
					error: "not_found".to_string(),
					message: t(locale, "server.api.user.not_found").to_string(),
				}),
			)
				.into_response();
		}
		Err(e) => {
			tracing::error!(error = %e, %target_user_id, "Failed to get user");
			return (
				StatusCode::INTERNAL_SERVER_ERROR,
				Json(UserErrorResponse {
					error: "internal_error".to_string(),
					message: t(locale, "server.api.error.internal").to_string(),
				}),
			)
				.into_response();
		}
	};

	let subject = build_subject_attrs(&current_user, &state.org_repo, &state.team_repo).await;
	let resource = user_resource(target_user_id);

	if let Err(e) = check_authorization(&subject, Action::Read, &resource) {
		return (StatusCode::FORBIDDEN, Json(e)).into_response();
	}

	let is_own_profile = current_user.user.id == target_user.id;
	let email = if is_own_profile || target_user.email_visible {
		target_user.primary_email.clone()
	} else {
		None
	};

	(
		StatusCode::OK,
		Json(UserProfileResponse {
			id: target_user.id.to_string(),
			display_name: target_user.display_name,
			email,
			avatar_url: target_user.avatar_url,
		}),
	)
		.into_response()
}

#[utoipa::path(
    patch,
    path = "/api/users/me",
    request_body = UpdateUserProfileRequest,
    responses(
        (status = 200, description = "Profile updated", body = CurrentUserProfileResponse),
        (status = 400, description = "Invalid request", body = UserErrorResponse),
        (status = 401, description = "Not authenticated", body = UserErrorResponse)
    ),
    tag = "users"
)]
/// Update the current user's profile.
///
/// Users can update their display name, avatar URL, and email visibility setting.
///
/// # Authorization
/// Requires authentication. Users can only update their own profile.
///
/// # Request Body
/// All fields are optional:
/// - `display_name`: New display name (cannot be empty)
/// - `avatar_url`: New avatar URL (empty string clears the avatar)
/// - `email_visible`: Whether email is visible to other users
///
/// # Response
/// Returns the updated user profile.
///
/// # Errors
/// - 400: Display name is empty
/// - 401: Not authenticated
/// - 500: Internal server error
#[tracing::instrument(skip(state, payload))]
pub async fn update_current_user(
	RequireAuth(current_user): RequireAuth,
	State(state): State<AppState>,
	Json(payload): Json<UpdateUserProfileRequest>,
) -> impl IntoResponse {
	let locale = resolve_user_locale(&current_user, &state.default_locale);
	let mut user = current_user.user.clone();
	let user_id = user.id;

	if let Some(display_name) = payload.display_name {
		if display_name.trim().is_empty() {
			return (
				StatusCode::BAD_REQUEST,
				Json(UserErrorResponse {
					error: "bad_request".to_string(),
					message: t(locale, "server.api.user.display_name_empty").to_string(),
				}),
			)
				.into_response();
		}
		user.display_name = display_name;
	}

	if let Some(ref username) = payload.username {
		if let Err(e) = validate_username(username) {
			let msg_key = if e.contains("at least 3") {
				"server.api.user.username_too_short"
			} else if e.contains("at most 39") {
				"server.api.user.username_too_long"
			} else if e.contains("reserved") {
				"server.api.user.username_reserved"
			} else {
				"server.api.user.username_invalid"
			};
			return (
				StatusCode::BAD_REQUEST,
				Json(UserErrorResponse {
					error: "bad_request".to_string(),
					message: t(locale, msg_key).to_string(),
				}),
			)
				.into_response();
		}

		let is_same_username = user
			.username
			.as_ref()
			.map(|u| u.eq_ignore_ascii_case(username))
			.unwrap_or(false);

		if !is_same_username {
			match state.user_repo.is_username_available(username).await {
				Ok(true) => {}
				Ok(false) => {
					return (
						StatusCode::CONFLICT,
						Json(UserErrorResponse {
							error: "conflict".to_string(),
							message: t(locale, "server.api.user.username_taken").to_string(),
						}),
					)
						.into_response();
				}
				Err(e) => {
					tracing::error!(error = %e, %user_id, "Failed to check username availability");
					return (
						StatusCode::INTERNAL_SERVER_ERROR,
						Json(UserErrorResponse {
							error: "internal_error".to_string(),
							message: t(locale, "server.api.error.internal").to_string(),
						}),
					)
						.into_response();
				}
			}
		}
		user.username = Some(username.clone());
	}

	if let Some(avatar_url) = payload.avatar_url {
		user.avatar_url = if avatar_url.is_empty() {
			None
		} else {
			Some(avatar_url)
		};
	}

	if let Some(email_visible) = payload.email_visible {
		user.email_visible = email_visible;
	}

	if let Some(ref new_locale) = payload.locale {
		if !loom_common_i18n::is_supported(new_locale) {
			return (
				StatusCode::BAD_REQUEST,
				Json(UserErrorResponse {
					error: "bad_request".to_string(),
					message: t(locale, "server.api.user.invalid_locale").to_string(),
				}),
			)
				.into_response();
		}
		user.locale = Some(new_locale.clone());
	}

	user.updated_at = Utc::now();

	if let Err(e) = state.user_repo.update_user(&user).await {
		tracing::error!(error = %e, %user_id, "Failed to update user");
		return (
			StatusCode::INTERNAL_SERVER_ERROR,
			Json(UserErrorResponse {
				error: "internal_error".to_string(),
				message: t(locale, "server.api.error.internal").to_string(),
			}),
		)
			.into_response();
	}

	tracing::info!(%user_id, "User profile updated");

	state.audit_service.log(
		AuditLogBuilder::new(AuditEventType::OrgUpdated)
			.actor(AuditUserId::new(user_id.into_inner()))
			.resource("user", user_id.to_string())
			.details(serde_json::json!({
				"action": "profile_updated",
				"display_name": user.display_name,
				"username": user.username,
				"email_visible": user.email_visible,
			}))
			.build(),
	);

	(
		StatusCode::OK,
		Json(CurrentUserProfileResponse {
			id: user.id.to_string(),
			display_name: user.display_name,
			username: user.username,
			primary_email: user.primary_email,
			avatar_url: user.avatar_url,
			email_visible: user.email_visible,
			locale: user.locale,
			created_at: user.created_at,
			updated_at: user.updated_at,
		}),
	)
		.into_response()
}

#[utoipa::path(
    post,
    path = "/api/users/me/delete",
    responses(
        (status = 200, description = "Deletion scheduled", body = AccountDeletionResponse),
        (status = 401, description = "Not authenticated", body = UserErrorResponse),
        (status = 409, description = "Deletion already scheduled", body = UserErrorResponse)
    ),
    tag = "users"
)]
/// Request account deletion.
///
/// Schedules the account for deletion after a grace period (90 days).
/// During the grace period, the user can restore their account.
/// All sessions are invalidated, and the user is logged out.
///
/// # Authorization
/// Requires authentication. Users can only delete their own account.
///
/// # Response
/// Returns deletion confirmation with scheduled date and grace period.
///
/// # Side Effects
/// - All user sessions are invalidated
/// - A confirmation email is sent (if SMTP is configured)
///
/// # Errors
/// - 401: Not authenticated
/// - 409: Deletion already scheduled
/// - 500: Internal server error
#[tracing::instrument(skip(state))]
pub async fn request_account_deletion(
	RequireAuth(current_user): RequireAuth,
	State(state): State<AppState>,
) -> impl IntoResponse {
	let locale = resolve_user_locale(&current_user, &state.default_locale);
	let user_id = current_user.user.id;

	if current_user.user.deleted_at.is_some() {
		return (
			StatusCode::CONFLICT,
			Json(UserErrorResponse {
				error: "conflict".to_string(),
				message: t(locale, "server.api.user.deletion_already_scheduled").to_string(),
			}),
		)
			.into_response();
	}

	if let Err(e) = state
		.user_repo
		.soft_delete_user(&current_user.user.id)
		.await
	{
		tracing::error!(error = %e, %user_id, "Failed to schedule account deletion");
		return (
			StatusCode::INTERNAL_SERVER_ERROR,
			Json(UserErrorResponse {
				error: "internal_error".to_string(),
				message: t(locale, "server.api.user.deletion_failed").to_string(),
			}),
		)
			.into_response();
	}

	if let Err(e) = state
		.session_repo
		.delete_all_sessions_for_user(&current_user.user.id)
		.await
	{
		tracing::warn!(error = %e, %user_id, "Failed to invalidate sessions");
	}

	let deletion_scheduled_at = Utc::now();

	if let Some(email_service) = &state.email_service {
		if let Some(email) = &current_user.user.primary_email {
			let request = loom_server_email::EmailRequest::DeletionScheduled {
				grace_days: ACCOUNT_DELETION_GRACE_DAYS,
			};
			if let Err(e) = email_service
				.send(email, request, current_user.user.locale.as_deref())
				.await
			{
				tracing::warn!(error = %e, %user_id, "Failed to send deletion confirmation email");
			}
		}
	}

	tracing::info!(%user_id, "Account deletion scheduled");

	state.audit_service.log(
		AuditLogBuilder::new(AuditEventType::OrgDeleted)
			.actor(AuditUserId::new(user_id.into_inner()))
			.resource("user", user_id.to_string())
			.details(serde_json::json!({
				"action": "account_deletion_scheduled",
				"grace_period_days": ACCOUNT_DELETION_GRACE_DAYS,
			}))
			.build(),
	);

	(
		StatusCode::OK,
		Json(AccountDeletionResponse {
			message: t(locale, "server.api.user.deletion_scheduled").to_string(),
			deletion_scheduled_at,
			grace_period_days: ACCOUNT_DELETION_GRACE_DAYS as i32,
		}),
	)
		.into_response()
}

#[utoipa::path(
    post,
    path = "/api/users/me/restore",
    responses(
        (status = 200, description = "Account restored", body = UserSuccessResponse),
        (status = 401, description = "Not authenticated", body = UserErrorResponse),
        (status = 404, description = "No pending deletion", body = UserErrorResponse),
        (status = 410, description = "Grace period expired", body = UserErrorResponse)
    ),
    tag = "users"
)]
/// Restore a deleted account.
///
/// Cancels a pending account deletion if within the grace period.
/// The user must authenticate (e.g., via magic link) to restore their account.
///
/// # Authorization
/// Requires authentication. Users can only restore their own account.
///
/// # Response
/// Returns a success message.
///
/// # Errors
/// - 401: Not authenticated
/// - 404: No pending deletion for this account
/// - 410: Grace period has expired, account cannot be restored
/// - 500: Internal server error
#[tracing::instrument(skip(state))]
pub async fn restore_account(
	RequireAuth(current_user): RequireAuth,
	State(state): State<AppState>,
) -> impl IntoResponse {
	let locale = resolve_user_locale(&current_user, &state.default_locale);
	let user_id = current_user.user.id;

	let deleted_at = match current_user.user.deleted_at {
		Some(dt) => dt,
		None => {
			return (
				StatusCode::NOT_FOUND,
				Json(UserErrorResponse {
					error: "not_found".to_string(),
					message: t(locale, "server.api.user.no_pending_deletion").to_string(),
				}),
			)
				.into_response();
		}
	};

	let hard_delete_at = deleted_at + chrono::Duration::days(ACCOUNT_DELETION_GRACE_DAYS);
	if Utc::now() >= hard_delete_at {
		return (
			StatusCode::GONE,
			Json(UserErrorResponse {
				error: "gone".to_string(),
				message: t(locale, "server.api.user.grace_period_expired").to_string(),
			}),
		)
			.into_response();
	}

	if let Err(e) = state.user_repo.restore_user(&current_user.user.id).await {
		tracing::error!(error = %e, %user_id, "Failed to restore account");
		return (
			StatusCode::INTERNAL_SERVER_ERROR,
			Json(UserErrorResponse {
				error: "internal_error".to_string(),
				message: t(locale, "server.api.error.internal").to_string(),
			}),
		)
			.into_response();
	}

	tracing::info!(%user_id, "Account restored");

	state.audit_service.log(
		AuditLogBuilder::new(AuditEventType::OrgRestored)
			.actor(AuditUserId::new(user_id.into_inner()))
			.resource("user", user_id.to_string())
			.details(serde_json::json!({
				"action": "account_restored",
			}))
			.build(),
	);

	(
		StatusCode::OK,
		Json(UserSuccessResponse {
			message: t(locale, "server.api.user.account_restored").to_string(),
		}),
	)
		.into_response()
}

#[utoipa::path(
    get,
    path = "/api/users/me/identities",
    responses(
        (status = 200, description = "List of linked identities", body = ListIdentitiesResponse),
        (status = 401, description = "Not authenticated", body = UserErrorResponse)
    ),
    tag = "users"
)]
/// List linked identities for current user.
///
/// Returns all OAuth providers and authentication methods linked to this account.
///
/// # Authorization
/// Requires authentication. Users can only view their own identities.
///
/// # Response
/// Returns a list of linked identities with provider, email, and verification status.
///
/// # Errors
/// - 401: Not authenticated
/// - 500: Internal server error
#[tracing::instrument(skip(state))]
pub async fn list_identities(
	RequireAuth(current_user): RequireAuth,
	State(state): State<AppState>,
) -> impl IntoResponse {
	let locale = resolve_user_locale(&current_user, &state.default_locale);
	let user_id = current_user.user.id;

	let identities = match state
		.user_repo
		.get_identities_for_user(&current_user.user.id)
		.await
	{
		Ok(identities) => identities,
		Err(e) => {
			tracing::error!(error = %e, %user_id, "Failed to get identities");
			return (
				StatusCode::INTERNAL_SERVER_ERROR,
				Json(UserErrorResponse {
					error: "internal_error".to_string(),
					message: t(locale, "server.api.error.internal").to_string(),
				}),
			)
				.into_response();
		}
	};

	let response = ListIdentitiesResponse {
		identities: identities
			.into_iter()
			.map(|i| IdentityResponse {
				id: i.id.to_string(),
				provider: i.provider.to_string(),
				email: i.email,
				email_verified: i.email_verified,
				created_at: i.created_at,
			})
			.collect(),
	};

	(StatusCode::OK, Json(response)).into_response()
}

#[utoipa::path(
    delete,
    path = "/api/users/me/identities/{id}",
    params(
        ("id" = String, Path, description = "Identity ID to unlink")
    ),
    responses(
        (status = 200, description = "Identity unlinked", body = UserSuccessResponse),
        (status = 401, description = "Not authenticated", body = UserErrorResponse),
        (status = 404, description = "Identity not found", body = UserErrorResponse),
        (status = 409, description = "Cannot unlink last identity", body = UserErrorResponse)
    ),
    tag = "users"
)]
/// Unlink an identity (OAuth provider).
///
/// Removes an OAuth provider or authentication method from the account.
/// Cannot unlink the last remaining identity.
///
/// # Authorization
/// Requires authentication. Users can only unlink their own identities.
///
/// # Path Parameters
/// - `id`: Identity UUID to unlink
///
/// # Response
/// Returns a success message.
///
/// # Errors
/// - 401: Not authenticated
/// - 404: Identity not found
/// - 409: Cannot unlink last identity
/// - 500: Internal server error
#[tracing::instrument(skip(state), fields(%identity_id))]
pub async fn unlink_identity(
	RequireAuth(current_user): RequireAuth,
	State(state): State<AppState>,
	Path(identity_id): Path<String>,
) -> impl IntoResponse {
	let locale = resolve_user_locale(&current_user, &state.default_locale);
	let user_id = current_user.user.id;

	let identities = match state
		.user_repo
		.get_identities_for_user(&current_user.user.id)
		.await
	{
		Ok(identities) => identities,
		Err(e) => {
			tracing::error!(error = %e, %user_id, "Failed to get identities");
			return (
				StatusCode::INTERNAL_SERVER_ERROR,
				Json(UserErrorResponse {
					error: "internal_error".to_string(),
					message: t(locale, "server.api.error.internal").to_string(),
				}),
			)
				.into_response();
		}
	};

	let target_identity = identities.iter().find(|i| i.id.to_string() == identity_id);

	let target_identity = match target_identity {
		Some(i) => i,
		None => {
			return (
				StatusCode::NOT_FOUND,
				Json(UserErrorResponse {
					error: "not_found".to_string(),
					message: t(locale, "server.api.user.identity_not_found").to_string(),
				}),
			)
				.into_response();
		}
	};

	if identities.len() <= 1 {
		return (
			StatusCode::CONFLICT,
			Json(UserErrorResponse {
				error: "conflict".to_string(),
				message: t(locale, "server.api.user.cannot_unlink_last_identity").to_string(),
			}),
		)
			.into_response();
	}

	if let Err(e) = state.user_repo.delete_identity(&target_identity.id).await {
		tracing::error!(error = %e, %user_id, %identity_id, "Failed to delete identity");
		return (
			StatusCode::INTERNAL_SERVER_ERROR,
			Json(UserErrorResponse {
				error: "internal_error".to_string(),
				message: t(locale, "server.api.error.internal").to_string(),
			}),
		)
			.into_response();
	}

	tracing::info!(%user_id, %identity_id, "Identity unlinked");

	state.audit_service.log(
		AuditLogBuilder::new(AuditEventType::MemberRemoved)
			.actor(AuditUserId::new(user_id.into_inner()))
			.resource("identity", identity_id.clone())
			.details(serde_json::json!({
				"action": "identity_unlinked",
				"provider": target_identity.provider.to_string(),
			}))
			.build(),
	);

	(
		StatusCode::OK,
		Json(UserSuccessResponse {
			message: t(locale, "server.api.user.identity_unlinked").to_string(),
		}),
	)
		.into_response()
}
