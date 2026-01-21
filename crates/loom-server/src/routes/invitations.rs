// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights
// reserved. SPDX-License-Identifier: Proprietary

//! Invitation HTTP handlers.
//!
//! Implements invitation endpoints:
//! - List pending invitations for an org
//! - Create invitation
//! - Cancel invitation
//! - Accept invitation
//! - List join requests
//! - Create join request
//! - Approve/reject join requests
//!
//! # Security
//!
//! - Invitation tokens are stored as SHA-256 hashes
//! - Tokens expire after a configurable period
//! - All invitation operations are logged
//!
//! # Authorization Matrix
//!
//! | Endpoint                | Required Permission           |
//! |------------------------|------------------------------|
//! | `list_invitations`     | `ManageOrg`                  |
//! | `create_invitation`    | `ManageOrg`                  |
//! | `cancel_invitation`    | `ManageOrg`                  |
//! | `accept_invitation`    | authenticated (any)          |
//! | `get_invitation`       | public (with token)          |
//! | `list_join_requests`   | `ManageOrg`                  |
//! | `create_join_request`  | authenticated (any)          |
//! | `approve_join_request` | `ManageOrg`                  |
//! | `reject_join_request`  | `ManageOrg`                  |

use axum::{
	extract::{Path, State},
	http::StatusCode,
	response::IntoResponse,
	Json,
};
use chrono::Utc;
use loom_server_audit::{AuditEventType, AuditLogBuilder, UserId as AuditUserId};
use loom_server_auth::{hash_token, org::OrgVisibility, Action, OrgRole, Visibility};
use loom_server_email::EmailRequest;
use uuid::Uuid;

pub use loom_server_api::invitations::*;

use crate::{
	abac_middleware::{build_subject_attrs, org_resource},
	api::AppState,
	auth_middleware::RequireAuth,
	authorize,
	i18n::{resolve_user_locale, t},
	impl_api_error_response, parse_id, parse_role,
	validation::{parse_org_id as shared_parse_org_id, parse_org_role},
};

impl_api_error_response!(InvitationErrorResponse);

fn generate_invitation_token() -> String {
	format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple())
}

fn org_visibility_to_abac(v: OrgVisibility) -> Visibility {
	match v {
		OrgVisibility::Public => Visibility::Public,
		OrgVisibility::Unlisted => Visibility::Organization,
		OrgVisibility::Private => Visibility::Private,
	}
}

/// List pending invitations for an organization.
///
/// # Authorization
///
/// Requires `ManageOrg` permission on the organization.
///
/// # Request
///
/// Path parameters:
/// - `org_id`: Organization UUID
///
/// # Response
///
/// Returns [`ListInvitationsResponse`] with all pending invitations.
///
/// # Errors
///
/// - `400 Bad Request`: Invalid organization ID format
/// - `401 Unauthorized`: Missing or invalid authentication
/// - `403 Forbidden`: Caller lacks `ManageOrg` permission
/// - `404 Not Found`: Organization does not exist
/// - `500 Internal Server Error`: Database error
#[utoipa::path(
    get,
    path = "/api/orgs/{org_id}/invitations",
    params(
        ("org_id" = String, Path, description = "Organization ID")
    ),
    responses(
        (status = 200, description = "List of pending invitations", body = ListInvitationsResponse),
        (status = 401, description = "Not authenticated", body = InvitationErrorResponse),
        (status = 403, description = "Not authorized", body = InvitationErrorResponse),
        (status = 404, description = "Organization not found", body = InvitationErrorResponse)
    ),
    tag = "invitations"
)]
#[tracing::instrument(
	skip(state),
	fields(
		actor_id = %current_user.user.id,
		org_id = %org_id
	)
)]
pub async fn list_invitations(
	RequireAuth(current_user): RequireAuth,
	State(state): State<AppState>,
	Path(org_id): Path<String>,
) -> impl IntoResponse {
	let locale = resolve_user_locale(&current_user, &state.default_locale);

	let org_id = parse_id!(
		InvitationErrorResponse,
		shared_parse_org_id(&org_id, &t(locale, "server.api.org.invalid_id"))
	);

	let org = match state.org_repo.get_org_by_id(&org_id).await {
		Ok(Some(org)) => org,
		Ok(None) => {
			return (
				StatusCode::NOT_FOUND,
				Json(InvitationErrorResponse {
					error: "not_found".to_string(),
					message: t(locale, "server.api.org.not_found").to_string(),
				}),
			)
				.into_response();
		}
		Err(e) => {
			tracing::error!(
				error = %e,
				actor_id = %current_user.user.id,
				org_id = %org_id,
				"Failed to get organization"
			);
			return (
				StatusCode::INTERNAL_SERVER_ERROR,
				Json(InvitationErrorResponse {
					error: "internal_error".to_string(),
					message: t(locale, "server.api.invitation.list_failed").to_string(),
				}),
			)
				.into_response();
		}
	};

	let subject = build_subject_attrs(&current_user, &state.org_repo, &state.team_repo).await;
	let resource = org_resource(org.id, org_visibility_to_abac(org.visibility));

	if let Err(e) = authorize!(&subject, Action::ManageOrg, &resource) {
		tracing::warn!(
			actor_id = %current_user.user.id,
			org_id = %org_id,
			"Unauthorized invitation list attempt"
		);
		return e.into_response();
	}

	let invitations = match state.org_repo.list_pending_invitations(&org_id).await {
		Ok(invs) => invs,
		Err(e) => {
			tracing::error!(
				error = %e,
				actor_id = %current_user.user.id,
				org_id = %org_id,
				"Failed to list invitations"
			);
			return (
				StatusCode::INTERNAL_SERVER_ERROR,
				Json(InvitationErrorResponse {
					error: "internal_error".to_string(),
					message: t(locale, "server.api.invitation.list_failed").to_string(),
				}),
			)
				.into_response();
		}
	};

	let mut responses = Vec::with_capacity(invitations.len());
	for inv in invitations {
		let invited_by_name = match state.user_repo.get_user_by_id(&inv.invited_by).await {
			Ok(Some(user)) => user.display_name,
			_ => "Unknown".to_string(),
		};
		let is_expired = inv.is_expired();

		responses.push(InvitationResponse {
			id: inv.id.to_string(),
			org_id: inv.org_id.to_string(),
			org_name: org.name.clone(),
			email: inv.email,
			role: inv.role.to_string(),
			invited_by: inv.invited_by.to_string(),
			invited_by_name,
			created_at: inv.created_at,
			expires_at: inv.expires_at,
			is_expired,
		});
	}

	tracing::info!(
		actor_id = %current_user.user.id,
		org_id = %org_id,
		invitation_count = responses.len(),
		"Listed invitations"
	);

	(
		StatusCode::OK,
		Json(ListInvitationsResponse {
			invitations: responses,
		}),
	)
		.into_response()
}

/// Create an invitation to join an organization.
///
/// # Authorization
///
/// Requires `ManageOrg` permission on the organization.
///
/// # Request
///
/// Path parameters:
/// - `org_id`: Organization UUID
///
/// Body ([`CreateInvitationRequest`]):
/// - `email`: Email address to invite
/// - `role` (optional): Role to assign (default: "member")
///
/// # Response
///
/// Returns [`CreateInvitationResponse`] with invitation details.
///
/// # Errors
///
/// - `400 Bad Request`: Invalid organization ID or role
/// - `401 Unauthorized`: Missing or invalid authentication
/// - `403 Forbidden`: Caller lacks `ManageOrg` permission
/// - `404 Not Found`: Organization does not exist
/// - `500 Internal Server Error`: Database error
///
/// # Security
///
/// - Invitation token is sent via email, not returned in response
/// - Token is stored as a hash
#[utoipa::path(
    post,
    path = "/api/orgs/{org_id}/invitations",
    params(
        ("org_id" = String, Path, description = "Organization ID")
    ),
    request_body = CreateInvitationRequest,
    responses(
        (status = 201, description = "Invitation created", body = CreateInvitationResponse),
        (status = 400, description = "Invalid request", body = InvitationErrorResponse),
        (status = 401, description = "Not authenticated", body = InvitationErrorResponse),
        (status = 403, description = "Not authorized", body = InvitationErrorResponse),
        (status = 404, description = "Organization not found", body = InvitationErrorResponse)
    ),
    tag = "invitations"
)]
#[tracing::instrument(
	skip(state, payload),
	fields(
		actor_id = %current_user.user.id,
		org_id = %org_id
	)
)]
pub async fn create_invitation(
	RequireAuth(current_user): RequireAuth,
	State(state): State<AppState>,
	Path(org_id): Path<String>,
	Json(payload): Json<CreateInvitationRequest>,
) -> impl IntoResponse {
	let locale = resolve_user_locale(&current_user, &state.default_locale);

	let org_id = parse_id!(
		InvitationErrorResponse,
		shared_parse_org_id(&org_id, &t(locale, "server.api.org.invalid_id"))
	);

	let org = match state.org_repo.get_org_by_id(&org_id).await {
		Ok(Some(org)) => org,
		Ok(None) => {
			return (
				StatusCode::NOT_FOUND,
				Json(InvitationErrorResponse {
					error: "not_found".to_string(),
					message: t(locale, "server.api.org.not_found").to_string(),
				}),
			)
				.into_response();
		}
		Err(e) => {
			tracing::error!(
				error = %e,
				actor_id = %current_user.user.id,
				org_id = %org_id,
				"Failed to get organization"
			);
			return (
				StatusCode::INTERNAL_SERVER_ERROR,
				Json(InvitationErrorResponse {
					error: "internal_error".to_string(),
					message: t(locale, "server.api.error.internal").to_string(),
				}),
			)
				.into_response();
		}
	};

	let subject = build_subject_attrs(&current_user, &state.org_repo, &state.team_repo).await;
	let resource = org_resource(org.id, org_visibility_to_abac(org.visibility));

	if let Err(e) = authorize!(&subject, Action::ManageOrg, &resource) {
		tracing::warn!(
			actor_id = %current_user.user.id,
			org_id = %org_id,
			"Unauthorized invitation creation attempt"
		);
		return e.into_response();
	}

	let role = match payload.role.as_deref() {
		Some(r) => parse_role!(
			InvitationErrorResponse,
			parse_org_role(r, &t(locale, "server.api.org.invalid_role"))
		),
		None => OrgRole::Member,
	};

	let token = generate_invitation_token();
	let token_hash = hash_token(&token);

	let invitation_id = match state
		.org_repo
		.create_invitation(
			&org_id,
			&payload.email,
			role,
			&current_user.user.id,
			&token_hash,
		)
		.await
	{
		Ok(id) => id,
		Err(e) => {
			tracing::error!(
				error = %e,
				actor_id = %current_user.user.id,
				org_id = %org_id,
				"Failed to create invitation"
			);
			return (
				StatusCode::INTERNAL_SERVER_ERROR,
				Json(InvitationErrorResponse {
					error: "internal_error".to_string(),
					message: t(locale, "server.api.error.internal").to_string(),
				}),
			)
				.into_response();
		}
	};

	if let Some(email_service) = &state.email_service {
		let request = EmailRequest::OrgInvitation {
			org_name: org.name.clone(),
			inviter_name: current_user.user.display_name.clone(),
			token: token.clone(),
		};
		if let Err(e) = email_service
			.send(&payload.email, request, current_user.user.locale.as_deref())
			.await
		{
			tracing::warn!(error = %e, "Failed to send invitation email");
		}
	}

	let expires_at =
		Utc::now() + chrono::Duration::days(loom_server_auth::OrgInvitation::EXPIRY_DAYS);

	state.audit_service.log(
		AuditLogBuilder::new(AuditEventType::MemberAdded)
			.actor(AuditUserId::new(current_user.user.id.into_inner()))
			.resource("invitation", invitation_id.clone())
			.details(serde_json::json!({
				"action": "invitation_created",
				"org_id": org_id.to_string(),
				"email": &payload.email,
				"role": role.to_string(),
			}))
			.build(),
	);

	tracing::info!(
		actor_id = %current_user.user.id,
		org_id = %org_id,
		invitation_id = %invitation_id,
		role = %role,
		"Invitation created"
	);

	(
		StatusCode::CREATED,
		Json(CreateInvitationResponse {
			id: invitation_id,
			email: payload.email,
			role: role.to_string(),
			expires_at,
		}),
	)
		.into_response()
}

/// Cancel a pending invitation.
///
/// # Authorization
///
/// Requires `ManageOrg` permission on the organization.
///
/// # Request
///
/// Path parameters:
/// - `org_id`: Organization UUID
/// - `id`: Invitation ID
///
/// # Response
///
/// Returns [`InvitationSuccessResponse`] confirming cancellation.
///
/// # Errors
///
/// - `400 Bad Request`: Invalid organization ID format
/// - `401 Unauthorized`: Missing or invalid authentication
/// - `403 Forbidden`: Caller lacks `ManageOrg` permission
/// - `404 Not Found`: Organization or invitation not found
/// - `500 Internal Server Error`: Database error
#[utoipa::path(
    delete,
    path = "/api/orgs/{org_id}/invitations/{id}",
    params(
        ("org_id" = String, Path, description = "Organization ID"),
        ("id" = String, Path, description = "Invitation ID")
    ),
    responses(
        (status = 200, description = "Invitation cancelled", body = InvitationSuccessResponse),
        (status = 401, description = "Not authenticated", body = InvitationErrorResponse),
        (status = 403, description = "Not authorized", body = InvitationErrorResponse),
        (status = 404, description = "Invitation not found", body = InvitationErrorResponse)
    ),
    tag = "invitations"
)]
#[tracing::instrument(
	skip(state),
	fields(
		actor_id = %current_user.user.id,
		org_id = %org_id,
		invitation_id = %invitation_id
	)
)]
pub async fn cancel_invitation(
	RequireAuth(current_user): RequireAuth,
	State(state): State<AppState>,
	Path((org_id, invitation_id)): Path<(String, String)>,
) -> impl IntoResponse {
	let locale = resolve_user_locale(&current_user, &state.default_locale);

	let org_id = parse_id!(
		InvitationErrorResponse,
		shared_parse_org_id(&org_id, &t(locale, "server.api.org.invalid_id"))
	);

	let org = match state.org_repo.get_org_by_id(&org_id).await {
		Ok(Some(org)) => org,
		Ok(None) => {
			return (
				StatusCode::NOT_FOUND,
				Json(InvitationErrorResponse {
					error: "not_found".to_string(),
					message: t(locale, "server.api.org.not_found").to_string(),
				}),
			)
				.into_response();
		}
		Err(e) => {
			tracing::error!(
				error = %e,
				actor_id = %current_user.user.id,
				org_id = %org_id,
				"Failed to get organization"
			);
			return (
				StatusCode::INTERNAL_SERVER_ERROR,
				Json(InvitationErrorResponse {
					error: "internal_error".to_string(),
					message: t(locale, "server.api.error.internal").to_string(),
				}),
			)
				.into_response();
		}
	};

	let subject = build_subject_attrs(&current_user, &state.org_repo, &state.team_repo).await;
	let resource = org_resource(org.id, org_visibility_to_abac(org.visibility));

	if let Err(e) = authorize!(&subject, Action::ManageOrg, &resource) {
		tracing::warn!(
			actor_id = %current_user.user.id,
			org_id = %org_id,
			invitation_id = %invitation_id,
			"Unauthorized invitation cancellation attempt"
		);
		return e.into_response();
	}

	let invitation = match state.org_repo.get_invitation_by_id(&invitation_id).await {
		Ok(Some(inv)) => inv,
		Ok(None) => {
			return (
				StatusCode::NOT_FOUND,
				Json(InvitationErrorResponse {
					error: "not_found".to_string(),
					message: t(locale, "server.api.invitation.not_found").to_string(),
				}),
			)
				.into_response();
		}
		Err(e) => {
			tracing::error!(
				error = %e,
				actor_id = %current_user.user.id,
				invitation_id = %invitation_id,
				"Failed to get invitation"
			);
			return (
				StatusCode::INTERNAL_SERVER_ERROR,
				Json(InvitationErrorResponse {
					error: "internal_error".to_string(),
					message: t(locale, "server.api.error.internal").to_string(),
				}),
			)
				.into_response();
		}
	};

	if invitation.org_id != org_id {
		return (
			StatusCode::NOT_FOUND,
			Json(InvitationErrorResponse {
				error: "not_found".to_string(),
				message: t(locale, "server.api.invitation.not_found").to_string(),
			}),
		)
			.into_response();
	}

	if let Err(e) = state.org_repo.delete_invitation(&invitation_id).await {
		tracing::error!(
			error = %e,
			actor_id = %current_user.user.id,
			invitation_id = %invitation_id,
			"Failed to delete invitation"
		);
		return (
			StatusCode::INTERNAL_SERVER_ERROR,
			Json(InvitationErrorResponse {
				error: "internal_error".to_string(),
				message: t(locale, "server.api.error.internal").to_string(),
			}),
		)
			.into_response();
	}

	tracing::info!(
		actor_id = %current_user.user.id,
		org_id = %org_id,
		invitation_id = %invitation_id,
		"Invitation cancelled"
	);

	(
		StatusCode::OK,
		Json(InvitationSuccessResponse {
			message: t(locale, "server.api.invitation.cancelled").to_string(),
		}),
	)
		.into_response()
}

/// Accept an invitation to join an organization.
///
/// # Authorization
///
/// Requires authentication. Any authenticated user can accept an invitation
/// sent to their email.
///
/// # Request
///
/// Body ([`AcceptInvitationRequest`]):
/// - `token`: The invitation token from the email
///
/// # Response
///
/// Returns [`AcceptInvitationResponse`] with organization details.
///
/// # Errors
///
/// - `400 Bad Request`: Invalid or expired invitation
/// - `401 Unauthorized`: Missing or invalid authentication
/// - `404 Not Found`: Invitation not found or expired
/// - `409 Conflict`: User is already a member
/// - `500 Internal Server Error`: Database error
///
/// # Security
///
/// - Token is validated by comparing hashes
/// - Token can only be used once
#[utoipa::path(
    post,
    path = "/api/invitations/accept",
    request_body = AcceptInvitationRequest,
    responses(
        (status = 200, description = "Invitation accepted", body = AcceptInvitationResponse),
        (status = 400, description = "Invalid invitation", body = InvitationErrorResponse),
        (status = 401, description = "Not authenticated", body = InvitationErrorResponse),
        (status = 404, description = "Invitation not found or expired", body = InvitationErrorResponse),
        (status = 409, description = "Already a member", body = InvitationErrorResponse)
    ),
    tag = "invitations"
)]
#[tracing::instrument(
	skip(state, payload),
	fields(actor_id = %current_user.user.id)
)]
pub async fn accept_invitation(
	RequireAuth(current_user): RequireAuth,
	State(state): State<AppState>,
	Json(payload): Json<AcceptInvitationRequest>,
) -> impl IntoResponse {
	let locale = resolve_user_locale(&current_user, &state.default_locale);

	let token_hash = hash_token(&payload.token);

	let invitation = match state
		.org_repo
		.get_invitation_by_token_hash(&token_hash)
		.await
	{
		Ok(Some(inv)) => inv,
		Ok(None) => {
			return (
				StatusCode::NOT_FOUND,
				Json(InvitationErrorResponse {
					error: "not_found".to_string(),
					message: t(locale, "server.api.invitation.invalid_token").to_string(),
				}),
			)
				.into_response();
		}
		Err(e) => {
			tracing::error!(error = %e, actor_id = %current_user.user.id, "Failed to get invitation");
			return (
				StatusCode::INTERNAL_SERVER_ERROR,
				Json(InvitationErrorResponse {
					error: "internal_error".to_string(),
					message: t(locale, "server.api.error.internal").to_string(),
				}),
			)
				.into_response();
		}
	};

	if !invitation.is_valid() {
		return (
			StatusCode::BAD_REQUEST,
			Json(InvitationErrorResponse {
				error: "invalid_invitation".to_string(),
				message: if invitation.is_expired() {
					t(locale, "server.api.invitation.expired").to_string()
				} else {
					t(locale, "server.api.invitation.already_accepted").to_string()
				},
			}),
		)
			.into_response();
	}

	if let Ok(Some(_)) = state
		.org_repo
		.get_membership(&invitation.org_id, &current_user.user.id)
		.await
	{
		return (
			StatusCode::CONFLICT,
			Json(InvitationErrorResponse {
				error: "already_member".to_string(),
				message: t(locale, "server.api.join_request.already_member").to_string(),
			}),
		)
			.into_response();
	}

	let org = match state.org_repo.get_org_by_id(&invitation.org_id).await {
		Ok(Some(org)) => org,
		Ok(None) => {
			return (
				StatusCode::NOT_FOUND,
				Json(InvitationErrorResponse {
					error: "not_found".to_string(),
					message: t(locale, "server.api.org.not_found").to_string(),
				}),
			)
				.into_response();
		}
		Err(e) => {
			tracing::error!(
				error = %e,
				actor_id = %current_user.user.id,
				org_id = %invitation.org_id,
				"Failed to get organization"
			);
			return (
				StatusCode::INTERNAL_SERVER_ERROR,
				Json(InvitationErrorResponse {
					error: "internal_error".to_string(),
					message: t(locale, "server.api.error.internal").to_string(),
				}),
			)
				.into_response();
		}
	};

	if let Err(e) = state
		.org_repo
		.add_member(&invitation.org_id, &current_user.user.id, invitation.role)
		.await
	{
		tracing::error!(
			error = %e,
			actor_id = %current_user.user.id,
			org_id = %invitation.org_id,
			"Failed to add member"
		);
		return (
			StatusCode::INTERNAL_SERVER_ERROR,
			Json(InvitationErrorResponse {
				error: "internal_error".to_string(),
				message: t(locale, "server.api.error.internal").to_string(),
			}),
		)
			.into_response();
	}

	if let Err(e) = state
		.org_repo
		.accept_invitation(&invitation.id.to_string())
		.await
	{
		tracing::error!(
			error = %e,
			actor_id = %current_user.user.id,
			invitation_id = %invitation.id,
			"Failed to mark invitation as accepted"
		);
	}

	state.audit_service.log(
		AuditLogBuilder::new(AuditEventType::MemberAdded)
			.actor(AuditUserId::new(current_user.user.id.into_inner()))
			.resource("org", org.id.to_string())
			.details(serde_json::json!({
				"action": "invitation_accepted",
				"invitation_id": invitation.id.to_string(),
				"role": invitation.role.to_string(),
			}))
			.build(),
	);

	tracing::info!(
		actor_id = %current_user.user.id,
		org_id = %org.id,
		invitation_id = %invitation.id,
		role = %invitation.role,
		"Invitation accepted"
	);

	(
		StatusCode::OK,
		Json(AcceptInvitationResponse {
			org_id: org.id.to_string(),
			org_name: org.name,
			role: invitation.role.to_string(),
		}),
	)
		.into_response()
}

/// Get invitation details by token.
///
/// # Authorization
///
/// Public endpoint. Anyone with the token can view invitation details.
///
/// # Request
///
/// Path parameters:
/// - `token`: The invitation token
///
/// # Response
///
/// Returns [`InvitationResponse`] with invitation details.
///
/// # Errors
///
/// - `404 Not Found`: Invitation not found or expired
/// - `500 Internal Server Error`: Database error
///
/// # Security
///
/// - Token acts as authentication for this endpoint
/// - Limited information is exposed (no sensitive org data)
#[utoipa::path(
    get,
    path = "/api/invitations/{token}",
    params(
        ("token" = String, Path, description = "Invitation token")
    ),
    responses(
        (status = 200, description = "Invitation details", body = InvitationResponse),
        (status = 404, description = "Invitation not found or expired", body = InvitationErrorResponse)
    ),
    tag = "invitations"
)]
#[tracing::instrument(skip(state, token))]
pub async fn get_invitation(
	State(state): State<AppState>,
	Path(token): Path<String>,
) -> impl IntoResponse {
	let locale = &state.default_locale;

	let token_hash = hash_token(&token);

	let invitation = match state
		.org_repo
		.get_invitation_by_token_hash(&token_hash)
		.await
	{
		Ok(Some(inv)) => inv,
		Ok(None) => {
			return (
				StatusCode::NOT_FOUND,
				Json(InvitationErrorResponse {
					error: "not_found".to_string(),
					message: t(locale, "server.api.invitation.invalid_token").to_string(),
				}),
			)
				.into_response();
		}
		Err(e) => {
			tracing::error!(error = %e, "Failed to get invitation");
			return (
				StatusCode::INTERNAL_SERVER_ERROR,
				Json(InvitationErrorResponse {
					error: "internal_error".to_string(),
					message: t(locale, "server.api.invitation.list_failed").to_string(),
				}),
			)
				.into_response();
		}
	};

	let org = match state.org_repo.get_org_by_id(&invitation.org_id).await {
		Ok(Some(org)) => org,
		Ok(None) => {
			return (
				StatusCode::NOT_FOUND,
				Json(InvitationErrorResponse {
					error: "not_found".to_string(),
					message: t(locale, "server.api.org.not_found").to_string(),
				}),
			)
				.into_response();
		}
		Err(e) => {
			tracing::error!(error = %e, org_id = %invitation.org_id, "Failed to get organization");
			return (
				StatusCode::INTERNAL_SERVER_ERROR,
				Json(InvitationErrorResponse {
					error: "internal_error".to_string(),
					message: t(locale, "server.api.invitation.list_failed").to_string(),
				}),
			)
				.into_response();
		}
	};

	let invited_by_name = match state.user_repo.get_user_by_id(&invitation.invited_by).await {
		Ok(Some(user)) => user.display_name,
		_ => "Unknown".to_string(),
	};
	let is_expired = invitation.is_expired();

	(
		StatusCode::OK,
		Json(InvitationResponse {
			id: invitation.id.to_string(),
			org_id: invitation.org_id.to_string(),
			org_name: org.name,
			email: invitation.email,
			role: invitation.role.to_string(),
			invited_by: invitation.invited_by.to_string(),
			invited_by_name,
			created_at: invitation.created_at,
			expires_at: invitation.expires_at,
			is_expired,
		}),
	)
		.into_response()
}

/// List pending join requests for an organization.
///
/// # Authorization
///
/// Requires `ManageOrg` permission on the organization.
///
/// # Request
///
/// Path parameters:
/// - `org_id`: Organization UUID
///
/// # Response
///
/// Returns [`ListJoinRequestsResponse`] with pending requests.
///
/// # Errors
///
/// - `400 Bad Request`: Invalid organization ID format
/// - `401 Unauthorized`: Missing or invalid authentication
/// - `403 Forbidden`: Caller lacks `ManageOrg` permission
/// - `404 Not Found`: Organization does not exist
/// - `500 Internal Server Error`: Database error
#[utoipa::path(
    get,
    path = "/api/orgs/{org_id}/join-requests",
    params(
        ("org_id" = String, Path, description = "Organization ID")
    ),
    responses(
        (status = 200, description = "List of pending join requests", body = ListJoinRequestsResponse),
        (status = 401, description = "Not authenticated", body = InvitationErrorResponse),
        (status = 403, description = "Not authorized", body = InvitationErrorResponse),
        (status = 404, description = "Organization not found", body = InvitationErrorResponse)
    ),
    tag = "invitations"
)]
#[tracing::instrument(
	skip(state),
	fields(
		actor_id = %current_user.user.id,
		org_id = %org_id
	)
)]
pub async fn list_join_requests(
	RequireAuth(current_user): RequireAuth,
	State(state): State<AppState>,
	Path(org_id): Path<String>,
) -> impl IntoResponse {
	let locale = resolve_user_locale(&current_user, &state.default_locale);

	let org_id = parse_id!(
		InvitationErrorResponse,
		shared_parse_org_id(&org_id, &t(locale, "server.api.org.invalid_id"))
	);

	let org = match state.org_repo.get_org_by_id(&org_id).await {
		Ok(Some(org)) => org,
		Ok(None) => {
			return (
				StatusCode::NOT_FOUND,
				Json(InvitationErrorResponse {
					error: "not_found".to_string(),
					message: t(locale, "server.api.org.not_found").to_string(),
				}),
			)
				.into_response();
		}
		Err(e) => {
			tracing::error!(
				error = %e,
				actor_id = %current_user.user.id,
				org_id = %org_id,
				"Failed to get organization"
			);
			return (
				StatusCode::INTERNAL_SERVER_ERROR,
				Json(InvitationErrorResponse {
					error: "internal_error".to_string(),
					message: t(locale, "server.api.error.internal").to_string(),
				}),
			)
				.into_response();
		}
	};

	let subject = build_subject_attrs(&current_user, &state.org_repo, &state.team_repo).await;
	let resource = org_resource(org.id, org_visibility_to_abac(org.visibility));

	if let Err(e) = authorize!(&subject, Action::ManageOrg, &resource) {
		tracing::warn!(
			actor_id = %current_user.user.id,
			org_id = %org_id,
			"Unauthorized join request list attempt"
		);
		return e.into_response();
	}

	let requests = match state.org_repo.list_pending_join_requests(&org_id).await {
		Ok(reqs) => reqs,
		Err(e) => {
			tracing::error!(
				error = %e,
				actor_id = %current_user.user.id,
				org_id = %org_id,
				"Failed to list join requests"
			);
			return (
				StatusCode::INTERNAL_SERVER_ERROR,
				Json(InvitationErrorResponse {
					error: "internal_error".to_string(),
					message: t(locale, "server.api.error.internal").to_string(),
				}),
			)
				.into_response();
		}
	};

	let mut responses = Vec::with_capacity(requests.len());
	for req in requests {
		let (display_name, email) = match state.user_repo.get_user_by_id(&req.user_id).await {
			Ok(Some(user)) => (user.display_name, user.primary_email),
			_ => ("Unknown".to_string(), None),
		};

		responses.push(JoinRequestResponse {
			id: req.user_id.to_string(),
			user_id: req.user_id.to_string(),
			display_name,
			email,
			created_at: req.created_at,
		});
	}

	tracing::info!(
		actor_id = %current_user.user.id,
		org_id = %org_id,
		request_count = responses.len(),
		"Listed join requests"
	);

	(
		StatusCode::OK,
		Json(ListJoinRequestsResponse {
			requests: responses,
		}),
	)
		.into_response()
}

/// Create a join request for an organization.
///
/// # Authorization
///
/// Requires authentication. Any authenticated user can request to join
/// public or unlisted organizations.
///
/// # Request
///
/// Path parameters:
/// - `org_id`: Organization UUID
///
/// # Response
///
/// Returns [`InvitationSuccessResponse`] confirming submission.
///
/// # Errors
///
/// - `400 Bad Request`: Invalid organization ID format
/// - `401 Unauthorized`: Missing or invalid authentication
/// - `403 Forbidden`: Organization is private
/// - `404 Not Found`: Organization does not exist
/// - `409 Conflict`: Already a member or pending request exists
/// - `500 Internal Server Error`: Database error
#[utoipa::path(
    post,
    path = "/api/orgs/{org_id}/join-requests",
    params(
        ("org_id" = String, Path, description = "Organization ID")
    ),
    responses(
        (status = 201, description = "Join request created", body = InvitationSuccessResponse),
        (status = 401, description = "Not authenticated", body = InvitationErrorResponse),
        (status = 404, description = "Organization not found", body = InvitationErrorResponse),
        (status = 409, description = "Already a member or pending request", body = InvitationErrorResponse)
    ),
    tag = "invitations"
)]
#[tracing::instrument(
	skip(state),
	fields(
		actor_id = %current_user.user.id,
		org_id = %org_id
	)
)]
pub async fn create_join_request(
	RequireAuth(current_user): RequireAuth,
	State(state): State<AppState>,
	Path(org_id): Path<String>,
) -> impl IntoResponse {
	let locale = resolve_user_locale(&current_user, &state.default_locale);

	let org_id = parse_id!(
		InvitationErrorResponse,
		shared_parse_org_id(&org_id, &t(locale, "server.api.org.invalid_id"))
	);

	let org = match state.org_repo.get_org_by_id(&org_id).await {
		Ok(Some(org)) => org,
		Ok(None) => {
			return (
				StatusCode::NOT_FOUND,
				Json(InvitationErrorResponse {
					error: "not_found".to_string(),
					message: t(locale, "server.api.org.not_found").to_string(),
				}),
			)
				.into_response();
		}
		Err(e) => {
			tracing::error!(
				error = %e,
				actor_id = %current_user.user.id,
				org_id = %org_id,
				"Failed to get organization"
			);
			return (
				StatusCode::INTERNAL_SERVER_ERROR,
				Json(InvitationErrorResponse {
					error: "internal_error".to_string(),
					message: t(locale, "server.api.error.internal").to_string(),
				}),
			)
				.into_response();
		}
	};

	if org.visibility == OrgVisibility::Private {
		tracing::warn!(
			actor_id = %current_user.user.id,
			org_id = %org_id,
			"Join request denied for private organization"
		);
		return (
			StatusCode::FORBIDDEN,
			Json(InvitationErrorResponse {
				error: "forbidden".to_string(),
				message: t(locale, "server.api.join_request.private_org").to_string(),
			}),
		)
			.into_response();
	}

	if let Ok(Some(_)) = state
		.org_repo
		.get_membership(&org_id, &current_user.user.id)
		.await
	{
		return (
			StatusCode::CONFLICT,
			Json(InvitationErrorResponse {
				error: "already_member".to_string(),
				message: t(locale, "server.api.join_request.already_member").to_string(),
			}),
		)
			.into_response();
	}

	if let Err(e) = state
		.org_repo
		.create_join_request(&org_id, &current_user.user.id)
		.await
	{
		tracing::error!(
			error = %e,
			actor_id = %current_user.user.id,
			org_id = %org_id,
			"Failed to create join request"
		);
		return (
			StatusCode::INTERNAL_SERVER_ERROR,
			Json(InvitationErrorResponse {
				error: "internal_error".to_string(),
				message: t(locale, "server.api.error.internal").to_string(),
			}),
		)
			.into_response();
	}

	tracing::info!(
		actor_id = %current_user.user.id,
		org_id = %org_id,
		"Join request created"
	);

	(
		StatusCode::CREATED,
		Json(InvitationSuccessResponse {
			message: t(locale, "server.api.join_request.created").to_string(),
		}),
	)
		.into_response()
}

/// Approve a join request.
///
/// # Authorization
///
/// Requires `ManageOrg` permission on the organization.
///
/// # Request
///
/// Path parameters:
/// - `org_id`: Organization UUID
/// - `request_id`: Join request ID
///
/// # Response
///
/// Returns [`InvitationSuccessResponse`] confirming approval.
///
/// # Errors
///
/// - `400 Bad Request`: Request already handled
/// - `401 Unauthorized`: Missing or invalid authentication
/// - `403 Forbidden`: Caller lacks `ManageOrg` permission
/// - `404 Not Found`: Organization or request not found
/// - `500 Internal Server Error`: Database error
#[utoipa::path(
    post,
    path = "/api/orgs/{org_id}/join-requests/{request_id}/approve",
    params(
        ("org_id" = String, Path, description = "Organization ID"),
        ("request_id" = String, Path, description = "Join request ID")
    ),
    responses(
        (status = 200, description = "Join request approved", body = InvitationSuccessResponse),
        (status = 401, description = "Not authenticated", body = InvitationErrorResponse),
        (status = 403, description = "Not authorized", body = InvitationErrorResponse),
        (status = 404, description = "Request not found", body = InvitationErrorResponse)
    ),
    tag = "invitations"
)]
#[tracing::instrument(
	skip(state),
	fields(
		actor_id = %current_user.user.id,
		org_id = %org_id,
		request_id = %request_id
	)
)]
pub async fn approve_join_request(
	RequireAuth(current_user): RequireAuth,
	State(state): State<AppState>,
	Path((org_id, request_id)): Path<(String, String)>,
) -> impl IntoResponse {
	let locale = resolve_user_locale(&current_user, &state.default_locale);

	let org_id = parse_id!(
		InvitationErrorResponse,
		shared_parse_org_id(&org_id, &t(locale, "server.api.org.invalid_id"))
	);

	let org = match state.org_repo.get_org_by_id(&org_id).await {
		Ok(Some(org)) => org,
		Ok(None) => {
			return (
				StatusCode::NOT_FOUND,
				Json(InvitationErrorResponse {
					error: "not_found".to_string(),
					message: t(locale, "server.api.org.not_found").to_string(),
				}),
			)
				.into_response();
		}
		Err(e) => {
			tracing::error!(
				error = %e,
				actor_id = %current_user.user.id,
				org_id = %org_id,
				"Failed to get organization"
			);
			return (
				StatusCode::INTERNAL_SERVER_ERROR,
				Json(InvitationErrorResponse {
					error: "internal_error".to_string(),
					message: t(locale, "server.api.error.internal").to_string(),
				}),
			)
				.into_response();
		}
	};

	let subject = build_subject_attrs(&current_user, &state.org_repo, &state.team_repo).await;
	let resource = org_resource(org.id, org_visibility_to_abac(org.visibility));

	if let Err(e) = authorize!(&subject, Action::ManageOrg, &resource) {
		tracing::warn!(
			actor_id = %current_user.user.id,
			org_id = %org_id,
			request_id = %request_id,
			"Unauthorized join request approval attempt"
		);
		return e.into_response();
	}

	let join_request = match state.org_repo.get_join_request(&request_id).await {
		Ok(Some(req)) => req,
		Ok(None) => {
			return (
				StatusCode::NOT_FOUND,
				Json(InvitationErrorResponse {
					error: "not_found".to_string(),
					message: t(locale, "server.api.join_request.not_found").to_string(),
				}),
			)
				.into_response();
		}
		Err(e) => {
			tracing::error!(
				error = %e,
				actor_id = %current_user.user.id,
				request_id = %request_id,
				"Failed to get join request"
			);
			return (
				StatusCode::INTERNAL_SERVER_ERROR,
				Json(InvitationErrorResponse {
					error: "internal_error".to_string(),
					message: t(locale, "server.api.error.internal").to_string(),
				}),
			)
				.into_response();
		}
	};

	if join_request.org_id != org_id {
		return (
			StatusCode::NOT_FOUND,
			Json(InvitationErrorResponse {
				error: "not_found".to_string(),
				message: t(locale, "server.api.join_request.not_found").to_string(),
			}),
		)
			.into_response();
	}

	if join_request.handled_at.is_some() {
		return (
			StatusCode::BAD_REQUEST,
			Json(InvitationErrorResponse {
				error: "already_handled".to_string(),
				message: t(locale, "server.api.join_request.already_pending").to_string(),
			}),
		)
			.into_response();
	}

	if let Err(e) = state
		.org_repo
		.add_member(&org_id, &join_request.user_id, OrgRole::Member)
		.await
	{
		tracing::error!(
			error = %e,
			actor_id = %current_user.user.id,
			target_id = %join_request.user_id,
			org_id = %org_id,
			"Failed to add member"
		);
		return (
			StatusCode::INTERNAL_SERVER_ERROR,
			Json(InvitationErrorResponse {
				error: "internal_error".to_string(),
				message: t(locale, "server.api.error.internal").to_string(),
			}),
		)
			.into_response();
	}

	if let Err(e) = state
		.org_repo
		.approve_join_request(&request_id, &current_user.user.id)
		.await
	{
		tracing::error!(
			error = %e,
			actor_id = %current_user.user.id,
			request_id = %request_id,
			"Failed to mark join request as approved"
		);
	}

	state.audit_service.log(
		AuditLogBuilder::new(AuditEventType::MemberAdded)
			.actor(AuditUserId::new(current_user.user.id.into_inner()))
			.resource("org", org_id.to_string())
			.details(serde_json::json!({
				"action": "join_request_approved",
				"request_id": &request_id,
				"target_user_id": join_request.user_id.to_string(),
			}))
			.build(),
	);

	tracing::info!(
		actor_id = %current_user.user.id,
		target_id = %join_request.user_id,
		org_id = %org_id,
		request_id = %request_id,
		"Join request approved"
	);

	(
		StatusCode::OK,
		Json(InvitationSuccessResponse {
			message: t(locale, "server.api.join_request.approved").to_string(),
		}),
	)
		.into_response()
}

/// Reject a join request.
///
/// # Authorization
///
/// Requires `ManageOrg` permission on the organization.
///
/// # Request
///
/// Path parameters:
/// - `org_id`: Organization UUID
/// - `request_id`: Join request ID
///
/// # Response
///
/// Returns [`InvitationSuccessResponse`] confirming rejection.
///
/// # Errors
///
/// - `400 Bad Request`: Request already handled
/// - `401 Unauthorized`: Missing or invalid authentication
/// - `403 Forbidden`: Caller lacks `ManageOrg` permission
/// - `404 Not Found`: Organization or request not found
/// - `500 Internal Server Error`: Database error
#[utoipa::path(
    post,
    path = "/api/orgs/{org_id}/join-requests/{request_id}/reject",
    params(
        ("org_id" = String, Path, description = "Organization ID"),
        ("request_id" = String, Path, description = "Join request ID")
    ),
    responses(
        (status = 200, description = "Join request rejected", body = InvitationSuccessResponse),
        (status = 401, description = "Not authenticated", body = InvitationErrorResponse),
        (status = 403, description = "Not authorized", body = InvitationErrorResponse),
        (status = 404, description = "Request not found", body = InvitationErrorResponse)
    ),
    tag = "invitations"
)]
#[tracing::instrument(
	skip(state),
	fields(
		actor_id = %current_user.user.id,
		org_id = %org_id,
		request_id = %request_id
	)
)]
pub async fn reject_join_request(
	RequireAuth(current_user): RequireAuth,
	State(state): State<AppState>,
	Path((org_id, request_id)): Path<(String, String)>,
) -> impl IntoResponse {
	let locale = resolve_user_locale(&current_user, &state.default_locale);

	let org_id = parse_id!(
		InvitationErrorResponse,
		shared_parse_org_id(&org_id, &t(locale, "server.api.org.invalid_id"))
	);

	let org = match state.org_repo.get_org_by_id(&org_id).await {
		Ok(Some(org)) => org,
		Ok(None) => {
			return (
				StatusCode::NOT_FOUND,
				Json(InvitationErrorResponse {
					error: "not_found".to_string(),
					message: t(locale, "server.api.org.not_found").to_string(),
				}),
			)
				.into_response();
		}
		Err(e) => {
			tracing::error!(
				error = %e,
				actor_id = %current_user.user.id,
				org_id = %org_id,
				"Failed to get organization"
			);
			return (
				StatusCode::INTERNAL_SERVER_ERROR,
				Json(InvitationErrorResponse {
					error: "internal_error".to_string(),
					message: t(locale, "server.api.error.internal").to_string(),
				}),
			)
				.into_response();
		}
	};

	let subject = build_subject_attrs(&current_user, &state.org_repo, &state.team_repo).await;
	let resource = org_resource(org.id, org_visibility_to_abac(org.visibility));

	if let Err(e) = authorize!(&subject, Action::ManageOrg, &resource) {
		tracing::warn!(
			actor_id = %current_user.user.id,
			org_id = %org_id,
			request_id = %request_id,
			"Unauthorized join request rejection attempt"
		);
		return e.into_response();
	}

	let join_request = match state.org_repo.get_join_request(&request_id).await {
		Ok(Some(req)) => req,
		Ok(None) => {
			return (
				StatusCode::NOT_FOUND,
				Json(InvitationErrorResponse {
					error: "not_found".to_string(),
					message: t(locale, "server.api.join_request.not_found").to_string(),
				}),
			)
				.into_response();
		}
		Err(e) => {
			tracing::error!(
				error = %e,
				actor_id = %current_user.user.id,
				request_id = %request_id,
				"Failed to get join request"
			);
			return (
				StatusCode::INTERNAL_SERVER_ERROR,
				Json(InvitationErrorResponse {
					error: "internal_error".to_string(),
					message: t(locale, "server.api.error.internal").to_string(),
				}),
			)
				.into_response();
		}
	};

	if join_request.org_id != org_id {
		return (
			StatusCode::NOT_FOUND,
			Json(InvitationErrorResponse {
				error: "not_found".to_string(),
				message: t(locale, "server.api.join_request.not_found").to_string(),
			}),
		)
			.into_response();
	}

	if join_request.handled_at.is_some() {
		return (
			StatusCode::BAD_REQUEST,
			Json(InvitationErrorResponse {
				error: "already_handled".to_string(),
				message: t(locale, "server.api.join_request.already_pending").to_string(),
			}),
		)
			.into_response();
	}

	if let Err(e) = state
		.org_repo
		.reject_join_request(&request_id, &current_user.user.id)
		.await
	{
		tracing::error!(
			error = %e,
			actor_id = %current_user.user.id,
			request_id = %request_id,
			"Failed to reject join request"
		);
		return (
			StatusCode::INTERNAL_SERVER_ERROR,
			Json(InvitationErrorResponse {
				error: "internal_error".to_string(),
				message: t(locale, "server.api.error.internal").to_string(),
			}),
		)
			.into_response();
	}

	tracing::info!(
		actor_id = %current_user.user.id,
		target_id = %join_request.user_id,
		org_id = %org_id,
		request_id = %request_id,
		"Join request rejected"
	);

	(
		StatusCode::OK,
		Json(InvitationSuccessResponse {
			message: t(locale, "server.api.join_request.rejected").to_string(),
		}),
	)
		.into_response()
}
