// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights
// reserved. SPDX-License-Identifier: Proprietary

//! Branch protection rule HTTP handlers.
//!
//! Implements branch protection endpoints per the scm-system.md specification:
//! - List protection rules for a repository
//! - Create protection rule
//! - Delete protection rule
//!
//! Only repo admins can manage protection rules.

use axum::{
	extract::{Path, State},
	http::StatusCode,
	response::IntoResponse,
	Json,
};
use loom_server_audit::{AuditEventType, AuditLogBuilder, UserId as AuditUserId};
use loom_server_auth::types::{OrgId, OrgRole};
use loom_server_scm::{BranchProtectionRuleRecord, OwnerType, ProtectionStore, RepoStore};
use uuid::Uuid;

pub use loom_server_api::protection::*;
pub use loom_server_api::repos::RepoErrorResponse;

use crate::{
	api::AppState,
	auth_middleware::RequireAuth,
	i18n::{resolve_user_locale, t},
};

async fn check_repo_admin(
	repo_id: Uuid,
	current_user: &loom_server_auth::middleware::CurrentUser,
	state: &AppState,
	locale: &str,
) -> Result<(), (StatusCode, Json<RepoErrorResponse>)> {
	let scm_store = state.scm_repo_store.as_ref().ok_or_else(|| {
		(
			StatusCode::INTERNAL_SERVER_ERROR,
			Json(RepoErrorResponse {
				error: "not_configured".to_string(),
				message: t(locale, "server.api.scm.not_configured").to_string(),
			}),
		)
	})?;

	let repo = scm_store.get_by_id(repo_id).await.map_err(|e| {
		tracing::error!(error = %e, "Failed to get repository");
		(
			StatusCode::INTERNAL_SERVER_ERROR,
			Json(RepoErrorResponse {
				error: "internal_error".to_string(),
				message: t(locale, "server.api.scm.internal_error").to_string(),
			}),
		)
	})?;

	let repo = repo.ok_or_else(|| {
		(
			StatusCode::NOT_FOUND,
			Json(RepoErrorResponse {
				error: "not_found".to_string(),
				message: t(locale, "server.api.scm.repo_not_found").to_string(),
			}),
		)
	})?;

	let is_admin = match repo.owner_type {
		OwnerType::User => repo.owner_id == current_user.user.id.into_inner(),
		OwnerType::Org => {
			let org_id = OrgId::new(repo.owner_id);
			match state
				.org_repo
				.get_membership(&org_id, &current_user.user.id)
				.await
			{
				Ok(Some(m)) => m.role == OrgRole::Owner || m.role == OrgRole::Admin,
				_ => false,
			}
		}
	};

	if !is_admin {
		return Err((
			StatusCode::FORBIDDEN,
			Json(RepoErrorResponse {
				error: "forbidden".to_string(),
				message: t(locale, "server.api.scm.admin_required").to_string(),
			}),
		));
	}

	Ok(())
}

#[utoipa::path(
	get,
	path = "/api/v1/repos/{id}/protection",
	params(
		("id" = Uuid, Path, description = "Repository ID")
	),
	responses(
		(status = 200, description = "List of protection rules", body = ListProtectionRulesResponse),
		(status = 401, description = "Not authenticated", body = RepoErrorResponse),
		(status = 403, description = "Not authorized", body = RepoErrorResponse),
		(status = 404, description = "Repository not found", body = RepoErrorResponse)
	),
	tag = "repos"
)]
#[tracing::instrument(skip(state), fields(repo_id = %id))]
pub async fn list_protection_rules(
	RequireAuth(current_user): RequireAuth,
	State(state): State<AppState>,
	Path(id): Path<Uuid>,
) -> impl IntoResponse {
	let locale = resolve_user_locale(&current_user, &state.default_locale);

	if let Err(e) = check_repo_admin(id, &current_user, &state, locale).await {
		return e.into_response();
	}

	let protection_store = match state.scm_protection_store.as_ref() {
		Some(store) => store,
		None => {
			return (
				StatusCode::INTERNAL_SERVER_ERROR,
				Json(RepoErrorResponse {
					error: "not_configured".to_string(),
					message: t(locale, "server.api.scm.not_configured").to_string(),
				}),
			)
				.into_response();
		}
	};

	match protection_store.list_by_repo(id).await {
		Ok(rules) => {
			let response = ListProtectionRulesResponse {
				rules: rules.into_iter().map(Into::into).collect(),
			};
			(StatusCode::OK, Json(response)).into_response()
		}
		Err(e) => {
			tracing::error!(error = %e, "Failed to list protection rules");
			(
				StatusCode::INTERNAL_SERVER_ERROR,
				Json(RepoErrorResponse {
					error: "internal_error".to_string(),
					message: t(locale, "server.api.scm.protection.failed_to_list").to_string(),
				}),
			)
				.into_response()
		}
	}
}

#[utoipa::path(
	post,
	path = "/api/v1/repos/{id}/protection",
	params(
		("id" = Uuid, Path, description = "Repository ID")
	),
	request_body = CreateProtectionRuleRequest,
	responses(
		(status = 201, description = "Protection rule created", body = ProtectionRuleResponse),
		(status = 400, description = "Invalid request", body = RepoErrorResponse),
		(status = 401, description = "Not authenticated", body = RepoErrorResponse),
		(status = 403, description = "Not authorized", body = RepoErrorResponse),
		(status = 404, description = "Repository not found", body = RepoErrorResponse),
		(status = 409, description = "Rule already exists", body = RepoErrorResponse)
	),
	tag = "repos"
)]
#[tracing::instrument(skip(state, payload), fields(repo_id = %id))]
pub async fn create_protection_rule(
	RequireAuth(current_user): RequireAuth,
	State(state): State<AppState>,
	Path(id): Path<Uuid>,
	Json(payload): Json<CreateProtectionRuleRequest>,
) -> impl IntoResponse {
	let locale = resolve_user_locale(&current_user, &state.default_locale);

	if let Err(e) = check_repo_admin(id, &current_user, &state, locale).await {
		return e.into_response();
	}

	if payload.pattern.is_empty() || payload.pattern.len() > 256 {
		return (
			StatusCode::BAD_REQUEST,
			Json(RepoErrorResponse {
				error: "invalid_pattern".to_string(),
				message: t(locale, "server.api.scm.protection.invalid_pattern").to_string(),
			}),
		)
			.into_response();
	}

	let protection_store = match state.scm_protection_store.as_ref() {
		Some(store) => store,
		None => {
			return (
				StatusCode::INTERNAL_SERVER_ERROR,
				Json(RepoErrorResponse {
					error: "not_configured".to_string(),
					message: t(locale, "server.api.scm.not_configured").to_string(),
				}),
			)
				.into_response();
		}
	};

	let rule = BranchProtectionRuleRecord {
		id: Uuid::new_v4(),
		repo_id: id,
		pattern: payload.pattern.clone(),
		block_direct_push: payload.block_direct_push,
		block_force_push: payload.block_force_push,
		block_deletion: payload.block_deletion,
		created_at: chrono::Utc::now(),
	};

	match protection_store.create(&rule).await {
		Ok(created) => {
			state.audit_service.log(
				AuditLogBuilder::new(AuditEventType::AccessGranted)
					.actor(AuditUserId::new(current_user.user.id.into_inner()))
					.resource("protection_rule", created.id.to_string())
					.details(serde_json::json!({
						"action": "protection_rule_created",
						"repo_id": id.to_string(),
						"pattern": &created.pattern,
					}))
					.build(),
			);

			tracing::info!(
				repo_id = %id,
				rule_id = %created.id,
				pattern = %created.pattern,
				created_by = %current_user.user.id,
				"Branch protection rule created"
			);
			(
				StatusCode::CREATED,
				Json(ProtectionRuleResponse::from(created)),
			)
				.into_response()
		}
		Err(loom_server_db::DbError::Conflict(_)) => (
			StatusCode::CONFLICT,
			Json(RepoErrorResponse {
				error: "already_exists".to_string(),
				message: t(locale, "server.api.scm.protection.already_exists").to_string(),
			}),
		)
			.into_response(),
		Err(e) => {
			tracing::error!(error = %e, "Failed to create protection rule");
			(
				StatusCode::INTERNAL_SERVER_ERROR,
				Json(RepoErrorResponse {
					error: "internal_error".to_string(),
					message: t(locale, "server.api.scm.protection.failed_to_create").to_string(),
				}),
			)
				.into_response()
		}
	}
}

#[utoipa::path(
	delete,
	path = "/api/v1/repos/{id}/protection/{rule_id}",
	params(
		("id" = Uuid, Path, description = "Repository ID"),
		("rule_id" = Uuid, Path, description = "Protection rule ID")
	),
	responses(
		(status = 204, description = "Protection rule deleted"),
		(status = 401, description = "Not authenticated", body = RepoErrorResponse),
		(status = 403, description = "Not authorized", body = RepoErrorResponse),
		(status = 404, description = "Repository or rule not found", body = RepoErrorResponse)
	),
	tag = "repos"
)]
#[tracing::instrument(skip(state), fields(repo_id = %id, rule_id = %rule_id))]
pub async fn delete_protection_rule(
	RequireAuth(current_user): RequireAuth,
	State(state): State<AppState>,
	Path((id, rule_id)): Path<(Uuid, Uuid)>,
) -> impl IntoResponse {
	let locale = resolve_user_locale(&current_user, &state.default_locale);

	if let Err(e) = check_repo_admin(id, &current_user, &state, locale).await {
		return e.into_response();
	}

	let protection_store = match state.scm_protection_store.as_ref() {
		Some(store) => store,
		None => {
			return (
				StatusCode::INTERNAL_SERVER_ERROR,
				Json(RepoErrorResponse {
					error: "not_configured".to_string(),
					message: t(locale, "server.api.scm.not_configured").to_string(),
				}),
			)
				.into_response();
		}
	};

	let rule = match protection_store.get_by_id(rule_id).await {
		Ok(Some(r)) => r,
		Ok(None) => {
			return (
				StatusCode::NOT_FOUND,
				Json(RepoErrorResponse {
					error: "not_found".to_string(),
					message: t(locale, "server.api.scm.protection.rule_not_found").to_string(),
				}),
			)
				.into_response();
		}
		Err(e) => {
			tracing::error!(error = %e, "Failed to get protection rule");
			return (
				StatusCode::INTERNAL_SERVER_ERROR,
				Json(RepoErrorResponse {
					error: "internal_error".to_string(),
					message: t(locale, "server.api.scm.internal_error").to_string(),
				}),
			)
				.into_response();
		}
	};

	if rule.repo_id != id {
		return (
			StatusCode::NOT_FOUND,
			Json(RepoErrorResponse {
				error: "not_found".to_string(),
				message: t(locale, "server.api.scm.protection.rule_not_found").to_string(),
			}),
		)
			.into_response();
	}

	match protection_store.delete(rule_id).await {
		Ok(()) => {
			state.audit_service.log(
				AuditLogBuilder::new(AuditEventType::AccessDenied)
					.actor(AuditUserId::new(current_user.user.id.into_inner()))
					.resource("protection_rule", rule_id.to_string())
					.details(serde_json::json!({
						"action": "protection_rule_deleted",
						"repo_id": id.to_string(),
						"pattern": &rule.pattern,
					}))
					.build(),
			);

			tracing::info!(
				repo_id = %id,
				rule_id = %rule_id,
				deleted_by = %current_user.user.id,
				"Branch protection rule deleted"
			);
			StatusCode::NO_CONTENT.into_response()
		}
		Err(loom_server_db::DbError::NotFound(_)) => (
			StatusCode::NOT_FOUND,
			Json(RepoErrorResponse {
				error: "not_found".to_string(),
				message: t(locale, "server.api.scm.protection.rule_not_found").to_string(),
			}),
		)
			.into_response(),
		Err(e) => {
			tracing::error!(error = %e, "Failed to delete protection rule");
			(
				StatusCode::INTERNAL_SERVER_ERROR,
				Json(RepoErrorResponse {
					error: "internal_error".to_string(),
					message: t(locale, "server.api.scm.protection.failed_to_delete").to_string(),
				}),
			)
				.into_response()
		}
	}
}
