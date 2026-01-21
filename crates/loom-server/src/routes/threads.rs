// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights
// reserved. SPDX-License-Identifier: Proprietary

//! Thread-related HTTP handlers.
//!
//! All thread operations are scoped to the authenticated user:
//! - Users can only see, modify, and delete their own threads
//! - Threads are automatically associated with the creating user

use axum::{
	extract::{Path, Query, State},
	http::{HeaderMap, StatusCode},
	response::IntoResponse,
	Json,
};
use loom_common_thread::{Thread, ThreadId};
pub use loom_server_api::threads::{
	ListParams, ListResponse, SearchParams, SearchResponse, SearchResponseHit,
	UpdateVisibilityRequest,
};
use loom_server_audit::{AuditEventType, AuditLogBuilder};

use crate::{api::AppState, auth_middleware::RequireAuth, error::ServerError};

/// PUT /api/threads/{id} - Create or update a thread.
///
/// Supports optimistic concurrency via If-Match header.
/// For new threads, the current user is set as the owner.
/// For existing threads, only the owner can update.
#[utoipa::path(
    put,
    path = "/api/threads/{id}",
    params(
        ("id" = String, Path, description = "Thread ID")
    ),
    request_body = Thread,
    responses(
        (status = 200, description = "Thread created or updated", body = Thread),
        (status = 400, description = "Invalid request", body = crate::error::ErrorResponse),
        (status = 403, description = "Forbidden - not the thread owner", body = crate::error::ErrorResponse),
        (status = 409, description = "Version conflict", body = crate::error::ErrorResponse),
        (status = 500, description = "Internal server error", body = crate::error::ErrorResponse)
    ),
    tag = "threads"
)]
#[axum::debug_handler]
pub async fn upsert_thread(
	RequireAuth(current_user): RequireAuth,
	State(state): State<AppState>,
	Path(id): Path<String>,
	headers: HeaderMap,
	Json(mut thread): Json<Thread>,
) -> Result<impl IntoResponse, ServerError> {
	let user_id = current_user.user.id.to_string();

	// Validate ID matches path
	if thread.id.as_str() != id {
		return Err(ServerError::BadRequest(format!(
			"Thread ID in body ({}) does not match path ({})",
			thread.id, id
		)));
	}

	// Check if thread exists and verify ownership
	let existing_owner = state.repo.get_thread_owner_user_id(&id).await?;
	if let Some(owner_id) = &existing_owner {
		// Thread exists - verify ownership
		if owner_id != &user_id && !current_user.user.is_system_admin {
			tracing::info!(
				thread_id = %id,
				user_id = %user_id,
				owner_id = %owner_id,
				"unauthorized thread upsert attempt"
			);
			return Err(ServerError::NotFound(id));
		}
	}

	// Parse If-Match header for optimistic concurrency
	let expected_version = headers
		.get("If-Match")
		.and_then(|v| v.to_str().ok())
		.and_then(|s| s.parse::<u64>().ok());

	tracing::debug!(
			thread_id = %id,
			version = thread.version,
			expected_version = ?expected_version,
			"upserting thread"
	);

	// Update timestamp
	thread.updated_at = chrono::Utc::now().to_rfc3339();

	let stored = state.repo.upsert(&thread, expected_version).await?;

	// Set owner for new threads
	if existing_owner.is_none() {
		state.repo.set_owner_user_id(&id, &user_id).await?;
	}

	tracing::info!(
			thread_id = %id,
			version = stored.version,
			"thread upserted"
	);

	state.audit_service.log(
		AuditLogBuilder::new(AuditEventType::ThreadCreated)
			.resource("thread", id.clone())
			.details(serde_json::json!({
				"version": stored.version,
				"visibility": format!("{:?}", stored.visibility),
				"user_id": user_id,
			}))
			.build(),
	);

	Ok((StatusCode::OK, Json(stored)))
}

/// GET /api/threads/{id} - Get a thread by ID.
///
/// Only the thread owner can access the thread.
#[utoipa::path(
    get,
    path = "/api/threads/{id}",
    params(
        ("id" = String, Path, description = "Thread ID")
    ),
    responses(
        (status = 200, description = "Thread found", body = Thread),
        (status = 404, description = "Thread not found", body = crate::error::ErrorResponse),
        (status = 500, description = "Internal server error", body = crate::error::ErrorResponse)
    ),
    tag = "threads"
)]
#[axum::debug_handler]
pub async fn get_thread(
	RequireAuth(current_user): RequireAuth,
	State(state): State<AppState>,
	Path(id): Path<String>,
) -> Result<impl IntoResponse, ServerError> {
	let user_id = current_user.user.id.to_string();
	let thread_id = ThreadId::from_string(id.clone());

	tracing::debug!(thread_id = %id, "getting thread");

	// Check ownership first
	let owner_id = state.repo.get_thread_owner_user_id(&id).await?;
	match owner_id {
		Some(owner) if owner == user_id || current_user.user.is_system_admin => {
			// User is owner or admin, allow access
		}
		Some(_) => {
			// Thread exists but user is not the owner
			tracing::debug!(thread_id = %id, user_id = %user_id, "thread access denied");
			return Err(ServerError::NotFound(id));
		}
		None => {
			// Thread doesn't exist
			return Err(ServerError::NotFound(id));
		}
	}

	let thread = state
		.repo
		.get(&thread_id)
		.await?
		.ok_or_else(|| ServerError::NotFound(id.clone()))?;

	Ok(Json(thread))
}

/// GET /api/threads - List threads.
///
/// Returns only threads owned by the current user.
#[utoipa::path(
    get,
    path = "/api/threads",
    params(ListParams),
    responses(
        (status = 200, description = "List of threads", body = ListResponse),
        (status = 500, description = "Internal server error", body = crate::error::ErrorResponse)
    ),
    tag = "threads"
)]
#[axum::debug_handler]
pub async fn list_threads(
	RequireAuth(current_user): RequireAuth,
	State(state): State<AppState>,
	Query(params): Query<ListParams>,
) -> Result<impl IntoResponse, ServerError> {
	let user_id = current_user.user.id.to_string();

	tracing::debug!(
			workspace = ?params.workspace,
			limit = params.limit,
			offset = params.offset,
			"listing threads for user"
	);

	let threads = state
		.repo
		.list_for_owner(
			&user_id,
			params.workspace.as_deref(),
			params.limit,
			params.offset,
		)
		.await?;

	let total = state
		.repo
		.count_for_owner(&user_id, params.workspace.as_deref())
		.await?;

	let response = ListResponse {
		threads,
		total,
		limit: params.limit,
		offset: params.offset,
	};

	Ok(Json(response))
}

/// DELETE /api/threads/{id} - Soft-delete a thread.
///
/// Only the thread owner can delete their thread.
#[utoipa::path(
    delete,
    path = "/api/threads/{id}",
    params(
        ("id" = String, Path, description = "Thread ID")
    ),
    responses(
        (status = 204, description = "Thread deleted"),
        (status = 404, description = "Thread not found", body = crate::error::ErrorResponse),
        (status = 500, description = "Internal server error", body = crate::error::ErrorResponse)
    ),
    tag = "threads"
)]
#[axum::debug_handler]
pub async fn delete_thread(
	RequireAuth(current_user): RequireAuth,
	State(state): State<AppState>,
	Path(id): Path<String>,
) -> Result<impl IntoResponse, ServerError> {
	let user_id = current_user.user.id.to_string();
	let thread_id = ThreadId::from_string(id.clone());

	tracing::debug!(thread_id = %id, "deleting thread");

	// Check ownership first
	let owner_id = state.repo.get_thread_owner_user_id(&id).await?;
	match owner_id {
		Some(owner) if owner == user_id || current_user.user.is_system_admin => {
			// User is owner or admin, allow deletion
		}
		Some(_) => {
			// Thread exists but user is not the owner
			tracing::debug!(thread_id = %id, user_id = %user_id, "thread deletion denied");
			return Err(ServerError::NotFound(id));
		}
		None => {
			// Thread doesn't exist
			return Err(ServerError::NotFound(id));
		}
	}

	let deleted = state.repo.delete(&thread_id).await?;

	if deleted {
		tracing::info!(thread_id = %id, "thread deleted");

		state.audit_service.log(
			AuditLogBuilder::new(AuditEventType::ThreadDeleted)
				.resource("thread", id.clone())
				.details(serde_json::json!({
					"user_id": user_id,
				}))
				.build(),
		);

		Ok(StatusCode::NO_CONTENT)
	} else {
		Err(ServerError::NotFound(id))
	}
}

/// POST /api/threads/{id}/visibility - Update thread visibility.
///
/// Allows changing the visibility of a thread without syncing the full thread
/// content. Supports optimistic concurrency via If-Match header.
/// Only the thread owner can update visibility.
#[utoipa::path(
    post,
    path = "/api/threads/{id}/visibility",
    params(
        ("id" = String, Path, description = "Thread ID")
    ),
    request_body = UpdateVisibilityRequest,
    responses(
        (status = 200, description = "Visibility updated", body = Thread),
        (status = 404, description = "Thread not found", body = crate::error::ErrorResponse),
        (status = 409, description = "Version conflict", body = crate::error::ErrorResponse),
        (status = 500, description = "Internal server error", body = crate::error::ErrorResponse)
    ),
    tag = "threads"
)]
#[axum::debug_handler]
pub async fn update_thread_visibility(
	RequireAuth(current_user): RequireAuth,
	State(state): State<AppState>,
	Path(id): Path<String>,
	headers: HeaderMap,
	Json(body): Json<UpdateVisibilityRequest>,
) -> Result<impl IntoResponse, ServerError> {
	let user_id = current_user.user.id.to_string();
	let thread_id = ThreadId::from_string(id.clone());

	// Check ownership first
	let owner_id = state.repo.get_thread_owner_user_id(&id).await?;
	match owner_id {
		Some(owner) if owner == user_id || current_user.user.is_system_admin => {
			// User is owner or admin, allow update
		}
		Some(_) => {
			// Thread exists but user is not the owner
			tracing::debug!(thread_id = %id, user_id = %user_id, "thread visibility update denied");
			return Err(ServerError::NotFound(id));
		}
		None => {
			// Thread doesn't exist
			return Err(ServerError::NotFound(id));
		}
	}

	let expected_version = headers
		.get("If-Match")
		.and_then(|v| v.to_str().ok())
		.and_then(|s| s.parse::<u64>().ok());

	tracing::debug!(
			thread_id = %id,
			visibility = ?body.visibility,
			expected_version = ?expected_version,
			"updating thread visibility"
	);

	let mut thread = state
		.repo
		.get(&thread_id)
		.await?
		.ok_or_else(|| ServerError::NotFound(id.clone()))?;

	if let Some(expected) = expected_version {
		if thread.version != expected {
			return Err(ServerError::Conflict {
				expected: thread.version,
				actual: expected,
			});
		}
	}

	thread.visibility = body.visibility;
	thread.updated_at = chrono::Utc::now().to_rfc3339();
	thread.version += 1;

	let stored = state.repo.upsert(&thread, None).await?;

	tracing::info!(
			thread_id = %id,
			version = stored.version,
			visibility = ?stored.visibility,
			"thread visibility updated"
	);

	state.audit_service.log(
		AuditLogBuilder::new(AuditEventType::ThreadVisibilityChanged)
			.resource("thread", id.clone())
			.details(serde_json::json!({
				"visibility": format!("{:?}", stored.visibility),
				"version": stored.version,
				"user_id": user_id,
			}))
			.build(),
	);

	Ok((StatusCode::OK, Json(stored)))
}

/// GET /api/threads/search - Search threads.
///
/// Searches only threads owned by the current user.
#[utoipa::path(
    get,
    path = "/api/threads/search",
    params(SearchParams),
    responses(
        (status = 200, description = "Search results", body = SearchResponse),
        (status = 400, description = "Invalid search query", body = crate::error::ErrorResponse),
        (status = 500, description = "Internal server error", body = crate::error::ErrorResponse)
    ),
    tag = "threads"
)]
#[axum::debug_handler]
pub async fn search_threads(
	RequireAuth(current_user): RequireAuth,
	State(state): State<AppState>,
	Query(params): Query<SearchParams>,
) -> Result<Json<SearchResponse>, ServerError> {
	let user_id = current_user.user.id.to_string();
	let query = params.q.trim();

	if query.is_empty() {
		return Err(ServerError::BadRequest("Empty search query".into()));
	}

	tracing::debug!(
			query = %query,
			workspace = ?params.workspace,
			limit = params.limit,
			offset = params.offset,
			"searching threads for user"
	);

	let hits = state
		.repo
		.search_for_owner(
			&user_id,
			query,
			params.workspace.as_deref(),
			params.limit,
			params.offset,
		)
		.await?;

	let response_hits = hits
		.into_iter()
		.map(|h| SearchResponseHit {
			summary: h.summary,
			score: h.score,
		})
		.collect();

	Ok(Json(SearchResponse {
		hits: response_hits,
		limit: params.limit,
		offset: params.offset,
	}))
}
