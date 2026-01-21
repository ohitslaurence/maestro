// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights
// reserved. SPDX-License-Identifier: Proprietary

//! Admin routes for job scheduler management.
//!
//! Provides endpoints for managing background jobs:
//! - List all registered jobs with status
//! - Trigger manual job execution
//! - Cancel running jobs
//! - View job history
//! - Enable/disable jobs
//!
//! # Security
//!
//! All endpoints require `system_admin` role.

use axum::{
	extract::{Path, Query, State},
	http::StatusCode,
	response::IntoResponse,
	Json,
};
use loom_server_jobs::TriggerSource;

pub use loom_server_api::jobs::*;

use crate::{
	api::AppState,
	auth_middleware::RequireAuth,
	i18n::{resolve_user_locale, t},
	routes::admin::AdminErrorResponse,
};

/// List all registered jobs with their status.
///
/// # Authorization
///
/// Requires `system_admin` role.
///
/// # Response
///
/// Returns [`ListJobsResponse`] with all registered jobs and their current status.
#[utoipa::path(
	get,
	path = "/api/admin/jobs",
	responses(
		(status = 200, description = "List of jobs", body = ListJobsResponse),
		(status = 401, description = "Not authenticated", body = AdminErrorResponse),
		(status = 403, description = "Not authorized", body = AdminErrorResponse),
		(status = 501, description = "Job scheduler not configured", body = AdminErrorResponse)
	),
	tag = "admin-jobs"
)]
#[tracing::instrument(skip(state), fields(actor_id = %current_user.user.id))]
pub async fn list_jobs(
	RequireAuth(current_user): RequireAuth,
	State(state): State<AppState>,
) -> impl IntoResponse {
	let locale = resolve_user_locale(&current_user, &state.default_locale);

	if !current_user.user.is_system_admin {
		tracing::warn!(actor_id = %current_user.user.id, "Unauthorized job list attempt");
		return (
			StatusCode::FORBIDDEN,
			Json(AdminErrorResponse {
				error: "forbidden".to_string(),
				message: t(locale, "server.api.admin.system_admin_required").to_string(),
			}),
		)
			.into_response();
	}

	let scheduler = match &state.job_scheduler {
		Some(s) => s,
		None => {
			return (
				StatusCode::NOT_IMPLEMENTED,
				Json(AdminErrorResponse {
					error: "not_implemented".to_string(),
					message: "Job scheduler not configured".to_string(),
				}),
			)
				.into_response();
		}
	};

	let health = scheduler.health_status().await;

	let definitions = match state.job_repository.as_ref() {
		Some(repo) => repo.list_definitions().await.unwrap_or_default(),
		None => vec![],
	};

	let jobs: Vec<JobInfo> = health
		.jobs
		.into_iter()
		.map(|j| {
			let def = definitions.iter().find(|d| d.id == j.job_id);
			JobInfo {
				id: j.job_id.clone(),
				name: j.name,
				description: def.map(|d| d.description.clone()).unwrap_or_default(),
				job_type: def
					.map(|d| d.job_type.clone())
					.unwrap_or_else(|| "periodic".to_string()),
				interval_secs: def.and_then(|d| d.interval_secs),
				enabled: def.map(|d| d.enabled).unwrap_or(true),
				status: j.status.into(),
				last_run: j.last_run.map(|r| LastRunInfo {
					run_id: r.run_id,
					status: r.status.as_str().to_string(),
					started_at: r.started_at.to_rfc3339(),
					completed_at: None,
					duration_ms: r.duration_ms,
					error: r.error,
				}),
				consecutive_failures: j.consecutive_failures,
			}
		})
		.collect();

	tracing::info!(
		actor_id = %current_user.user.id,
		job_count = jobs.len(),
		"Listed jobs"
	);

	(StatusCode::OK, Json(ListJobsResponse { jobs })).into_response()
}

/// Trigger immediate execution of a job.
///
/// # Authorization
///
/// Requires `system_admin` role.
///
/// # Response
///
/// Returns [`TriggerJobResponse`] with the run ID of the triggered execution.
#[utoipa::path(
	post,
	path = "/api/admin/jobs/{job_id}/run",
	params(("job_id" = String, Path, description = "Job ID")),
	responses(
		(status = 200, description = "Job triggered", body = TriggerJobResponse),
		(status = 401, description = "Not authenticated", body = AdminErrorResponse),
		(status = 403, description = "Not authorized", body = AdminErrorResponse),
		(status = 404, description = "Job not found", body = AdminErrorResponse),
		(status = 501, description = "Job scheduler not configured", body = AdminErrorResponse)
	),
	tag = "admin-jobs"
)]
#[tracing::instrument(skip(state), fields(actor_id = %current_user.user.id, job_id = %job_id))]
pub async fn trigger_job(
	RequireAuth(current_user): RequireAuth,
	State(state): State<AppState>,
	Path(job_id): Path<String>,
) -> impl IntoResponse {
	let locale = resolve_user_locale(&current_user, &state.default_locale);

	if !current_user.user.is_system_admin {
		tracing::warn!(actor_id = %current_user.user.id, "Unauthorized job trigger attempt");
		return (
			StatusCode::FORBIDDEN,
			Json(AdminErrorResponse {
				error: "forbidden".to_string(),
				message: t(locale, "server.api.admin.system_admin_required").to_string(),
			}),
		)
			.into_response();
	}

	let scheduler = match &state.job_scheduler {
		Some(s) => s,
		None => {
			return (
				StatusCode::NOT_IMPLEMENTED,
				Json(AdminErrorResponse {
					error: "not_implemented".to_string(),
					message: "Job scheduler not configured".to_string(),
				}),
			)
				.into_response();
		}
	};

	match scheduler.trigger_job(&job_id, TriggerSource::Manual).await {
		Ok(run_id) => {
			tracing::info!(
				actor_id = %current_user.user.id,
				job_id = %job_id,
				run_id = %run_id,
				"Manually triggered job"
			);
			(
				StatusCode::OK,
				Json(TriggerJobResponse {
					run_id,
					message: format!("Job {} triggered", job_id),
				}),
			)
				.into_response()
		}
		Err(e) => {
			let error_msg = e.to_string();
			if error_msg.contains("not found") {
				(
					StatusCode::NOT_FOUND,
					Json(AdminErrorResponse {
						error: "not_found".to_string(),
						message: format!("Job '{}' not found", job_id),
					}),
				)
					.into_response()
			} else {
				tracing::error!(error = %e, job_id = %job_id, "Failed to trigger job");
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
}

/// Cancel a running job.
///
/// # Authorization
///
/// Requires `system_admin` role.
///
/// # Response
///
/// Returns success message if the job was cancelled.
#[utoipa::path(
	post,
	path = "/api/admin/jobs/{job_id}/cancel",
	params(("job_id" = String, Path, description = "Job ID")),
	responses(
		(status = 200, description = "Job cancelled", body = JobSuccessResponse),
		(status = 401, description = "Not authenticated", body = AdminErrorResponse),
		(status = 403, description = "Not authorized", body = AdminErrorResponse),
		(status = 404, description = "Job not found", body = AdminErrorResponse),
		(status = 501, description = "Job scheduler not configured", body = AdminErrorResponse)
	),
	tag = "admin-jobs"
)]
#[tracing::instrument(skip(state), fields(actor_id = %current_user.user.id, job_id = %job_id))]
pub async fn cancel_job(
	RequireAuth(current_user): RequireAuth,
	State(state): State<AppState>,
	Path(job_id): Path<String>,
) -> impl IntoResponse {
	let locale = resolve_user_locale(&current_user, &state.default_locale);

	if !current_user.user.is_system_admin {
		tracing::warn!(actor_id = %current_user.user.id, "Unauthorized job cancel attempt");
		return (
			StatusCode::FORBIDDEN,
			Json(AdminErrorResponse {
				error: "forbidden".to_string(),
				message: t(locale, "server.api.admin.system_admin_required").to_string(),
			}),
		)
			.into_response();
	}

	let scheduler = match &state.job_scheduler {
		Some(s) => s,
		None => {
			return (
				StatusCode::NOT_IMPLEMENTED,
				Json(AdminErrorResponse {
					error: "not_implemented".to_string(),
					message: "Job scheduler not configured".to_string(),
				}),
			)
				.into_response();
		}
	};

	match scheduler.cancel_job(&job_id).await {
		Ok(()) => {
			tracing::info!(
				actor_id = %current_user.user.id,
				job_id = %job_id,
				"Cancelled job"
			);
			(
				StatusCode::OK,
				Json(JobSuccessResponse {
					message: format!("Job {} cancelled", job_id),
				}),
			)
				.into_response()
		}
		Err(e) => {
			let error_msg = e.to_string();
			if error_msg.contains("not found") {
				(
					StatusCode::NOT_FOUND,
					Json(AdminErrorResponse {
						error: "not_found".to_string(),
						message: format!("Job '{}' not found", job_id),
					}),
				)
					.into_response()
			} else {
				tracing::error!(error = %e, job_id = %job_id, "Failed to cancel job");
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
}

/// Get job run history.
///
/// # Authorization
///
/// Requires `system_admin` role.
///
/// # Response
///
/// Returns [`JobHistoryResponse`] with paginated list of job runs.
#[utoipa::path(
	get,
	path = "/api/admin/jobs/{job_id}/history",
	params(
		("job_id" = String, Path, description = "Job ID"),
		HistoryQuery
	),
	responses(
		(status = 200, description = "Job history", body = JobHistoryResponse),
		(status = 401, description = "Not authenticated", body = AdminErrorResponse),
		(status = 403, description = "Not authorized", body = AdminErrorResponse),
		(status = 501, description = "Job scheduler not configured", body = AdminErrorResponse)
	),
	tag = "admin-jobs"
)]
#[tracing::instrument(skip(state), fields(actor_id = %current_user.user.id, job_id = %job_id))]
pub async fn job_history(
	RequireAuth(current_user): RequireAuth,
	State(state): State<AppState>,
	Path(job_id): Path<String>,
	Query(query): Query<HistoryQuery>,
) -> impl IntoResponse {
	let locale = resolve_user_locale(&current_user, &state.default_locale);

	if !current_user.user.is_system_admin {
		tracing::warn!(actor_id = %current_user.user.id, "Unauthorized job history access attempt");
		return (
			StatusCode::FORBIDDEN,
			Json(AdminErrorResponse {
				error: "forbidden".to_string(),
				message: t(locale, "server.api.admin.system_admin_required").to_string(),
			}),
		)
			.into_response();
	}

	let repository = match &state.job_repository {
		Some(r) => r,
		None => {
			return (
				StatusCode::NOT_IMPLEMENTED,
				Json(AdminErrorResponse {
					error: "not_implemented".to_string(),
					message: "Job scheduler not configured".to_string(),
				}),
			)
				.into_response();
		}
	};

	match repository
		.list_runs(&job_id, query.limit, query.offset)
		.await
	{
		Ok(runs) => {
			let run_infos: Vec<JobRunInfo> = runs
				.into_iter()
				.map(|r| JobRunInfo {
					id: r.id,
					status: r.status.as_str().to_string(),
					started_at: r.started_at.to_rfc3339(),
					completed_at: r.completed_at.map(|t| t.to_rfc3339()),
					duration_ms: r.duration_ms,
					error_message: r.error_message,
					retry_count: r.retry_count,
					triggered_by: r.triggered_by.as_str().to_string(),
					metadata: r.metadata,
				})
				.collect();

			tracing::info!(
				actor_id = %current_user.user.id,
				job_id = %job_id,
				run_count = run_infos.len(),
				"Retrieved job history"
			);

			(
				StatusCode::OK,
				Json(JobHistoryResponse {
					total: run_infos.len() as u32,
					runs: run_infos,
					limit: query.limit,
					offset: query.offset,
				}),
			)
				.into_response()
		}
		Err(e) => {
			tracing::error!(error = %e, job_id = %job_id, "Failed to get job history");
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

/// Enable a job.
///
/// # Authorization
///
/// Requires `system_admin` role.
///
/// # Response
///
/// Returns success message if the job was enabled.
#[utoipa::path(
	post,
	path = "/api/admin/jobs/{job_id}/enable",
	params(("job_id" = String, Path, description = "Job ID")),
	responses(
		(status = 200, description = "Job enabled", body = JobSuccessResponse),
		(status = 401, description = "Not authenticated", body = AdminErrorResponse),
		(status = 403, description = "Not authorized", body = AdminErrorResponse),
		(status = 404, description = "Job not found", body = AdminErrorResponse),
		(status = 501, description = "Job scheduler not configured", body = AdminErrorResponse)
	),
	tag = "admin-jobs"
)]
#[tracing::instrument(skip(state), fields(actor_id = %current_user.user.id, job_id = %job_id))]
pub async fn enable_job(
	RequireAuth(current_user): RequireAuth,
	State(state): State<AppState>,
	Path(job_id): Path<String>,
) -> impl IntoResponse {
	set_job_enabled(current_user, state, job_id, true).await
}

/// Disable a job.
///
/// # Authorization
///
/// Requires `system_admin` role.
///
/// # Response
///
/// Returns success message if the job was disabled.
#[utoipa::path(
	post,
	path = "/api/admin/jobs/{job_id}/disable",
	params(("job_id" = String, Path, description = "Job ID")),
	responses(
		(status = 200, description = "Job disabled", body = JobSuccessResponse),
		(status = 401, description = "Not authenticated", body = AdminErrorResponse),
		(status = 403, description = "Not authorized", body = AdminErrorResponse),
		(status = 404, description = "Job not found", body = AdminErrorResponse),
		(status = 501, description = "Job scheduler not configured", body = AdminErrorResponse)
	),
	tag = "admin-jobs"
)]
#[tracing::instrument(skip(state), fields(actor_id = %current_user.user.id, job_id = %job_id))]
pub async fn disable_job(
	RequireAuth(current_user): RequireAuth,
	State(state): State<AppState>,
	Path(job_id): Path<String>,
) -> impl IntoResponse {
	set_job_enabled(current_user, state, job_id, false).await
}

async fn set_job_enabled(
	current_user: loom_server_auth::middleware::CurrentUser,
	state: AppState,
	job_id: String,
	enabled: bool,
) -> impl IntoResponse {
	let locale = resolve_user_locale(&current_user, &state.default_locale);

	if !current_user.user.is_system_admin {
		tracing::warn!(actor_id = %current_user.user.id, "Unauthorized job enable/disable attempt");
		return (
			StatusCode::FORBIDDEN,
			Json(AdminErrorResponse {
				error: "forbidden".to_string(),
				message: t(locale, "server.api.admin.system_admin_required").to_string(),
			}),
		)
			.into_response();
	}

	let repository = match &state.job_repository {
		Some(r) => r,
		None => {
			return (
				StatusCode::NOT_IMPLEMENTED,
				Json(AdminErrorResponse {
					error: "not_implemented".to_string(),
					message: "Job scheduler not configured".to_string(),
				}),
			)
				.into_response();
		}
	};

	match repository.set_enabled(&job_id, enabled).await {
		Ok(()) => {
			let action = if enabled { "enabled" } else { "disabled" };
			tracing::info!(
				actor_id = %current_user.user.id,
				job_id = %job_id,
				enabled = enabled,
				"Job {}",
				action
			);
			(
				StatusCode::OK,
				Json(JobSuccessResponse {
					message: format!("Job {} {}", job_id, action),
				}),
			)
				.into_response()
		}
		Err(e) => {
			let error_msg = e.to_string();
			if error_msg.contains("not found") {
				(
					StatusCode::NOT_FOUND,
					Json(AdminErrorResponse {
						error: "not_found".to_string(),
						message: format!("Job '{}' not found", job_id),
					}),
				)
					.into_response()
			} else {
				tracing::error!(error = %e, job_id = %job_id, "Failed to set job enabled state");
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
}
