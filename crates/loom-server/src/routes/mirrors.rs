// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights
// reserved. SPDX-License-Identifier: Proprietary

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use axum::{
	extract::{Path, State},
	http::StatusCode,
	response::IntoResponse,
	Json,
};
use loom_server_audit::{AuditEventType, AuditLogBuilder, UserId as AuditUserId};
use loom_server_auth::types::{OrgId, OrgRole};
use loom_server_scm::{OwnerType, RepoStore};
use loom_server_scm_mirror::{CreatePushMirror, PushMirrorStore};
use url::Url;
use uuid::Uuid;

pub use loom_server_api::mirrors::*;
pub use loom_server_api::repos::RepoErrorResponse;

use crate::{
	api::AppState,
	auth_middleware::RequireAuth,
	i18n::{resolve_user_locale, t},
};

fn validate_mirror_url(url_str: &str, locale: &str) -> Option<String> {
	if url_str.is_empty() {
		return Some(t(locale, "server.api.scm.mirror.url_required").to_string());
	}
	if !url_str.starts_with("https://") && !url_str.starts_with("http://") {
		return Some(t(locale, "server.api.scm.mirror.url_invalid_protocol").to_string());
	}
	if url_str.len() > 2048 {
		return Some(t(locale, "server.api.scm.mirror.url_too_long").to_string());
	}

	let url = match Url::parse(url_str) {
		Ok(u) => u,
		Err(_) => return Some(t(locale, "server.api.scm.mirror.url_invalid").to_string()),
	};

	let host = match url.host_str() {
		Some(h) => h,
		None => return Some(t(locale, "server.api.scm.mirror.url_invalid").to_string()),
	};

	if host == "localhost" || host == "127.0.0.1" || host == "::1" || host == "[::1]" {
		return Some(t(locale, "server.api.scm.mirror.url_localhost_blocked").to_string());
	}

	if let Ok(ip) = host.parse::<IpAddr>() {
		if is_private_or_reserved(&ip) {
			return Some(t(locale, "server.api.scm.mirror.url_private_ip_blocked").to_string());
		}
	}

	if host.starts_with('[') && host.ends_with(']') {
		if let Ok(ip) = host[1..host.len() - 1].parse::<Ipv6Addr>() {
			if is_private_or_reserved(&IpAddr::V6(ip)) {
				return Some(t(locale, "server.api.scm.mirror.url_private_ip_blocked").to_string());
			}
		}
	}

	None
}

fn is_private_or_reserved(ip: &IpAddr) -> bool {
	match ip {
		IpAddr::V4(ipv4) => is_private_or_reserved_v4(ipv4),
		IpAddr::V6(ipv6) => is_private_or_reserved_v6(ipv6),
	}
}

fn is_private_or_reserved_v4(ipv4: &Ipv4Addr) -> bool {
	ipv4.is_loopback()
		|| ipv4.is_private()
		|| ipv4.is_link_local()
		|| ipv4.is_broadcast()
		|| ipv4.is_unspecified()
}

fn is_private_or_reserved_v6(ipv6: &Ipv6Addr) -> bool {
	ipv6.is_loopback()
		|| ipv6.is_unspecified()
		|| ipv6.segments()[0] == 0xfe80
		|| ipv6.segments()[0] & 0xfe00 == 0xfc00
}

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

async fn check_repo_write(
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

	let has_write = match repo.owner_type {
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

	if !has_write {
		return Err((
			StatusCode::FORBIDDEN,
			Json(RepoErrorResponse {
				error: "forbidden".to_string(),
				message: t(locale, "server.api.scm.write_required").to_string(),
			}),
		));
	}

	Ok(())
}

#[utoipa::path(
	get,
	path = "/api/v1/repos/{id}/mirrors",
	params(
		("id" = Uuid, Path, description = "Repository ID")
	),
	responses(
		(status = 200, description = "List of push mirrors", body = ListMirrorsResponse),
		(status = 401, description = "Not authenticated", body = RepoErrorResponse),
		(status = 403, description = "Not authorized", body = RepoErrorResponse),
		(status = 404, description = "Repository not found", body = RepoErrorResponse)
	),
	tag = "mirrors"
)]
#[tracing::instrument(skip(state), fields(repo_id = %id))]
pub async fn list_mirrors(
	RequireAuth(current_user): RequireAuth,
	State(state): State<AppState>,
	Path(id): Path<Uuid>,
) -> impl IntoResponse {
	let locale = resolve_user_locale(&current_user, &state.default_locale);

	if let Err(e) = check_repo_admin(id, &current_user, &state, locale).await {
		return e.into_response();
	}

	let mirror_store = match state.push_mirror_store.as_ref() {
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

	match mirror_store.list_by_repo(id).await {
		Ok(mirrors) => {
			let response = ListMirrorsResponse {
				mirrors: mirrors.into_iter().map(Into::into).collect(),
			};
			(StatusCode::OK, Json(response)).into_response()
		}
		Err(e) => {
			tracing::error!(error = %e, "Failed to list mirrors");
			(
				StatusCode::INTERNAL_SERVER_ERROR,
				Json(RepoErrorResponse {
					error: "internal_error".to_string(),
					message: t(locale, "server.api.scm.mirror.failed_to_list").to_string(),
				}),
			)
				.into_response()
		}
	}
}

#[utoipa::path(
	post,
	path = "/api/v1/repos/{id}/mirrors",
	params(
		("id" = Uuid, Path, description = "Repository ID")
	),
	request_body = CreateMirrorRequest,
	responses(
		(status = 201, description = "Mirror created", body = MirrorResponse),
		(status = 400, description = "Invalid request", body = RepoErrorResponse),
		(status = 401, description = "Not authenticated", body = RepoErrorResponse),
		(status = 403, description = "Not authorized", body = RepoErrorResponse),
		(status = 404, description = "Repository not found", body = RepoErrorResponse)
	),
	tag = "mirrors"
)]
#[tracing::instrument(skip(state, payload), fields(repo_id = %id))]
pub async fn create_mirror(
	RequireAuth(current_user): RequireAuth,
	State(state): State<AppState>,
	Path(id): Path<Uuid>,
	Json(payload): Json<CreateMirrorRequest>,
) -> impl IntoResponse {
	let locale = resolve_user_locale(&current_user, &state.default_locale);

	if let Err(e) = check_repo_admin(id, &current_user, &state, locale).await {
		return e.into_response();
	}

	if let Some(error) = validate_mirror_url(&payload.remote_url, locale) {
		return (
			StatusCode::BAD_REQUEST,
			Json(RepoErrorResponse {
				error: "invalid_url".to_string(),
				message: error,
			}),
		)
			.into_response();
	}

	let mirror_store = match state.push_mirror_store.as_ref() {
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

	let credential_key = payload
		.credential_key
		.unwrap_or_else(|| format!("mirror:{}:{}", id, Uuid::new_v4()));

	let create = CreatePushMirror {
		repo_id: id,
		remote_url: payload.remote_url,
		credential_key,
		enabled: payload.enabled.unwrap_or(true),
	};

	match mirror_store.create(&create).await {
		Ok(created) => {
			state.audit_service.log(
				AuditLogBuilder::new(AuditEventType::MirrorCreated)
					.actor(AuditUserId::new(current_user.user.id.into_inner()))
					.resource("mirror", created.id.to_string())
					.details(serde_json::json!({
						"repo_id": id.to_string(),
						"remote_url": &created.remote_url,
					}))
					.build(),
			);

			tracing::info!(
				repo_id = %id,
				mirror_id = %created.id,
				remote_url = %created.remote_url,
				created_by = %current_user.user.id,
				"Push mirror created"
			);
			(StatusCode::CREATED, Json(MirrorResponse::from(created))).into_response()
		}
		Err(e) => {
			tracing::error!(error = %e, "Failed to create mirror");
			(
				StatusCode::INTERNAL_SERVER_ERROR,
				Json(RepoErrorResponse {
					error: "internal_error".to_string(),
					message: t(locale, "server.api.scm.mirror.failed_to_create").to_string(),
				}),
			)
				.into_response()
		}
	}
}

#[utoipa::path(
	delete,
	path = "/api/v1/repos/{id}/mirrors/{mirror_id}",
	params(
		("id" = Uuid, Path, description = "Repository ID"),
		("mirror_id" = Uuid, Path, description = "Mirror ID")
	),
	responses(
		(status = 204, description = "Mirror deleted"),
		(status = 401, description = "Not authenticated", body = RepoErrorResponse),
		(status = 403, description = "Not authorized", body = RepoErrorResponse),
		(status = 404, description = "Repository or mirror not found", body = RepoErrorResponse)
	),
	tag = "mirrors"
)]
#[tracing::instrument(skip(state), fields(repo_id = %id, mirror_id = %mirror_id))]
pub async fn delete_mirror(
	RequireAuth(current_user): RequireAuth,
	State(state): State<AppState>,
	Path((id, mirror_id)): Path<(Uuid, Uuid)>,
) -> impl IntoResponse {
	let locale = resolve_user_locale(&current_user, &state.default_locale);

	if let Err(e) = check_repo_admin(id, &current_user, &state, locale).await {
		return e.into_response();
	}

	let mirror_store = match state.push_mirror_store.as_ref() {
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

	let mirror = match mirror_store.get_by_id(mirror_id).await {
		Ok(Some(m)) => m,
		Ok(None) => {
			return (
				StatusCode::NOT_FOUND,
				Json(RepoErrorResponse {
					error: "not_found".to_string(),
					message: t(locale, "server.api.scm.mirror.not_found").to_string(),
				}),
			)
				.into_response();
		}
		Err(e) => {
			tracing::error!(error = %e, "Failed to get mirror");
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

	if mirror.repo_id != id {
		return (
			StatusCode::NOT_FOUND,
			Json(RepoErrorResponse {
				error: "not_found".to_string(),
				message: t(locale, "server.api.scm.mirror.not_found").to_string(),
			}),
		)
			.into_response();
	}

	match mirror_store.delete(mirror_id).await {
		Ok(()) => {
			state.audit_service.log(
				AuditLogBuilder::new(AuditEventType::RepoDeleted)
					.actor(AuditUserId::new(current_user.user.id.into_inner()))
					.resource("mirror", mirror_id.to_string())
					.details(serde_json::json!({
						"repo_id": id.to_string(),
						"action": "mirror_deleted",
					}))
					.build(),
			);

			tracing::info!(
				repo_id = %id,
				mirror_id = %mirror_id,
				deleted_by = %current_user.user.id,
				"Push mirror deleted"
			);
			StatusCode::NO_CONTENT.into_response()
		}
		Err(loom_server_db::DbError::NotFound(_)) => (
			StatusCode::NOT_FOUND,
			Json(RepoErrorResponse {
				error: "not_found".to_string(),
				message: t(locale, "server.api.scm.mirror.not_found").to_string(),
			}),
		)
			.into_response(),
		Err(e) => {
			tracing::error!(error = %e, "Failed to delete mirror");
			(
				StatusCode::INTERNAL_SERVER_ERROR,
				Json(RepoErrorResponse {
					error: "internal_error".to_string(),
					message: t(locale, "server.api.scm.mirror.failed_to_delete").to_string(),
				}),
			)
				.into_response()
		}
	}
}

#[utoipa::path(
	post,
	path = "/api/v1/repos/{id}/mirrors/{mirror_id}/sync",
	params(
		("id" = Uuid, Path, description = "Repository ID"),
		("mirror_id" = Uuid, Path, description = "Mirror ID")
	),
	responses(
		(status = 200, description = "Sync triggered", body = SyncResponse),
		(status = 401, description = "Not authenticated", body = RepoErrorResponse),
		(status = 403, description = "Not authorized", body = RepoErrorResponse),
		(status = 404, description = "Repository or mirror not found", body = RepoErrorResponse)
	),
	tag = "mirrors"
)]
#[tracing::instrument(skip(state), fields(repo_id = %id, mirror_id = %mirror_id))]
pub async fn trigger_sync(
	RequireAuth(current_user): RequireAuth,
	State(state): State<AppState>,
	Path((id, mirror_id)): Path<(Uuid, Uuid)>,
) -> impl IntoResponse {
	let locale = resolve_user_locale(&current_user, &state.default_locale);

	if let Err(e) = check_repo_write(id, &current_user, &state, locale).await {
		return e.into_response();
	}

	let mirror_store = match state.push_mirror_store.as_ref() {
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

	let mirror = match mirror_store.get_by_id(mirror_id).await {
		Ok(Some(m)) => m,
		Ok(None) => {
			return (
				StatusCode::NOT_FOUND,
				Json(RepoErrorResponse {
					error: "not_found".to_string(),
					message: t(locale, "server.api.scm.mirror.not_found").to_string(),
				}),
			)
				.into_response();
		}
		Err(e) => {
			tracing::error!(error = %e, "Failed to get mirror");
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

	if mirror.repo_id != id {
		return (
			StatusCode::NOT_FOUND,
			Json(RepoErrorResponse {
				error: "not_found".to_string(),
				message: t(locale, "server.api.scm.mirror.not_found").to_string(),
			}),
		)
			.into_response();
	}

	if !mirror.enabled {
		return (
			StatusCode::BAD_REQUEST,
			Json(RepoErrorResponse {
				error: "mirror_disabled".to_string(),
				message: t(locale, "server.api.scm.mirror.disabled").to_string(),
			}),
		)
			.into_response();
	}

	state.audit_service.log(
		AuditLogBuilder::new(AuditEventType::MirrorSynced)
			.actor(AuditUserId::new(current_user.user.id.into_inner()))
			.resource("mirror", mirror_id.to_string())
			.details(serde_json::json!({
				"repo_id": id.to_string(),
			}))
			.build(),
	);

	tracing::info!(
		repo_id = %id,
		mirror_id = %mirror_id,
		triggered_by = %current_user.user.id,
		"Mirror sync triggered"
	);

	(
		StatusCode::OK,
		Json(SyncResponse {
			message: t(locale, "server.api.scm.mirror.sync_queued").to_string(),
			queued: true,
		}),
	)
		.into_response()
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_ssrf_protection_blocks_localhost() {
		assert!(validate_mirror_url("http://localhost/repo.git", "en").is_some());
		assert!(validate_mirror_url("http://127.0.0.1/repo.git", "en").is_some());
		assert!(validate_mirror_url("https://localhost:8080/repo.git", "en").is_some());
	}

	#[test]
	fn test_ssrf_protection_blocks_ipv6_localhost() {
		assert!(validate_mirror_url("http://[::1]/repo.git", "en").is_some());
		assert!(validate_mirror_url("https://[::1]:8080/repo.git", "en").is_some());
	}

	#[test]
	fn test_ssrf_protection_blocks_private_ipv4() {
		assert!(validate_mirror_url("http://10.0.0.1/repo.git", "en").is_some());
		assert!(validate_mirror_url("http://172.16.0.1/repo.git", "en").is_some());
		assert!(validate_mirror_url("http://192.168.1.1/repo.git", "en").is_some());
	}

	#[test]
	fn test_ssrf_protection_blocks_link_local() {
		assert!(validate_mirror_url("http://169.254.169.254/metadata", "en").is_some());
	}

	#[test]
	fn test_ssrf_protection_allows_public_urls() {
		assert!(validate_mirror_url("https://github.com/owner/repo.git", "en").is_none());
		assert!(validate_mirror_url("https://gitlab.com/owner/repo.git", "en").is_none());
		assert!(validate_mirror_url("https://8.8.8.8/repo.git", "en").is_none());
	}

	#[test]
	fn test_validate_url_rejects_invalid_protocol() {
		assert!(validate_mirror_url("ftp://example.com/repo.git", "en").is_some());
		assert!(validate_mirror_url("file:///etc/passwd", "en").is_some());
	}

	#[test]
	fn test_validate_url_rejects_empty() {
		assert!(validate_mirror_url("", "en").is_some());
	}
}
