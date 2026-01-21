// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights
// reserved. SPDX-License-Identifier: Proprietary

//! Organization management HTTP handlers.
//!
//! Implements organization endpoints per the auth-abac-system.md specification:
//! - List organizations
//! - Create organization
//! - Get/update/delete organization
//! - Manage organization members

use axum::{
	extract::{Path, State},
	http::StatusCode,
	response::IntoResponse,
	Json,
};
use chrono::Utc;
use loom_flags_core::{Environment, EnvironmentId};
pub use loom_server_api::orgs::{
	AddOrgMemberRequest, CreateOrgRequest, JoinRequestResponse, ListJoinRequestsResponse,
	ListOrgMembersResponse, ListOrgsResponse, OrgErrorResponse, OrgMemberResponse, OrgResponse,
	OrgSuccessResponse, OrgVisibilityApi, UpdateOrgMemberRoleRequest, UpdateOrgRequest,
};
use loom_server_audit::{AuditEventType, AuditLogBuilder, UserId as AuditUserId};
use loom_server_auth::{
	is_username_reserved,
	org::{OrgVisibility, Organization},
	types::{OrgId, OrgRole},
	Action, Visibility,
};
use loom_server_flags::FlagsRepository;

use crate::{
	abac_middleware::{build_subject_attrs, org_resource},
	api::AppState,
	api_response::{bad_request, conflict, internal_error, not_found},
	auth_middleware::RequireAuth,
	authorize,
	i18n::{resolve_user_locale, t},
	impl_api_error_response, parse_id, parse_role, validate_slug_or_error,
	validation::{
		parse_org_id as shared_parse_org_id, parse_org_role, parse_user_id as shared_parse_user_id,
		validate_slug_with_error,
	},
};

impl_api_error_response!(OrgErrorResponse);

fn org_visibility_to_abac(v: OrgVisibility) -> Visibility {
	match v {
		OrgVisibility::Public => Visibility::Public,
		OrgVisibility::Unlisted => Visibility::Organization,
		OrgVisibility::Private => Visibility::Private,
	}
}

#[utoipa::path(
    get,
    path = "/api/orgs",
    responses(
        (status = 200, description = "List of organizations", body = ListOrgsResponse),
        (status = 401, description = "Not authenticated", body = OrgErrorResponse)
    ),
    tag = "orgs"
)]
/// List organizations the user belongs to.
///
/// Returns all organizations where the current user is a member,
/// including their personal organization.
///
/// # Authorization
/// Requires authentication. No additional ABAC checks - users can only
/// see organizations they are members of.
///
/// # Response
/// Returns a list of organizations with basic details and member counts.
///
/// # Errors
/// - 401: Not authenticated
/// - 500: Internal server error
#[tracing::instrument(skip(state))]
pub async fn list_orgs(
	RequireAuth(current_user): RequireAuth,
	State(state): State<AppState>,
) -> impl IntoResponse {
	let locale = resolve_user_locale(&current_user, &state.default_locale);
	let user_id = current_user.user.id;
	let orgs = match state.org_repo.list_orgs_for_user(&user_id).await {
		Ok(orgs) => orgs,
		Err(e) => {
			tracing::error!(error = %e, %user_id, "Failed to list orgs for user");
			return (
				StatusCode::INTERNAL_SERVER_ERROR,
				Json(OrgErrorResponse {
					error: "internal_error".to_string(),
					message: t(locale, "server.api.org.list_failed").to_string(),
				}),
			)
				.into_response();
		}
	};

	let mut org_responses = Vec::with_capacity(orgs.len());
	for org in orgs {
		let member_count = match state.org_repo.list_members(&org.id).await {
			Ok(members) => Some(members.len() as i64),
			Err(_) => None,
		};
		org_responses.push(OrgResponse::from_org(org, member_count));
	}

	(
		StatusCode::OK,
		Json(ListOrgsResponse {
			orgs: org_responses,
		}),
	)
		.into_response()
}

#[utoipa::path(
    post,
    path = "/api/orgs",
    request_body = CreateOrgRequest,
    responses(
        (status = 201, description = "Organization created", body = OrgResponse),
        (status = 400, description = "Invalid request", body = OrgErrorResponse),
        (status = 401, description = "Not authenticated", body = OrgErrorResponse),
        (status = 409, description = "Slug already exists", body = OrgErrorResponse)
    ),
    tag = "orgs"
)]
/// Create a new organization.
///
/// Creates a new organization with the current user as the owner.
///
/// # Authorization
/// Requires authentication. Any authenticated user can create organizations.
///
/// # Request Body
/// - `name`: Organization display name (required)
/// - `slug`: URL-safe identifier (required, 3-50 chars, lowercase alphanumeric and hyphens)
/// - `visibility`: Optional visibility setting (public, unlisted, private)
///
/// # Response
/// Returns the created organization with member count of 1.
///
/// # Errors
/// - 400: Invalid slug format or name
/// - 401: Not authenticated
/// - 409: Slug already exists
/// - 500: Internal server error
#[tracing::instrument(skip(state, payload), fields(slug = %payload.slug))]
pub async fn create_org(
	RequireAuth(current_user): RequireAuth,
	State(state): State<AppState>,
	Json(payload): Json<CreateOrgRequest>,
) -> impl IntoResponse {
	let locale = resolve_user_locale(&current_user, &state.default_locale);
	let user_id = current_user.user.id;

	validate_slug_or_error!(
		OrgErrorResponse,
		validate_slug_with_error(
			&payload.slug,
			3,
			50,
			&t(locale, "server.api.org.invalid_slug_length"),
			&t(locale, "server.api.org.invalid_slug_format")
		)
	);

	if is_username_reserved(&payload.slug) {
		return bad_request::<OrgErrorResponse>(
			"slug_reserved",
			t(locale, "server.api.org.slug_reserved"),
		)
		.into_response();
	}

	match state.org_repo.get_org_by_slug(&payload.slug).await {
		Ok(Some(_)) => {
			return conflict::<OrgErrorResponse>("slug_exists", t(locale, "server.api.org.slug_exists"))
				.into_response();
		}
		Ok(None) => {}
		Err(e) => {
			tracing::error!(error = %e, %user_id, "Failed to check slug uniqueness");
			return internal_error::<OrgErrorResponse>(t(locale, "server.api.org.create_failed"))
				.into_response();
		}
	}

	let visibility = payload
		.visibility
		.map(OrgVisibility::from)
		.unwrap_or(OrgVisibility::Public);

	let org = Organization {
		id: OrgId::generate(),
		name: payload.name,
		slug: payload.slug,
		visibility,
		is_personal: false,
		created_at: Utc::now(),
		updated_at: Utc::now(),
		deleted_at: None,
	};

	let org_id = org.id;
	if let Err(e) = state.org_repo.create_org(&org).await {
		tracing::error!(error = %e, %user_id, %org_id, "Failed to create organization");
		return (
			StatusCode::INTERNAL_SERVER_ERROR,
			Json(OrgErrorResponse {
				error: "internal_error".to_string(),
				message: t(locale, "server.api.org.create_failed").to_string(),
			}),
		)
			.into_response();
	}

	if let Err(e) = state
		.org_repo
		.add_member(&org.id, &current_user.user.id, OrgRole::Owner)
		.await
	{
		tracing::error!(error = %e, %user_id, %org_id, "Failed to add owner to organization");
		return (
			StatusCode::INTERNAL_SERVER_ERROR,
			Json(OrgErrorResponse {
				error: "internal_error".to_string(),
				message: t(locale, "server.api.org.create_failed").to_string(),
			}),
		)
			.into_response();
	}

	tracing::info!(%user_id, %org_id, "Organization created");

	// Auto-create dev and prod environments for the new organization
	let flags_org_id = loom_flags_core::OrgId(org_id.into_inner());
	for (name, color) in Environment::default_environments() {
		let env = Environment {
			id: EnvironmentId::new(),
			org_id: flags_org_id,
			name: name.to_string(),
			color: Some(color.to_string()),
			created_at: Utc::now(),
		};
		if let Err(e) = state.flags_repo.create_environment(&env).await {
			tracing::warn!(error = %e, env_name = %name, %org_id, "Failed to create default environment");
			// Non-fatal: org creation still succeeds even if env creation fails
		} else {
			tracing::info!(env_id = %env.id, env_name = %name, %org_id, "Default environment created");
		}
	}

	state.audit_service.log(
		AuditLogBuilder::new(AuditEventType::OrgCreated)
			.actor(AuditUserId::new(user_id.into_inner()))
			.resource("org", org.id.to_string())
			.details(serde_json::json!({
				"name": org.name,
				"slug": org.slug,
				"visibility": format!("{:?}", org.visibility),
			}))
			.build(),
	);

	(
		StatusCode::CREATED,
		Json(OrgResponse::from_org(org, Some(1))),
	)
		.into_response()
}

#[utoipa::path(
    get,
    path = "/api/orgs/{id}",
    params(
        ("id" = String, Path, description = "Organization ID")
    ),
    responses(
        (status = 200, description = "Organization details", body = OrgResponse),
        (status = 401, description = "Not authenticated", body = OrgErrorResponse),
        (status = 403, description = "Access denied", body = OrgErrorResponse),
        (status = 404, description = "Organization not found", body = OrgErrorResponse)
    ),
    tag = "orgs"
)]
/// Get organization details.
///
/// Returns detailed information about an organization if the user has access.
///
/// # Authorization
/// Requires authentication. ABAC check for `Action::Read` on the organization resource.
/// - Public orgs: Anyone can read
/// - Unlisted orgs: Members only
/// - Private orgs: Members only
///
/// # Path Parameters
/// - `id`: Organization UUID
///
/// # Response
/// Returns organization details including member count.
///
/// # Errors
/// - 400: Invalid organization ID format
/// - 401: Not authenticated
/// - 403: Access denied (ABAC check failed)
/// - 404: Organization not found
/// - 500: Internal server error
#[tracing::instrument(skip(state), fields(%org_id))]
pub async fn get_org(
	RequireAuth(current_user): RequireAuth,
	State(state): State<AppState>,
	Path(org_id): Path<String>,
) -> impl IntoResponse {
	let locale = resolve_user_locale(&current_user, &state.default_locale);
	let org_id = parse_id!(
		OrgErrorResponse,
		shared_parse_org_id(&org_id, &t(locale, "server.api.org.invalid_id"))
	);

	let org = match state.org_repo.get_org_by_id(&org_id).await {
		Ok(Some(org)) => org,
		Ok(None) => {
			return not_found::<OrgErrorResponse>(t(locale, "server.api.org.not_found")).into_response();
		}
		Err(e) => {
			tracing::error!(error = %e, %org_id, "Failed to get organization");
			return internal_error::<OrgErrorResponse>(t(locale, "server.api.error.internal"))
				.into_response();
		}
	};

	let subject = build_subject_attrs(&current_user, &state.org_repo, &state.team_repo).await;
	let resource = org_resource(org.id, org_visibility_to_abac(org.visibility));

	if let Err(e) = authorize!(&subject, Action::Read, &resource) {
		return e.into_response();
	}

	let member_count = match state.org_repo.list_members(&org.id).await {
		Ok(members) => Some(members.len() as i64),
		Err(_) => None,
	};

	(
		StatusCode::OK,
		Json(OrgResponse::from_org(org, member_count)),
	)
		.into_response()
}

#[utoipa::path(
    patch,
    path = "/api/orgs/{id}",
    params(
        ("id" = String, Path, description = "Organization ID")
    ),
    request_body = UpdateOrgRequest,
    responses(
        (status = 200, description = "Organization updated", body = OrgResponse),
        (status = 400, description = "Invalid request", body = OrgErrorResponse),
        (status = 401, description = "Not authenticated", body = OrgErrorResponse),
        (status = 403, description = "Not authorized to update", body = OrgErrorResponse),
        (status = 404, description = "Organization not found", body = OrgErrorResponse),
        (status = 409, description = "Slug already exists", body = OrgErrorResponse)
    ),
    tag = "orgs"
)]
/// Update an organization.
///
/// Updates organization settings like name, slug, and visibility.
///
/// # Authorization
/// Requires authentication. ABAC check for `Action::Write` on the organization.
/// Only owners and admins can update organization settings.
///
/// # Path Parameters
/// - `id`: Organization UUID
///
/// # Request Body
/// All fields are optional:
/// - `name`: New display name
/// - `slug`: New URL-safe identifier (must be unique)
/// - `visibility`: New visibility setting
///
/// # Response
/// Returns the updated organization.
///
/// # Errors
/// - 400: Invalid slug format
/// - 401: Not authenticated
/// - 403: Not authorized (not owner/admin)
/// - 404: Organization not found
/// - 409: New slug already exists
/// - 500: Internal server error
#[tracing::instrument(skip(state, payload), fields(%org_id))]
pub async fn update_org(
	RequireAuth(current_user): RequireAuth,
	State(state): State<AppState>,
	Path(org_id): Path<String>,
	Json(payload): Json<UpdateOrgRequest>,
) -> impl IntoResponse {
	let locale = resolve_user_locale(&current_user, &state.default_locale);
	let org_id = parse_id!(
		OrgErrorResponse,
		shared_parse_org_id(&org_id, &t(locale, "server.api.org.invalid_id"))
	);

	let mut org = match state.org_repo.get_org_by_id(&org_id).await {
		Ok(Some(org)) => org,
		Ok(None) => {
			return (
				StatusCode::NOT_FOUND,
				Json(OrgErrorResponse {
					error: "not_found".to_string(),
					message: t(locale, "server.api.org.not_found").to_string(),
				}),
			)
				.into_response();
		}
		Err(e) => {
			tracing::error!(error = %e, %org_id, "Failed to get organization");
			return (
				StatusCode::INTERNAL_SERVER_ERROR,
				Json(OrgErrorResponse {
					error: "internal_error".to_string(),
					message: t(locale, "server.api.error.internal").to_string(),
				}),
			)
				.into_response();
		}
	};

	let subject = build_subject_attrs(&current_user, &state.org_repo, &state.team_repo).await;
	let resource = org_resource(org.id, org_visibility_to_abac(org.visibility));

	if let Err(e) = authorize!(&subject, Action::Write, &resource) {
		return e.into_response();
	}

	if let Some(ref new_slug) = payload.slug {
		if new_slug != &org.slug {
			validate_slug_or_error!(
				OrgErrorResponse,
				validate_slug_with_error(
					new_slug,
					3,
					50,
					&t(locale, "server.api.org.invalid_slug_length"),
					&t(locale, "server.api.org.invalid_slug_format")
				)
			);

			if is_username_reserved(new_slug) {
				return bad_request::<OrgErrorResponse>(
					"slug_reserved",
					t(locale, "server.api.org.slug_reserved"),
				)
				.into_response();
			}

			match state.org_repo.get_org_by_slug(new_slug).await {
				Ok(Some(_)) => {
					return (
						StatusCode::CONFLICT,
						Json(OrgErrorResponse {
							error: "slug_exists".to_string(),
							message: t(locale, "server.api.org.slug_exists").to_string(),
						}),
					)
						.into_response();
				}
				Ok(None) => {}
				Err(e) => {
					tracing::error!(error = %e, %org_id, "Failed to check slug uniqueness");
					return (
						StatusCode::INTERNAL_SERVER_ERROR,
						Json(OrgErrorResponse {
							error: "internal_error".to_string(),
							message: t(locale, "server.api.error.internal").to_string(),
						}),
					)
						.into_response();
				}
			}
			org.slug = new_slug.clone();
		}
	}

	if let Some(name) = payload.name {
		org.name = name;
	}

	if let Some(visibility) = payload.visibility {
		org.visibility = visibility.into();
	}

	org.updated_at = Utc::now();

	if let Err(e) = state.org_repo.update_org(&org).await {
		tracing::error!(error = %e, %org_id, "Failed to update organization");
		return (
			StatusCode::INTERNAL_SERVER_ERROR,
			Json(OrgErrorResponse {
				error: "internal_error".to_string(),
				message: t(locale, "server.api.error.internal").to_string(),
			}),
		)
			.into_response();
	}

	tracing::info!(%org_id, user_id = %current_user.user.id, "Organization updated");

	state.audit_service.log(
		AuditLogBuilder::new(AuditEventType::OrgUpdated)
			.actor(AuditUserId::new(current_user.user.id.into_inner()))
			.resource("org", org.id.to_string())
			.details(serde_json::json!({
				"name": org.name,
				"slug": org.slug,
				"visibility": format!("{:?}", org.visibility),
			}))
			.build(),
	);

	let member_count = match state.org_repo.list_members(&org.id).await {
		Ok(members) => Some(members.len() as i64),
		Err(_) => None,
	};

	(
		StatusCode::OK,
		Json(OrgResponse::from_org(org, member_count)),
	)
		.into_response()
}

#[utoipa::path(
    delete,
    path = "/api/orgs/{id}",
    params(
        ("id" = String, Path, description = "Organization ID")
    ),
    responses(
        (status = 200, description = "Organization deleted", body = OrgSuccessResponse),
        (status = 401, description = "Not authenticated", body = OrgErrorResponse),
        (status = 403, description = "Not authorized to delete", body = OrgErrorResponse),
        (status = 404, description = "Organization not found", body = OrgErrorResponse)
    ),
    tag = "orgs"
)]
/// Soft-delete an organization.
///
/// Marks an organization for deletion. Personal organizations cannot be deleted.
/// The deletion is soft and can be restored within a grace period.
///
/// # Authorization
/// Requires authentication. ABAC check for `Action::Delete` on the organization.
/// Only owners can delete an organization.
///
/// # Path Parameters
/// - `id`: Organization UUID
///
/// # Response
/// Returns a success message.
///
/// # Errors
/// - 400: Cannot delete personal organization
/// - 401: Not authenticated
/// - 403: Not authorized (not owner)
/// - 404: Organization not found
/// - 500: Internal server error
#[tracing::instrument(skip(state), fields(%org_id))]
pub async fn delete_org(
	RequireAuth(current_user): RequireAuth,
	State(state): State<AppState>,
	Path(org_id): Path<String>,
) -> impl IntoResponse {
	let locale = resolve_user_locale(&current_user, &state.default_locale);
	let org_id = parse_id!(
		OrgErrorResponse,
		shared_parse_org_id(&org_id, &t(locale, "server.api.org.invalid_id"))
	);

	let org = match state.org_repo.get_org_by_id(&org_id).await {
		Ok(Some(org)) => org,
		Ok(None) => {
			return (
				StatusCode::NOT_FOUND,
				Json(OrgErrorResponse {
					error: "not_found".to_string(),
					message: t(locale, "server.api.org.not_found").to_string(),
				}),
			)
				.into_response();
		}
		Err(e) => {
			tracing::error!(error = %e, %org_id, "Failed to get organization");
			return (
				StatusCode::INTERNAL_SERVER_ERROR,
				Json(OrgErrorResponse {
					error: "internal_error".to_string(),
					message: t(locale, "server.api.error.internal").to_string(),
				}),
			)
				.into_response();
		}
	};

	if org.is_personal {
		return (
			StatusCode::BAD_REQUEST,
			Json(OrgErrorResponse {
				error: "cannot_delete_personal".to_string(),
				message: t(locale, "server.api.org.cannot_delete_personal").to_string(),
			}),
		)
			.into_response();
	}

	let subject = build_subject_attrs(&current_user, &state.org_repo, &state.team_repo).await;
	let resource = org_resource(org.id, org_visibility_to_abac(org.visibility));

	if let Err(e) = authorize!(&subject, Action::Delete, &resource) {
		return e.into_response();
	}

	if let Err(e) = state.org_repo.soft_delete_org(&org_id).await {
		tracing::error!(error = %e, %org_id, "Failed to delete organization");
		return (
			StatusCode::INTERNAL_SERVER_ERROR,
			Json(OrgErrorResponse {
				error: "internal_error".to_string(),
				message: t(locale, "server.api.error.internal").to_string(),
			}),
		)
			.into_response();
	}

	tracing::info!(%org_id, user_id = %current_user.user.id, "Organization deleted");

	state.audit_service.log(
		AuditLogBuilder::new(AuditEventType::OrgDeleted)
			.actor(AuditUserId::new(current_user.user.id.into_inner()))
			.resource("org", org.id.to_string())
			.details(serde_json::json!({
				"name": org.name,
				"slug": org.slug,
			}))
			.build(),
	);

	(
		StatusCode::OK,
		Json(OrgSuccessResponse {
			message: t(locale, "server.api.org.deleted").to_string(),
		}),
	)
		.into_response()
}

#[utoipa::path(
    get,
    path = "/api/orgs/{id}/members",
    params(
        ("id" = String, Path, description = "Organization ID")
    ),
    responses(
        (status = 200, description = "List of members", body = ListOrgMembersResponse),
        (status = 401, description = "Not authenticated", body = OrgErrorResponse),
        (status = 403, description = "Access denied", body = OrgErrorResponse),
        (status = 404, description = "Organization not found", body = OrgErrorResponse)
    ),
    tag = "orgs"
)]
/// List organization members.
///
/// Returns all members of the organization with their roles.
///
/// # Authorization
/// Requires authentication. ABAC check for `Action::Read` on the organization.
/// Members can view the member list.
///
/// # Path Parameters
/// - `id`: Organization UUID
///
/// # Response
/// Returns a list of members with user details and roles.
/// Email is only included if the user has `email_visible` enabled.
///
/// # Errors
/// - 400: Invalid organization ID format
/// - 401: Not authenticated
/// - 403: Access denied
/// - 404: Organization not found
/// - 500: Internal server error
#[tracing::instrument(skip(state), fields(%org_id))]
pub async fn list_org_members(
	RequireAuth(current_user): RequireAuth,
	State(state): State<AppState>,
	Path(org_id): Path<String>,
) -> impl IntoResponse {
	let locale = resolve_user_locale(&current_user, &state.default_locale);
	let org_id = parse_id!(
		OrgErrorResponse,
		shared_parse_org_id(&org_id, &t(locale, "server.api.org.invalid_id"))
	);

	let org = match state.org_repo.get_org_by_id(&org_id).await {
		Ok(Some(org)) => org,
		Ok(None) => {
			return (
				StatusCode::NOT_FOUND,
				Json(OrgErrorResponse {
					error: "not_found".to_string(),
					message: t(locale, "server.api.org.not_found").to_string(),
				}),
			)
				.into_response();
		}
		Err(e) => {
			tracing::error!(error = %e, %org_id, "Failed to get organization");
			return (
				StatusCode::INTERNAL_SERVER_ERROR,
				Json(OrgErrorResponse {
					error: "internal_error".to_string(),
					message: t(locale, "server.api.error.internal").to_string(),
				}),
			)
				.into_response();
		}
	};

	let subject = build_subject_attrs(&current_user, &state.org_repo, &state.team_repo).await;
	let resource = org_resource(org.id, org_visibility_to_abac(org.visibility));

	if let Err(e) = authorize!(&subject, Action::Read, &resource) {
		return e.into_response();
	}

	let members = match state.org_repo.list_members(&org_id).await {
		Ok(members) => members,
		Err(e) => {
			tracing::error!(error = %e, %org_id, "Failed to list members");
			return (
				StatusCode::INTERNAL_SERVER_ERROR,
				Json(OrgErrorResponse {
					error: "internal_error".to_string(),
					message: t(locale, "server.api.error.internal").to_string(),
				}),
			)
				.into_response();
		}
	};

	let member_responses: Vec<OrgMemberResponse> = members
		.into_iter()
		.map(|(membership, user)| OrgMemberResponse {
			user_id: user.id.to_string(),
			display_name: user.display_name,
			email: if user.email_visible {
				user.primary_email
			} else {
				None
			},
			avatar_url: user.avatar_url,
			role: membership.role.to_string(),
			joined_at: membership.created_at,
		})
		.collect();

	(
		StatusCode::OK,
		Json(ListOrgMembersResponse {
			members: member_responses,
		}),
	)
		.into_response()
}

#[utoipa::path(
    post,
    path = "/api/orgs/{id}/members",
    params(
        ("id" = String, Path, description = "Organization ID")
    ),
    request_body = AddOrgMemberRequest,
    responses(
        (status = 200, description = "Member invitation sent", body = OrgSuccessResponse),
        (status = 400, description = "Invalid request", body = OrgErrorResponse),
        (status = 401, description = "Not authenticated", body = OrgErrorResponse),
        (status = 403, description = "Not authorized to add members", body = OrgErrorResponse),
        (status = 404, description = "Organization not found", body = OrgErrorResponse),
        (status = 409, description = "User already a member", body = OrgErrorResponse)
    ),
    tag = "orgs"
)]
/// Add a member to an organization.
///
/// Adds a user to the organization by email address.
///
/// # Authorization
/// Requires authentication. ABAC check for `Action::ManageOrg`.
/// Owners and admins can invite new members.
///
/// # Path Parameters
/// - `id`: Organization UUID
///
/// # Request Body
/// - `email`: Email address of the user to add
/// - `role`: Optional role (owner, admin, member). Defaults to member.
///
/// # Response
/// Returns a success message.
///
/// # Errors
/// - 400: Invalid role
/// - 401: Not authenticated
/// - 403: Not authorized (not owner/admin)
/// - 404: Organization or user not found
/// - 409: User already a member
/// - 500: Internal server error
#[tracing::instrument(skip(state, payload), fields(%org_id))]
pub async fn add_org_member(
	RequireAuth(current_user): RequireAuth,
	State(state): State<AppState>,
	Path(org_id): Path<String>,
	Json(payload): Json<AddOrgMemberRequest>,
) -> impl IntoResponse {
	let locale = resolve_user_locale(&current_user, &state.default_locale);
	let org_id = parse_id!(
		OrgErrorResponse,
		shared_parse_org_id(&org_id, &t(locale, "server.api.org.invalid_id"))
	);

	let org = match state.org_repo.get_org_by_id(&org_id).await {
		Ok(Some(org)) => org,
		Ok(None) => {
			return (
				StatusCode::NOT_FOUND,
				Json(OrgErrorResponse {
					error: "not_found".to_string(),
					message: t(locale, "server.api.org.not_found").to_string(),
				}),
			)
				.into_response();
		}
		Err(e) => {
			tracing::error!(error = %e, %org_id, "Failed to get organization");
			return (
				StatusCode::INTERNAL_SERVER_ERROR,
				Json(OrgErrorResponse {
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
		return e.into_response();
	}

	let role = match payload.role.as_deref() {
		Some(r) => parse_role!(
			OrgErrorResponse,
			parse_org_role(r, &t(locale, "server.api.org.invalid_role"))
		),
		None => OrgRole::Member,
	};

	let target_user = match state.user_repo.get_user_by_email(&payload.email).await {
		Ok(Some(user)) => user,
		Ok(None) => {
			return (
				StatusCode::NOT_FOUND,
				Json(OrgErrorResponse {
					error: "user_not_found".to_string(),
					message: t(locale, "server.api.user.not_found_by_email").to_string(),
				}),
			)
				.into_response();
		}
		Err(e) => {
			tracing::error!(error = %e, %org_id, "Failed to find user by email");
			return (
				StatusCode::INTERNAL_SERVER_ERROR,
				Json(OrgErrorResponse {
					error: "internal_error".to_string(),
					message: t(locale, "server.api.error.internal").to_string(),
				}),
			)
				.into_response();
		}
	};

	let target_user_id = target_user.id;
	if let Ok(Some(_)) = state
		.org_repo
		.get_membership(&org_id, &target_user.id)
		.await
	{
		return (
			StatusCode::CONFLICT,
			Json(OrgErrorResponse {
				error: "already_member".to_string(),
				message: t(locale, "server.api.org.already_member").to_string(),
			}),
		)
			.into_response();
	}

	if let Err(e) = state
		.org_repo
		.add_member(&org_id, &target_user.id, role)
		.await
	{
		tracing::error!(error = %e, %org_id, %target_user_id, "Failed to add member");
		return (
			StatusCode::INTERNAL_SERVER_ERROR,
			Json(OrgErrorResponse {
				error: "internal_error".to_string(),
				message: t(locale, "server.api.error.internal").to_string(),
			}),
		)
			.into_response();
	}

	tracing::info!(%org_id, %target_user_id, added_by = %current_user.user.id, "Member added to organization");

	state.audit_service.log(
		AuditLogBuilder::new(AuditEventType::MemberAdded)
			.actor(AuditUserId::new(current_user.user.id.into_inner()))
			.resource("org", org_id.to_string())
			.details(serde_json::json!({
				"target_user_id": target_user_id.to_string(),
				"role": format!("{:?}", role),
			}))
			.build(),
	);

	(
		StatusCode::OK,
		Json(OrgSuccessResponse {
			message: t(locale, "server.api.org.member_added").to_string(),
		}),
	)
		.into_response()
}

#[utoipa::path(
    delete,
    path = "/api/orgs/{org_id}/members/{user_id}",
    params(
        ("org_id" = String, Path, description = "Organization ID"),
        ("user_id" = String, Path, description = "User ID to remove")
    ),
    responses(
        (status = 200, description = "Member removed", body = OrgSuccessResponse),
        (status = 401, description = "Not authenticated", body = OrgErrorResponse),
        (status = 403, description = "Not authorized to remove members", body = OrgErrorResponse),
        (status = 404, description = "Organization or member not found", body = OrgErrorResponse)
    ),
    tag = "orgs"
)]
/// Remove a member from an organization.
///
/// Removes a user from the organization. Users can remove themselves (leave).
/// The last owner cannot be removed.
///
/// # Authorization
/// Requires authentication. ABAC check for `Action::ManageOrg` unless removing self.
/// - Owners and admins can remove members
/// - Users can remove themselves (leave the organization)
///
/// # Path Parameters
/// - `org_id`: Organization UUID
/// - `user_id`: User UUID to remove
///
/// # Response
/// Returns a success message.
///
/// # Errors
/// - 400: Cannot remove last owner
/// - 401: Not authenticated
/// - 403: Not authorized
/// - 404: Organization or member not found
/// - 500: Internal server error
#[tracing::instrument(skip(state), fields(%org_id, %user_id))]
pub async fn remove_org_member(
	RequireAuth(current_user): RequireAuth,
	State(state): State<AppState>,
	Path((org_id, user_id)): Path<(String, String)>,
) -> impl IntoResponse {
	let locale = resolve_user_locale(&current_user, &state.default_locale);
	let org_id = parse_id!(
		OrgErrorResponse,
		shared_parse_org_id(&org_id, &t(locale, "server.api.org.invalid_id"))
	);

	let target_user_id = parse_id!(
		OrgErrorResponse,
		shared_parse_user_id(&user_id, &t(locale, "server.api.user.invalid_id"))
	);

	let org = match state.org_repo.get_org_by_id(&org_id).await {
		Ok(Some(org)) => org,
		Ok(None) => {
			return (
				StatusCode::NOT_FOUND,
				Json(OrgErrorResponse {
					error: "not_found".to_string(),
					message: t(locale, "server.api.org.not_found").to_string(),
				}),
			)
				.into_response();
		}
		Err(e) => {
			tracing::error!(error = %e, %org_id, "Failed to get organization");
			return (
				StatusCode::INTERNAL_SERVER_ERROR,
				Json(OrgErrorResponse {
					error: "internal_error".to_string(),
					message: t(locale, "server.api.error.internal").to_string(),
				}),
			)
				.into_response();
		}
	};

	let is_self_removal = current_user.user.id == target_user_id;

	if !is_self_removal {
		let subject = build_subject_attrs(&current_user, &state.org_repo, &state.team_repo).await;
		let resource = org_resource(org.id, org_visibility_to_abac(org.visibility));

		if let Err(e) = authorize!(&subject, Action::ManageOrg, &resource) {
			return e.into_response();
		}
	}

	let membership = match state
		.org_repo
		.get_membership(&org_id, &target_user_id)
		.await
	{
		Ok(Some(m)) => m,
		Ok(None) => {
			return (
				StatusCode::NOT_FOUND,
				Json(OrgErrorResponse {
					error: "not_found".to_string(),
					message: t(locale, "server.api.org.member_not_found").to_string(),
				}),
			)
				.into_response();
		}
		Err(e) => {
			tracing::error!(error = %e, %org_id, %target_user_id, "Failed to get membership");
			return (
				StatusCode::INTERNAL_SERVER_ERROR,
				Json(OrgErrorResponse {
					error: "internal_error".to_string(),
					message: t(locale, "server.api.error.internal").to_string(),
				}),
			)
				.into_response();
		}
	};

	if membership.role == OrgRole::Owner {
		let owner_count = match state.org_repo.count_owners(&org_id).await {
			Ok(count) => count,
			Err(e) => {
				tracing::error!(error = %e, %org_id, "Failed to count owners");
				return (
					StatusCode::INTERNAL_SERVER_ERROR,
					Json(OrgErrorResponse {
						error: "internal_error".to_string(),
						message: t(locale, "server.api.error.internal").to_string(),
					}),
				)
					.into_response();
			}
		};

		if owner_count <= 1 {
			return (
				StatusCode::BAD_REQUEST,
				Json(OrgErrorResponse {
					error: "last_owner".to_string(),
					message: t(locale, "server.api.org.last_owner").to_string(),
				}),
			)
				.into_response();
		}
	}

	if let Err(e) = state.org_repo.remove_member(&org_id, &target_user_id).await {
		tracing::error!(error = %e, %org_id, %target_user_id, "Failed to remove member");
		return (
			StatusCode::INTERNAL_SERVER_ERROR,
			Json(OrgErrorResponse {
				error: "internal_error".to_string(),
				message: t(locale, "server.api.error.internal").to_string(),
			}),
		)
			.into_response();
	}

	tracing::info!(%org_id, %target_user_id, removed_by = %current_user.user.id, is_self_removal, "Member removed from organization");

	state.audit_service.log(
		AuditLogBuilder::new(AuditEventType::MemberRemoved)
			.actor(AuditUserId::new(current_user.user.id.into_inner()))
			.resource("org", org_id.to_string())
			.details(serde_json::json!({
				"target_user_id": target_user_id.to_string(),
				"is_self_removal": is_self_removal,
			}))
			.build(),
	);

	(
		StatusCode::OK,
		Json(OrgSuccessResponse {
			message: t(locale, "server.api.org.member_removed").to_string(),
		}),
	)
		.into_response()
}

#[utoipa::path(
    post,
    path = "/api/orgs/{id}/join-requests",
    params(
        ("id" = String, Path, description = "Organization ID")
    ),
    responses(
        (status = 201, description = "Join request created", body = OrgSuccessResponse),
        (status = 400, description = "Organization is private", body = OrgErrorResponse),
        (status = 401, description = "Not authenticated", body = OrgErrorResponse),
        (status = 404, description = "Organization not found", body = OrgErrorResponse),
        (status = 409, description = "Already a member or pending request", body = OrgErrorResponse)
    ),
    tag = "orgs"
)]
/// Request to join an organization.
///
/// Creates a pending join request for the current user.
/// Only works for public and unlisted organizations.
///
/// # Authorization
/// Requires authentication.
///
/// # Errors
/// - 400: Organization is private
/// - 401: Not authenticated
/// - 404: Organization not found
/// - 409: Already a member or pending request exists
/// - 500: Internal server error
#[tracing::instrument(skip(state), fields(%org_id))]
pub async fn create_join_request(
	RequireAuth(current_user): RequireAuth,
	State(state): State<AppState>,
	Path(org_id): Path<String>,
) -> impl IntoResponse {
	let locale = resolve_user_locale(&current_user, &state.default_locale);
	let org_id = parse_id!(
		OrgErrorResponse,
		shared_parse_org_id(&org_id, &t(locale, "server.api.org.invalid_id"))
	);

	let org = match state.org_repo.get_org_by_id(&org_id).await {
		Ok(Some(org)) => org,
		Ok(None) => {
			return (
				StatusCode::NOT_FOUND,
				Json(OrgErrorResponse {
					error: "not_found".to_string(),
					message: t(locale, "server.api.org.not_found").to_string(),
				}),
			)
				.into_response();
		}
		Err(e) => {
			tracing::error!(error = %e, %org_id, "Failed to get organization");
			return (
				StatusCode::INTERNAL_SERVER_ERROR,
				Json(OrgErrorResponse {
					error: "internal_error".to_string(),
					message: t(locale, "server.api.error.internal").to_string(),
				}),
			)
				.into_response();
		}
	};

	if org.visibility == loom_server_auth::org::OrgVisibility::Private {
		return (
			StatusCode::BAD_REQUEST,
			Json(OrgErrorResponse {
				error: "private_org".to_string(),
				message: t(locale, "server.api.org.join_request_private").to_string(),
			}),
		)
			.into_response();
	}

	let user_id = &current_user.user.id;
	if let Ok(Some(_)) = state.org_repo.get_membership(&org_id, user_id).await {
		return (
			StatusCode::CONFLICT,
			Json(OrgErrorResponse {
				error: "already_member".to_string(),
				message: t(locale, "server.api.org.already_member").to_string(),
			}),
		)
			.into_response();
	}

	match state
		.org_repo
		.has_pending_join_request(&org_id, user_id)
		.await
	{
		Ok(true) => {
			return (
				StatusCode::CONFLICT,
				Json(OrgErrorResponse {
					error: "pending_request".to_string(),
					message: t(locale, "server.api.org.join_request_pending").to_string(),
				}),
			)
				.into_response();
		}
		Ok(false) => {}
		Err(e) => {
			tracing::error!(error = %e, %org_id, "Failed to check pending join request");
			return (
				StatusCode::INTERNAL_SERVER_ERROR,
				Json(OrgErrorResponse {
					error: "internal_error".to_string(),
					message: t(locale, "server.api.error.internal").to_string(),
				}),
			)
				.into_response();
		}
	}

	match state.org_repo.create_join_request(&org_id, user_id).await {
		Ok(_id) => {
			tracing::info!(%org_id, %user_id, "Join request created");
			(
				StatusCode::CREATED,
				Json(OrgSuccessResponse {
					message: t(locale, "server.api.org.join_request_created").to_string(),
				}),
			)
				.into_response()
		}
		Err(e) => {
			tracing::error!(error = %e, %org_id, "Failed to create join request");
			(
				StatusCode::INTERNAL_SERVER_ERROR,
				Json(OrgErrorResponse {
					error: "internal_error".to_string(),
					message: t(locale, "server.api.error.internal").to_string(),
				}),
			)
				.into_response()
		}
	}
}

#[utoipa::path(
    get,
    path = "/api/orgs/{id}/join-requests",
    params(
        ("id" = String, Path, description = "Organization ID")
    ),
    responses(
        (status = 200, description = "List of pending join requests", body = ListJoinRequestsResponse),
        (status = 401, description = "Not authenticated", body = OrgErrorResponse),
        (status = 403, description = "Not authorized to view join requests", body = OrgErrorResponse),
        (status = 404, description = "Organization not found", body = OrgErrorResponse)
    ),
    tag = "orgs"
)]
/// List pending join requests.
///
/// Returns all pending join requests for the organization.
///
/// # Authorization
/// Requires authentication. ABAC check for `Action::ManageOrg`.
/// Owners and admins can view join requests.
///
/// # Errors
/// - 401: Not authenticated
/// - 403: Not authorized (not owner/admin)
/// - 404: Organization not found
/// - 500: Internal server error
#[tracing::instrument(skip(state), fields(%org_id))]
pub async fn list_join_requests(
	RequireAuth(current_user): RequireAuth,
	State(state): State<AppState>,
	Path(org_id): Path<String>,
) -> impl IntoResponse {
	let locale = resolve_user_locale(&current_user, &state.default_locale);
	let org_id = parse_id!(
		OrgErrorResponse,
		shared_parse_org_id(&org_id, &t(locale, "server.api.org.invalid_id"))
	);

	let org = match state.org_repo.get_org_by_id(&org_id).await {
		Ok(Some(org)) => org,
		Ok(None) => {
			return (
				StatusCode::NOT_FOUND,
				Json(OrgErrorResponse {
					error: "not_found".to_string(),
					message: t(locale, "server.api.org.not_found").to_string(),
				}),
			)
				.into_response();
		}
		Err(e) => {
			tracing::error!(error = %e, %org_id, "Failed to get organization");
			return (
				StatusCode::INTERNAL_SERVER_ERROR,
				Json(OrgErrorResponse {
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
		return e.into_response();
	}

	let requests = match state
		.org_repo
		.list_pending_join_requests_with_users(&org_id)
		.await
	{
		Ok(requests) => requests,
		Err(e) => {
			tracing::error!(error = %e, %org_id, "Failed to list join requests");
			return (
				StatusCode::INTERNAL_SERVER_ERROR,
				Json(OrgErrorResponse {
					error: "internal_error".to_string(),
					message: t(locale, "server.api.error.internal").to_string(),
				}),
			)
				.into_response();
		}
	};

	let request_responses: Vec<JoinRequestResponse> = requests
		.into_iter()
		.map(|(request, user)| JoinRequestResponse {
			id: request.user_id.to_string(),
			user_id: user.id.to_string(),
			display_name: user.display_name,
			email: if user.email_visible {
				user.primary_email
			} else {
				None
			},
			created_at: request.created_at,
		})
		.collect();

	(
		StatusCode::OK,
		Json(ListJoinRequestsResponse {
			requests: request_responses,
		}),
	)
		.into_response()
}

#[utoipa::path(
    post,
    path = "/api/orgs/{id}/join-requests/{request_id}/approve",
    params(
        ("id" = String, Path, description = "Organization ID"),
        ("request_id" = String, Path, description = "Join request ID")
    ),
    responses(
        (status = 200, description = "Join request approved", body = OrgSuccessResponse),
        (status = 401, description = "Not authenticated", body = OrgErrorResponse),
        (status = 403, description = "Not authorized to approve requests", body = OrgErrorResponse),
        (status = 404, description = "Organization or request not found", body = OrgErrorResponse)
    ),
    tag = "orgs"
)]
/// Approve a join request.
///
/// Approves a pending join request and adds the user as a member.
///
/// # Authorization
/// Requires authentication. ABAC check for `Action::ManageOrg`.
/// Owners and admins can approve join requests.
///
/// # Errors
/// - 401: Not authenticated
/// - 403: Not authorized (not owner/admin)
/// - 404: Organization or request not found
/// - 500: Internal server error
#[tracing::instrument(skip(state), fields(%org_id, %request_id))]
pub async fn approve_join_request(
	RequireAuth(current_user): RequireAuth,
	State(state): State<AppState>,
	Path((org_id, request_id)): Path<(String, String)>,
) -> impl IntoResponse {
	let locale = resolve_user_locale(&current_user, &state.default_locale);
	let org_id = parse_id!(
		OrgErrorResponse,
		shared_parse_org_id(&org_id, &t(locale, "server.api.org.invalid_id"))
	);

	let org = match state.org_repo.get_org_by_id(&org_id).await {
		Ok(Some(org)) => org,
		Ok(None) => {
			return (
				StatusCode::NOT_FOUND,
				Json(OrgErrorResponse {
					error: "not_found".to_string(),
					message: t(locale, "server.api.org.not_found").to_string(),
				}),
			)
				.into_response();
		}
		Err(e) => {
			tracing::error!(error = %e, %org_id, "Failed to get organization");
			return (
				StatusCode::INTERNAL_SERVER_ERROR,
				Json(OrgErrorResponse {
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
		return e.into_response();
	}

	let join_request = match state.org_repo.get_join_request(&request_id).await {
		Ok(Some(req)) if req.is_pending() && req.org_id == org_id => req,
		Ok(Some(_)) | Ok(None) => {
			return (
				StatusCode::NOT_FOUND,
				Json(OrgErrorResponse {
					error: "not_found".to_string(),
					message: t(locale, "server.api.org.join_request_not_found").to_string(),
				}),
			)
				.into_response();
		}
		Err(e) => {
			tracing::error!(error = %e, %org_id, %request_id, "Failed to get join request");
			return (
				StatusCode::INTERNAL_SERVER_ERROR,
				Json(OrgErrorResponse {
					error: "internal_error".to_string(),
					message: t(locale, "server.api.error.internal").to_string(),
				}),
			)
				.into_response();
		}
	};

	if let Err(e) = state
		.org_repo
		.approve_join_request(&request_id, &current_user.user.id)
		.await
	{
		tracing::error!(error = %e, %org_id, %request_id, "Failed to approve join request");
		return (
			StatusCode::INTERNAL_SERVER_ERROR,
			Json(OrgErrorResponse {
				error: "internal_error".to_string(),
				message: t(locale, "server.api.error.internal").to_string(),
			}),
		)
			.into_response();
	}

	if let Err(e) = state
		.org_repo
		.add_member(&org_id, &join_request.user_id, OrgRole::Member)
		.await
	{
		tracing::error!(error = %e, %org_id, user_id = %join_request.user_id, "Failed to add member after approval");
		return (
			StatusCode::INTERNAL_SERVER_ERROR,
			Json(OrgErrorResponse {
				error: "internal_error".to_string(),
				message: t(locale, "server.api.error.internal").to_string(),
			}),
		)
			.into_response();
	}

	tracing::info!(%org_id, %request_id, approved_by = %current_user.user.id, "Join request approved");

	(
		StatusCode::OK,
		Json(OrgSuccessResponse {
			message: t(locale, "server.api.org.join_request_approved").to_string(),
		}),
	)
		.into_response()
}

#[utoipa::path(
    post,
    path = "/api/orgs/{id}/join-requests/{request_id}/reject",
    params(
        ("id" = String, Path, description = "Organization ID"),
        ("request_id" = String, Path, description = "Join request ID")
    ),
    responses(
        (status = 200, description = "Join request rejected", body = OrgSuccessResponse),
        (status = 401, description = "Not authenticated", body = OrgErrorResponse),
        (status = 403, description = "Not authorized to reject requests", body = OrgErrorResponse),
        (status = 404, description = "Organization or request not found", body = OrgErrorResponse)
    ),
    tag = "orgs"
)]
/// Reject a join request.
///
/// Rejects a pending join request.
///
/// # Authorization
/// Requires authentication. ABAC check for `Action::ManageOrg`.
/// Owners and admins can reject join requests.
///
/// # Errors
/// - 401: Not authenticated
/// - 403: Not authorized (not owner/admin)
/// - 404: Organization or request not found
/// - 500: Internal server error
#[tracing::instrument(skip(state), fields(%org_id, %request_id))]
pub async fn reject_join_request(
	RequireAuth(current_user): RequireAuth,
	State(state): State<AppState>,
	Path((org_id, request_id)): Path<(String, String)>,
) -> impl IntoResponse {
	let locale = resolve_user_locale(&current_user, &state.default_locale);
	let org_id = parse_id!(
		OrgErrorResponse,
		shared_parse_org_id(&org_id, &t(locale, "server.api.org.invalid_id"))
	);

	let org = match state.org_repo.get_org_by_id(&org_id).await {
		Ok(Some(org)) => org,
		Ok(None) => {
			return (
				StatusCode::NOT_FOUND,
				Json(OrgErrorResponse {
					error: "not_found".to_string(),
					message: t(locale, "server.api.org.not_found").to_string(),
				}),
			)
				.into_response();
		}
		Err(e) => {
			tracing::error!(error = %e, %org_id, "Failed to get organization");
			return (
				StatusCode::INTERNAL_SERVER_ERROR,
				Json(OrgErrorResponse {
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
		return e.into_response();
	}

	match state.org_repo.get_join_request(&request_id).await {
		Ok(Some(req)) if req.is_pending() && req.org_id == org_id => {}
		Ok(Some(_)) | Ok(None) => {
			return (
				StatusCode::NOT_FOUND,
				Json(OrgErrorResponse {
					error: "not_found".to_string(),
					message: t(locale, "server.api.org.join_request_not_found").to_string(),
				}),
			)
				.into_response();
		}
		Err(e) => {
			tracing::error!(error = %e, %org_id, %request_id, "Failed to get join request");
			return (
				StatusCode::INTERNAL_SERVER_ERROR,
				Json(OrgErrorResponse {
					error: "internal_error".to_string(),
					message: t(locale, "server.api.error.internal").to_string(),
				}),
			)
				.into_response();
		}
	};

	if let Err(e) = state
		.org_repo
		.reject_join_request(&request_id, &current_user.user.id)
		.await
	{
		tracing::error!(error = %e, %org_id, %request_id, "Failed to reject join request");
		return (
			StatusCode::INTERNAL_SERVER_ERROR,
			Json(OrgErrorResponse {
				error: "internal_error".to_string(),
				message: t(locale, "server.api.error.internal").to_string(),
			}),
		)
			.into_response();
	}

	tracing::info!(%org_id, %request_id, rejected_by = %current_user.user.id, "Join request rejected");

	(
		StatusCode::OK,
		Json(OrgSuccessResponse {
			message: t(locale, "server.api.org.join_request_rejected").to_string(),
		}),
	)
		.into_response()
}

#[utoipa::path(
    post,
    path = "/api/orgs/{id}/restore",
    params(
        ("id" = String, Path, description = "Organization ID")
    ),
    responses(
        (status = 200, description = "Organization restored", body = OrgSuccessResponse),
        (status = 401, description = "Not authenticated", body = OrgErrorResponse),
        (status = 403, description = "Not authorized to restore", body = OrgErrorResponse),
        (status = 404, description = "Organization not found", body = OrgErrorResponse),
        (status = 410, description = "Grace period expired", body = OrgErrorResponse)
    ),
    tag = "orgs"
)]
/// Restore a soft-deleted organization.
///
/// Restores an organization that was previously deleted, if within the 90-day grace period.
///
/// # Authorization
/// Requires authentication. Only owners can restore.
///
/// # Errors
/// - 401: Not authenticated
/// - 403: Not authorized (not owner)
/// - 404: Organization not found
/// - 410: Grace period expired (>90 days since deletion)
/// - 500: Internal server error
#[tracing::instrument(skip(state), fields(%org_id))]
pub async fn restore_org(
	RequireAuth(current_user): RequireAuth,
	State(state): State<AppState>,
	Path(org_id): Path<String>,
) -> impl IntoResponse {
	let locale = resolve_user_locale(&current_user, &state.default_locale);
	let org_id = parse_id!(
		OrgErrorResponse,
		shared_parse_org_id(&org_id, &t(locale, "server.api.org.invalid_id"))
	);

	let org = match state
		.org_repo
		.get_org_by_id_including_deleted(&org_id)
		.await
	{
		Ok(Some(org)) => org,
		Ok(None) => {
			return (
				StatusCode::NOT_FOUND,
				Json(OrgErrorResponse {
					error: "not_found".to_string(),
					message: t(locale, "server.api.org.not_found").to_string(),
				}),
			)
				.into_response();
		}
		Err(e) => {
			tracing::error!(error = %e, %org_id, "Failed to get organization");
			return (
				StatusCode::INTERNAL_SERVER_ERROR,
				Json(OrgErrorResponse {
					error: "internal_error".to_string(),
					message: t(locale, "server.api.error.internal").to_string(),
				}),
			)
				.into_response();
		}
	};

	let deleted_at = match org.deleted_at {
		Some(dt) => dt,
		None => {
			return (
				StatusCode::BAD_REQUEST,
				Json(OrgErrorResponse {
					error: "not_deleted".to_string(),
					message: t(locale, "server.api.org.not_deleted").to_string(),
				}),
			)
				.into_response();
		}
	};

	let grace_period_days = 90;
	let grace_period_expired = Utc::now() > deleted_at + chrono::Duration::days(grace_period_days);
	if grace_period_expired {
		return (
			StatusCode::GONE,
			Json(OrgErrorResponse {
				error: "grace_period_expired".to_string(),
				message: t(locale, "server.api.org.grace_period_expired").to_string(),
			}),
		)
			.into_response();
	}

	let membership = match state
		.org_repo
		.get_membership(&org_id, &current_user.user.id)
		.await
	{
		Ok(Some(m)) => m,
		Ok(None) => {
			return (
				StatusCode::FORBIDDEN,
				Json(OrgErrorResponse {
					error: "forbidden".to_string(),
					message: t(locale, "server.api.org.not_a_member").to_string(),
				}),
			)
				.into_response();
		}
		Err(e) => {
			tracing::error!(error = %e, %org_id, "Failed to get membership");
			return (
				StatusCode::INTERNAL_SERVER_ERROR,
				Json(OrgErrorResponse {
					error: "internal_error".to_string(),
					message: t(locale, "server.api.error.internal").to_string(),
				}),
			)
				.into_response();
		}
	};

	if membership.role != OrgRole::Owner {
		return (
			StatusCode::FORBIDDEN,
			Json(OrgErrorResponse {
				error: "forbidden".to_string(),
				message: t(locale, "server.api.org.owner_required").to_string(),
			}),
		)
			.into_response();
	}

	if let Err(e) = state.org_repo.restore_org(&org_id).await {
		tracing::error!(error = %e, %org_id, "Failed to restore organization");
		return (
			StatusCode::INTERNAL_SERVER_ERROR,
			Json(OrgErrorResponse {
				error: "internal_error".to_string(),
				message: t(locale, "server.api.error.internal").to_string(),
			}),
		)
			.into_response();
	}

	tracing::info!(%org_id, restored_by = %current_user.user.id, "Organization restored");

	(
		StatusCode::OK,
		Json(OrgSuccessResponse {
			message: t(locale, "server.api.org.restored").to_string(),
		}),
	)
		.into_response()
}

#[utoipa::path(
    patch,
    path = "/api/orgs/{org_id}/members/{user_id}",
    params(
        ("org_id" = String, Path, description = "Organization ID"),
        ("user_id" = String, Path, description = "User ID to update")
    ),
    request_body = UpdateOrgMemberRoleRequest,
    responses(
        (status = 200, description = "Member role updated", body = OrgSuccessResponse),
        (status = 400, description = "Invalid role or cannot demote last owner", body = OrgErrorResponse),
        (status = 401, description = "Not authenticated", body = OrgErrorResponse),
        (status = 403, description = "Not authorized to change roles", body = OrgErrorResponse),
        (status = 404, description = "Organization or member not found", body = OrgErrorResponse)
    ),
    tag = "orgs"
)]
/// Update a member's role.
///
/// Changes a member's role within the organization.
/// Cannot demote the last owner.
///
/// # Authorization
/// Requires authentication. Only owners can change member roles.
///
/// # Errors
/// - 400: Invalid role or would remove last owner
/// - 401: Not authenticated
/// - 403: Not authorized (not owner)
/// - 404: Organization or member not found
/// - 500: Internal server error
#[tracing::instrument(skip(state, payload), fields(%org_id, %user_id))]
pub async fn update_org_member_role(
	RequireAuth(current_user): RequireAuth,
	State(state): State<AppState>,
	Path((org_id, user_id)): Path<(String, String)>,
	Json(payload): Json<UpdateOrgMemberRoleRequest>,
) -> impl IntoResponse {
	let locale = resolve_user_locale(&current_user, &state.default_locale);
	let org_id = parse_id!(
		OrgErrorResponse,
		shared_parse_org_id(&org_id, &t(locale, "server.api.org.invalid_id"))
	);

	let target_user_id = parse_id!(
		OrgErrorResponse,
		shared_parse_user_id(&user_id, &t(locale, "server.api.user.invalid_id"))
	);

	let new_role = parse_role!(
		OrgErrorResponse,
		parse_org_role(&payload.role, &t(locale, "server.api.org.invalid_role"))
	);

	let org = match state.org_repo.get_org_by_id(&org_id).await {
		Ok(Some(org)) => org,
		Ok(None) => {
			return (
				StatusCode::NOT_FOUND,
				Json(OrgErrorResponse {
					error: "not_found".to_string(),
					message: t(locale, "server.api.org.not_found").to_string(),
				}),
			)
				.into_response();
		}
		Err(e) => {
			tracing::error!(error = %e, %org_id, "Failed to get organization");
			return (
				StatusCode::INTERNAL_SERVER_ERROR,
				Json(OrgErrorResponse {
					error: "internal_error".to_string(),
					message: t(locale, "server.api.error.internal").to_string(),
				}),
			)
				.into_response();
		}
	};

	let caller_membership = match state
		.org_repo
		.get_membership(&org_id, &current_user.user.id)
		.await
	{
		Ok(Some(m)) => m,
		Ok(None) => {
			return (
				StatusCode::FORBIDDEN,
				Json(OrgErrorResponse {
					error: "forbidden".to_string(),
					message: t(locale, "server.api.org.not_a_member").to_string(),
				}),
			)
				.into_response();
		}
		Err(e) => {
			tracing::error!(error = %e, %org_id, "Failed to get caller membership");
			return (
				StatusCode::INTERNAL_SERVER_ERROR,
				Json(OrgErrorResponse {
					error: "internal_error".to_string(),
					message: t(locale, "server.api.error.internal").to_string(),
				}),
			)
				.into_response();
		}
	};

	if caller_membership.role != OrgRole::Owner {
		return (
			StatusCode::FORBIDDEN,
			Json(OrgErrorResponse {
				error: "forbidden".to_string(),
				message: t(locale, "server.api.org.owner_required").to_string(),
			}),
		)
			.into_response();
	}

	let target_membership = match state
		.org_repo
		.get_membership(&org_id, &target_user_id)
		.await
	{
		Ok(Some(m)) => m,
		Ok(None) => {
			return (
				StatusCode::NOT_FOUND,
				Json(OrgErrorResponse {
					error: "not_found".to_string(),
					message: t(locale, "server.api.org.member_not_found").to_string(),
				}),
			)
				.into_response();
		}
		Err(e) => {
			tracing::error!(error = %e, %org_id, %target_user_id, "Failed to get target membership");
			return (
				StatusCode::INTERNAL_SERVER_ERROR,
				Json(OrgErrorResponse {
					error: "internal_error".to_string(),
					message: t(locale, "server.api.error.internal").to_string(),
				}),
			)
				.into_response();
		}
	};

	if target_membership.role == OrgRole::Owner && new_role != OrgRole::Owner {
		let owner_count = match state.org_repo.count_owners(&org_id).await {
			Ok(count) => count,
			Err(e) => {
				tracing::error!(error = %e, %org_id, "Failed to count owners");
				return (
					StatusCode::INTERNAL_SERVER_ERROR,
					Json(OrgErrorResponse {
						error: "internal_error".to_string(),
						message: t(locale, "server.api.error.internal").to_string(),
					}),
				)
					.into_response();
			}
		};

		if owner_count <= 1 {
			return (
				StatusCode::BAD_REQUEST,
				Json(OrgErrorResponse {
					error: "last_owner".to_string(),
					message: t(locale, "server.api.org.last_owner").to_string(),
				}),
			)
				.into_response();
		}
	}

	if let Err(e) = state
		.org_repo
		.update_member_role(&org_id, &target_user_id, new_role)
		.await
	{
		tracing::error!(error = %e, %org_id, %target_user_id, "Failed to update member role");
		return (
			StatusCode::INTERNAL_SERVER_ERROR,
			Json(OrgErrorResponse {
				error: "internal_error".to_string(),
				message: t(locale, "server.api.error.internal").to_string(),
			}),
		)
			.into_response();
	}

	tracing::info!(
		%org_id,
		%target_user_id,
		old_role = %target_membership.role,
		%new_role,
		updated_by = %current_user.user.id,
		"Member role updated"
	);

	state.audit_service.log(
		AuditLogBuilder::new(AuditEventType::RoleChanged)
			.actor(AuditUserId::new(current_user.user.id.into_inner()))
			.resource("org", org_id.to_string())
			.details(serde_json::json!({
				"target_user_id": target_user_id.to_string(),
				"old_role": format!("{:?}", target_membership.role),
				"new_role": format!("{:?}", new_role),
			}))
			.build(),
	);

	let _ = org;

	(
		StatusCode::OK,
		Json(OrgSuccessResponse {
			message: t(locale, "server.api.org.role_updated").to_string(),
		}),
	)
		.into_response()
}
