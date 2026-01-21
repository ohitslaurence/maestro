// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights
// reserved. SPDX-License-Identifier: Proprietary

//! Routes for repository maintenance operations.
//!
//! Provides endpoints for:
//! - Triggering maintenance for a specific repository
//! - Triggering global maintenance sweep (admin only)
//! - Listing maintenance jobs for a repository

use axum::{
	extract::{Path, Query, State},
	http::StatusCode,
	response::IntoResponse,
	Json,
};
use uuid::Uuid;

pub use loom_server_api::maintenance::*;

use crate::{
	api::AppState,
	auth_middleware::RequireAuth,
	i18n::{resolve_user_locale, t, t_fmt},
	routes::admin::AdminErrorResponse,
};
use loom_server_auth::types::{OrgId, OrgRole};
use loom_server_scm::{
	MaintenanceJob, MaintenanceJobStore, MaintenanceTask, OwnerType, RepoStore, Repository,
	Visibility,
};

async fn check_repo_admin(
	repo: &Repository,
	current_user: &loom_server_auth::middleware::CurrentUser,
	state: &AppState,
	locale: &str,
) -> Result<(), (StatusCode, Json<MaintenanceErrorResponse>)> {
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
			Json(MaintenanceErrorResponse {
				error: "forbidden".to_string(),
				message: t(locale, "server.api.scm.admin_required").to_string(),
			}),
		));
	}

	Ok(())
}

async fn check_repo_access(
	repo: &Repository,
	current_user: &loom_server_auth::middleware::CurrentUser,
	state: &AppState,
	locale: &str,
) -> Result<(), (StatusCode, Json<MaintenanceErrorResponse>)> {
	if repo.visibility == Visibility::Public {
		return Ok(());
	}

	let has_access = match repo.owner_type {
		OwnerType::User => repo.owner_id == current_user.user.id.into_inner(),
		OwnerType::Org => {
			let org_id = OrgId::new(repo.owner_id);
			matches!(
				state
					.org_repo
					.get_membership(&org_id, &current_user.user.id)
					.await,
				Ok(Some(_))
			)
		}
	};

	if !has_access {
		return Err((
			StatusCode::FORBIDDEN,
			Json(MaintenanceErrorResponse {
				error: "forbidden".to_string(),
				message: t(locale, "server.api.scm.access_denied").to_string(),
			}),
		));
	}

	Ok(())
}

/// Trigger maintenance for a repository.
///
/// # Authorization
///
/// Requires `repo:admin` role on the repository.
#[utoipa::path(
	post,
	path = "/api/v1/repos/{id}/maintenance",
	params(("id" = String, Path, description = "Repository ID")),
	request_body = TriggerMaintenanceRequest,
	responses(
		(status = 202, description = "Maintenance job queued", body = TriggerMaintenanceResponse),
		(status = 400, description = "Invalid request", body = MaintenanceErrorResponse),
		(status = 401, description = "Not authenticated", body = MaintenanceErrorResponse),
		(status = 403, description = "Not authorized", body = MaintenanceErrorResponse),
		(status = 404, description = "Repository not found", body = MaintenanceErrorResponse)
	),
	tag = "repos-maintenance"
)]
#[tracing::instrument(skip(state), fields(actor_id = %current_user.user.id, repo_id = %repo_id))]
pub async fn trigger_repo_maintenance(
	RequireAuth(current_user): RequireAuth,
	State(state): State<AppState>,
	Path(repo_id): Path<Uuid>,
	Json(request): Json<TriggerMaintenanceRequest>,
) -> impl IntoResponse {
	let locale = resolve_user_locale(&current_user, &state.default_locale);

	let maintenance_store = match &state.scm_maintenance_store {
		Some(s) => s,
		None => {
			return (
				StatusCode::NOT_IMPLEMENTED,
				Json(MaintenanceErrorResponse {
					error: "not_implemented".to_string(),
					message: t(locale, "server.api.scm.maintenance.not_configured").to_string(),
				}),
			)
				.into_response();
		}
	};

	let repo_store = match &state.scm_repo_store {
		Some(s) => s,
		None => {
			return (
				StatusCode::NOT_IMPLEMENTED,
				Json(MaintenanceErrorResponse {
					error: "not_implemented".to_string(),
					message: t(locale, "server.api.error.not_configured").to_string(),
				}),
			)
				.into_response();
		}
	};

	let repo = match repo_store.get_by_id(repo_id).await {
		Ok(Some(r)) => r,
		Ok(None) => {
			return (
				StatusCode::NOT_FOUND,
				Json(MaintenanceErrorResponse {
					error: "not_found".to_string(),
					message: t(locale, "server.api.scm.repo.not_found").to_string(),
				}),
			)
				.into_response();
		}
		Err(e) => {
			tracing::error!(error = %e, "Failed to get repository");
			return (
				StatusCode::INTERNAL_SERVER_ERROR,
				Json(MaintenanceErrorResponse {
					error: "internal_error".to_string(),
					message: t(locale, "server.api.error.internal").to_string(),
				}),
			)
				.into_response();
		}
	};

	if let Err(e) = check_repo_admin(&repo, &current_user, &state, locale).await {
		return e.into_response();
	}

	let task: MaintenanceTask = request.task.into();
	let job = MaintenanceJob::new(Some(repo_id), task);

	match maintenance_store.create(&job).await {
		Ok(created_job) => {
			tracing::info!(
				actor_id = %current_user.user.id,
				repo_id = %repo_id,
				job_id = %created_job.id,
				task = ?task,
				"Maintenance job created"
			);

			(
				StatusCode::ACCEPTED,
				Json(TriggerMaintenanceResponse {
					job_id: created_job.id.to_string(),
					message: t_fmt(
						locale,
						"server.api.scm.maintenance.job_queued",
						&[("task", task.as_str())],
					),
				}),
			)
				.into_response()
		}
		Err(e) => {
			tracing::error!(error = %e, "Failed to create maintenance job");
			(
				StatusCode::INTERNAL_SERVER_ERROR,
				Json(MaintenanceErrorResponse {
					error: "internal_error".to_string(),
					message: t(locale, "server.api.error.internal").to_string(),
				}),
			)
				.into_response()
		}
	}
}

/// Trigger global maintenance sweep.
///
/// # Authorization
///
/// Requires `system_admin` role.
#[utoipa::path(
	post,
	path = "/api/v1/admin/maintenance/sweep",
	request_body = TriggerGlobalSweepRequest,
	responses(
		(status = 202, description = "Global sweep queued", body = TriggerMaintenanceResponse),
		(status = 401, description = "Not authenticated", body = AdminErrorResponse),
		(status = 403, description = "Not authorized", body = AdminErrorResponse)
	),
	tag = "admin-maintenance"
)]
#[tracing::instrument(skip(state), fields(actor_id = %current_user.user.id))]
pub async fn trigger_global_sweep(
	RequireAuth(current_user): RequireAuth,
	State(state): State<AppState>,
	Json(request): Json<TriggerGlobalSweepRequest>,
) -> impl IntoResponse {
	let locale = resolve_user_locale(&current_user, &state.default_locale);

	if !current_user.user.is_system_admin {
		tracing::warn!(actor_id = %current_user.user.id, "Unauthorized global sweep attempt");
		return (
			StatusCode::FORBIDDEN,
			Json(AdminErrorResponse {
				error: "forbidden".to_string(),
				message: t(locale, "server.api.admin.system_admin_required").to_string(),
			}),
		)
			.into_response();
	}

	let maintenance_store = match &state.scm_maintenance_store {
		Some(s) => s,
		None => {
			return (
				StatusCode::NOT_IMPLEMENTED,
				Json(AdminErrorResponse {
					error: "not_implemented".to_string(),
					message: t(locale, "server.api.scm.maintenance.not_configured").to_string(),
				}),
			)
				.into_response();
		}
	};

	let task: MaintenanceTask = request.task.into();
	let job = MaintenanceJob::new(None, task);

	match maintenance_store.create(&job).await {
		Ok(created_job) => {
			tracing::info!(
				actor_id = %current_user.user.id,
				job_id = %created_job.id,
				task = ?task,
				stagger_ms = request.stagger_ms,
				"Global maintenance sweep queued"
			);

			(
				StatusCode::ACCEPTED,
				Json(TriggerMaintenanceResponse {
					job_id: created_job.id.to_string(),
					message: t_fmt(
						locale,
						"server.api.scm.maintenance.sweep_queued",
						&[("task", task.as_str())],
					),
				}),
			)
				.into_response()
		}
		Err(e) => {
			tracing::error!(error = %e, "Failed to create global maintenance job");
			(
				StatusCode::INTERNAL_SERVER_ERROR,
				Json(AdminErrorResponse {
					error: "internal_error".to_string(),
					message: t(locale, "server.api.error.internal").to_string(),
				}),
			)
				.into_response()
		}
	}
}

/// List maintenance jobs for a repository.
#[utoipa::path(
	get,
	path = "/api/v1/repos/{id}/maintenance/jobs",
	params(
		("id" = String, Path, description = "Repository ID"),
		ListMaintenanceJobsQuery
	),
	responses(
		(status = 200, description = "List of maintenance jobs", body = ListMaintenanceJobsResponse),
		(status = 401, description = "Not authenticated", body = MaintenanceErrorResponse),
		(status = 403, description = "Not authorized", body = MaintenanceErrorResponse),
		(status = 404, description = "Repository not found", body = MaintenanceErrorResponse)
	),
	tag = "repos-maintenance"
)]
#[tracing::instrument(skip(state), fields(actor_id = %current_user.user.id, repo_id = %repo_id))]
pub async fn list_repo_maintenance_jobs(
	RequireAuth(current_user): RequireAuth,
	State(state): State<AppState>,
	Path(repo_id): Path<Uuid>,
	Query(query): Query<ListMaintenanceJobsQuery>,
) -> impl IntoResponse {
	let locale = resolve_user_locale(&current_user, &state.default_locale);

	let maintenance_store = match &state.scm_maintenance_store {
		Some(s) => s,
		None => {
			return (
				StatusCode::NOT_IMPLEMENTED,
				Json(MaintenanceErrorResponse {
					error: "not_implemented".to_string(),
					message: t(locale, "server.api.scm.maintenance.not_configured").to_string(),
				}),
			)
				.into_response();
		}
	};

	let repo_store = match &state.scm_repo_store {
		Some(s) => s,
		None => {
			return (
				StatusCode::NOT_IMPLEMENTED,
				Json(MaintenanceErrorResponse {
					error: "not_implemented".to_string(),
					message: t(locale, "server.api.error.not_configured").to_string(),
				}),
			)
				.into_response();
		}
	};

	let repo = match repo_store.get_by_id(repo_id).await {
		Ok(Some(r)) => r,
		Ok(None) => {
			return (
				StatusCode::NOT_FOUND,
				Json(MaintenanceErrorResponse {
					error: "not_found".to_string(),
					message: t(locale, "server.api.scm.repo.not_found").to_string(),
				}),
			)
				.into_response();
		}
		Err(e) => {
			tracing::error!(error = %e, "Failed to get repository");
			return (
				StatusCode::INTERNAL_SERVER_ERROR,
				Json(MaintenanceErrorResponse {
					error: "internal_error".to_string(),
					message: t(locale, "server.api.error.internal").to_string(),
				}),
			)
				.into_response();
		}
	};

	if let Err(e) = check_repo_access(&repo, &current_user, &state, locale).await {
		return e.into_response();
	}

	match maintenance_store.list_by_repo(repo_id, query.limit).await {
		Ok(jobs) => {
			let job_responses: Vec<MaintenanceJobResponse> =
				jobs.into_iter().map(MaintenanceJobResponse::from).collect();

			tracing::info!(
				actor_id = %current_user.user.id,
				repo_id = %repo_id,
				job_count = job_responses.len(),
				"Listed maintenance jobs"
			);

			(
				StatusCode::OK,
				Json(ListMaintenanceJobsResponse {
					jobs: job_responses,
				}),
			)
				.into_response()
		}
		Err(e) => {
			tracing::error!(error = %e, "Failed to list maintenance jobs");
			(
				StatusCode::INTERNAL_SERVER_ERROR,
				Json(MaintenanceErrorResponse {
					error: "internal_error".to_string(),
					message: t(locale, "server.api.error.internal").to_string(),
				}),
			)
				.into_response()
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use loom_server_scm::{MaintenanceJobStatus, Repository};
	use uuid::Uuid;

	fn make_user_repo(owner_id: Uuid) -> Repository {
		Repository {
			id: Uuid::new_v4(),
			owner_type: OwnerType::User,
			owner_id,
			name: "test-repo".to_string(),
			visibility: Visibility::Private,
			default_branch: "main".to_string(),
			created_at: chrono::Utc::now(),
			updated_at: chrono::Utc::now(),
			deleted_at: None,
		}
	}

	fn make_org_repo(org_id: Uuid) -> Repository {
		Repository {
			id: Uuid::new_v4(),
			owner_type: OwnerType::Org,
			owner_id: org_id,
			name: "org-repo".to_string(),
			visibility: Visibility::Private,
			default_branch: "main".to_string(),
			created_at: chrono::Utc::now(),
			updated_at: chrono::Utc::now(),
			deleted_at: None,
		}
	}

	fn make_public_repo() -> Repository {
		Repository {
			id: Uuid::new_v4(),
			owner_type: OwnerType::User,
			owner_id: Uuid::new_v4(),
			name: "public-repo".to_string(),
			visibility: Visibility::Public,
			default_branch: "main".to_string(),
			created_at: chrono::Utc::now(),
			updated_at: chrono::Utc::now(),
			deleted_at: None,
		}
	}

	#[test]
	fn test_user_is_admin_of_own_repo() {
		let user_id = Uuid::new_v4();
		let repo = make_user_repo(user_id);
		let is_owner = repo.owner_type == OwnerType::User && repo.owner_id == user_id;
		assert!(is_owner, "User should be admin of their own repo");
	}

	#[test]
	fn test_user_is_not_admin_of_other_user_repo() {
		let owner_id = Uuid::new_v4();
		let other_user_id = Uuid::new_v4();
		let repo = make_user_repo(owner_id);
		let is_owner = repo.owner_type == OwnerType::User && repo.owner_id == other_user_id;
		assert!(!is_owner, "User should not be admin of another user's repo");
	}

	#[test]
	fn test_public_repo_allows_access() {
		let repo = make_public_repo();
		assert_eq!(
			repo.visibility,
			Visibility::Public,
			"Public repo should be accessible"
		);
	}

	#[test]
	fn test_private_repo_requires_membership() {
		let org_id = Uuid::new_v4();
		let repo = make_org_repo(org_id);
		assert_eq!(
			repo.visibility,
			Visibility::Private,
			"Private repo requires membership check"
		);
		assert_eq!(repo.owner_type, OwnerType::Org, "Org repo ownership check");
	}

	#[test]
	fn test_maintenance_task_conversion() {
		assert!(matches!(MaintenanceTaskApi::Gc.into(), MaintenanceTask::Gc));
		assert!(matches!(
			MaintenanceTaskApi::Prune.into(),
			MaintenanceTask::Prune
		));
		assert!(matches!(
			MaintenanceTaskApi::Repack.into(),
			MaintenanceTask::Repack
		));
		assert!(matches!(
			MaintenanceTaskApi::Fsck.into(),
			MaintenanceTask::Fsck
		));
		assert!(matches!(
			MaintenanceTaskApi::All.into(),
			MaintenanceTask::All
		));
	}

	#[test]
	fn test_maintenance_status_conversion() {
		assert!(matches!(
			MaintenanceJobStatus::Pending.into(),
			MaintenanceJobStatusApi::Pending
		));
		assert!(matches!(
			MaintenanceJobStatus::Running.into(),
			MaintenanceJobStatusApi::Running
		));
		assert!(matches!(
			MaintenanceJobStatus::Success.into(),
			MaintenanceJobStatusApi::Success
		));
		assert!(matches!(
			MaintenanceJobStatus::Failed.into(),
			MaintenanceJobStatusApi::Failed
		));
	}
}
