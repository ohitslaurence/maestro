// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights
// reserved. SPDX-License-Identifier: Proprietary

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use axum::{
	extract::{Path, State},
	http::StatusCode,
	response::IntoResponse,
	Json,
};
use loom_common_secret::SecretString;
use loom_server_audit::{AuditEventType, AuditLogBuilder, UserId as AuditUserId};
use loom_server_auth::types::{OrgId, OrgRole};
use loom_server_scm::{OwnerType, RepoStore, Webhook, WebhookOwnerType, WebhookStore};
use url::Url;
use uuid::Uuid;

pub use loom_server_api::webhooks::*;

use crate::{
	api::AppState,
	auth_middleware::RequireAuth,
	i18n::{resolve_user_locale, t, t_fmt},
};

const VALID_EVENTS: &[&str] = &["push", "repo.created", "repo.deleted"];

fn validate_events(events: &[String], locale: &str) -> Option<String> {
	if events.is_empty() {
		return Some(t(locale, "server.api.scm.webhook.events_required").to_string());
	}
	for event in events {
		if !VALID_EVENTS.contains(&event.as_str()) {
			return Some(t_fmt(
				locale,
				"server.api.scm.webhook.invalid_event",
				&[("event", event), ("valid_events", &VALID_EVENTS.join(", "))],
			));
		}
	}
	None
}

fn validate_url(url_str: &str, locale: &str) -> Option<String> {
	if url_str.is_empty() {
		return Some(t(locale, "server.api.scm.webhook.url_required").to_string());
	}
	if !url_str.starts_with("https://") && !url_str.starts_with("http://") {
		return Some(t(locale, "server.api.scm.webhook.url_invalid_protocol").to_string());
	}
	if url_str.len() > 2048 {
		return Some(t(locale, "server.api.scm.webhook.url_too_long").to_string());
	}

	let url = match Url::parse(url_str) {
		Ok(u) => u,
		Err(_) => return Some(t(locale, "server.api.scm.webhook.url_invalid").to_string()),
	};

	let host = match url.host_str() {
		Some(h) => h,
		None => return Some(t(locale, "server.api.scm.webhook.url_invalid").to_string()),
	};

	if host == "localhost" || host == "127.0.0.1" || host == "::1" || host == "[::1]" {
		return Some(t(locale, "server.api.scm.webhook.url_localhost_blocked").to_string());
	}

	if let Ok(ip) = host.parse::<IpAddr>() {
		if is_private_or_reserved(&ip) {
			return Some(t(locale, "server.api.scm.webhook.url_private_ip_blocked").to_string());
		}
	}

	if host.starts_with('[') && host.ends_with(']') {
		if let Ok(ip) = host[1..host.len() - 1].parse::<Ipv6Addr>() {
			if is_private_or_reserved(&IpAddr::V6(ip)) {
				return Some(t(locale, "server.api.scm.webhook.url_private_ip_blocked").to_string());
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
	ipv4.is_loopback()              // 127.0.0.0/8
		|| ipv4.is_private()            // 10/8, 172.16/12, 192.168/16
		|| ipv4.is_link_local()         // 169.254.0.0/16 (includes cloud metadata 169.254.169.254)
		|| ipv4.is_broadcast()          // 255.255.255.255
		|| ipv4.is_unspecified() // 0.0.0.0
}

fn is_private_or_reserved_v6(ipv6: &Ipv6Addr) -> bool {
	ipv6.is_loopback()              // ::1
		|| ipv6.is_unspecified()        // ::
		|| ipv6.segments()[0] == 0xfe80 // Link-local (fe80::/10)
		|| ipv6.segments()[0] & 0xfe00 == 0xfc00 // Unique local (fc00::/7)
}

async fn check_repo_admin(
	repo_id: Uuid,
	current_user: &loom_server_auth::middleware::CurrentUser,
	state: &AppState,
	locale: &str,
) -> Result<(), (StatusCode, Json<WebhookErrorResponse>)> {
	let scm_store = state.scm_repo_store.as_ref().ok_or_else(|| {
		(
			StatusCode::INTERNAL_SERVER_ERROR,
			Json(WebhookErrorResponse {
				error: "not_configured".to_string(),
				message: t(locale, "server.api.error.not_configured").to_string(),
			}),
		)
	})?;

	let repo = scm_store.get_by_id(repo_id).await.map_err(|e| {
		tracing::error!(error = %e, "Failed to get repository");
		(
			StatusCode::INTERNAL_SERVER_ERROR,
			Json(WebhookErrorResponse {
				error: "internal_error".to_string(),
				message: t(locale, "server.api.error.internal").to_string(),
			}),
		)
	})?;

	let repo = repo.ok_or_else(|| {
		(
			StatusCode::NOT_FOUND,
			Json(WebhookErrorResponse {
				error: "not_found".to_string(),
				message: t(locale, "server.api.scm.repo.not_found").to_string(),
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
			Json(WebhookErrorResponse {
				error: "forbidden".to_string(),
				message: t(locale, "server.api.scm.webhook.admin_required").to_string(),
			}),
		));
	}

	Ok(())
}

async fn check_org_admin(
	org_id: Uuid,
	current_user: &loom_server_auth::middleware::CurrentUser,
	state: &AppState,
	locale: &str,
) -> Result<(), (StatusCode, Json<WebhookErrorResponse>)> {
	let org_id_typed = OrgId::new(org_id);

	let org = state
		.org_repo
		.get_org_by_id(&org_id_typed)
		.await
		.map_err(|e| {
			tracing::error!(error = %e, "Failed to get organization");
			(
				StatusCode::INTERNAL_SERVER_ERROR,
				Json(WebhookErrorResponse {
					error: "internal_error".to_string(),
					message: t(locale, "server.api.error.internal").to_string(),
				}),
			)
		})?;

	if org.is_none() {
		return Err((
			StatusCode::NOT_FOUND,
			Json(WebhookErrorResponse {
				error: "not_found".to_string(),
				message: t(locale, "server.api.org.not_found").to_string(),
			}),
		));
	}

	let membership = state
		.org_repo
		.get_membership(&org_id_typed, &current_user.user.id)
		.await
		.map_err(|e| {
			tracing::error!(error = %e, "Failed to check org membership");
			(
				StatusCode::INTERNAL_SERVER_ERROR,
				Json(WebhookErrorResponse {
					error: "internal_error".to_string(),
					message: t(locale, "server.api.error.internal").to_string(),
				}),
			)
		})?;

	let is_admin = match membership {
		Some(m) => m.role == OrgRole::Owner || m.role == OrgRole::Admin,
		None => false,
	};

	if !is_admin {
		return Err((
			StatusCode::FORBIDDEN,
			Json(WebhookErrorResponse {
				error: "forbidden".to_string(),
				message: t(locale, "server.api.scm.webhook.admin_required").to_string(),
			}),
		));
	}

	Ok(())
}

#[utoipa::path(
	get,
	path = "/api/v1/repos/{id}/webhooks",
	params(
		("id" = Uuid, Path, description = "Repository ID")
	),
	responses(
		(status = 200, description = "List of webhooks", body = ListWebhooksResponse),
		(status = 401, description = "Not authenticated", body = WebhookErrorResponse),
		(status = 403, description = "Not authorized", body = WebhookErrorResponse),
		(status = 404, description = "Repository not found", body = WebhookErrorResponse)
	),
	tag = "webhooks"
)]
#[tracing::instrument(skip(state), fields(repo_id = %id))]
pub async fn list_repo_webhooks(
	RequireAuth(current_user): RequireAuth,
	State(state): State<AppState>,
	Path(id): Path<Uuid>,
) -> impl IntoResponse {
	let locale = resolve_user_locale(&current_user, &state.default_locale);

	if let Err(e) = check_repo_admin(id, &current_user, &state, locale).await {
		return e.into_response();
	}

	let webhook_store = match state.scm_webhook_store.as_ref() {
		Some(store) => store,
		None => {
			return (
				StatusCode::INTERNAL_SERVER_ERROR,
				Json(WebhookErrorResponse {
					error: "not_configured".to_string(),
					message: t(locale, "server.api.error.not_configured").to_string(),
				}),
			)
				.into_response();
		}
	};

	match webhook_store.list_by_repo(id).await {
		Ok(webhooks) => {
			let response = ListWebhooksResponse {
				webhooks: webhooks.into_iter().map(Into::into).collect(),
			};
			(StatusCode::OK, Json(response)).into_response()
		}
		Err(e) => {
			tracing::error!(error = %e, "Failed to list webhooks");
			(
				StatusCode::INTERNAL_SERVER_ERROR,
				Json(WebhookErrorResponse {
					error: "internal_error".to_string(),
					message: t(locale, "server.api.error.internal").to_string(),
				}),
			)
				.into_response()
		}
	}
}

#[utoipa::path(
	post,
	path = "/api/v1/repos/{id}/webhooks",
	params(
		("id" = Uuid, Path, description = "Repository ID")
	),
	request_body = CreateWebhookRequest,
	responses(
		(status = 201, description = "Webhook created", body = WebhookResponse),
		(status = 400, description = "Invalid request", body = WebhookErrorResponse),
		(status = 401, description = "Not authenticated", body = WebhookErrorResponse),
		(status = 403, description = "Not authorized", body = WebhookErrorResponse),
		(status = 404, description = "Repository not found", body = WebhookErrorResponse)
	),
	tag = "webhooks"
)]
#[tracing::instrument(skip(state, payload), fields(repo_id = %id))]
pub async fn create_repo_webhook(
	RequireAuth(current_user): RequireAuth,
	State(state): State<AppState>,
	Path(id): Path<Uuid>,
	Json(payload): Json<CreateWebhookRequest>,
) -> impl IntoResponse {
	let locale = resolve_user_locale(&current_user, &state.default_locale);

	if let Err(e) = check_repo_admin(id, &current_user, &state, locale).await {
		return e.into_response();
	}

	if let Some(error) = validate_url(&payload.url, locale) {
		return (
			StatusCode::BAD_REQUEST,
			Json(WebhookErrorResponse {
				error: "invalid_url".to_string(),
				message: error,
			}),
		)
			.into_response();
	}

	if let Some(error) = validate_events(&payload.events, locale) {
		return (
			StatusCode::BAD_REQUEST,
			Json(WebhookErrorResponse {
				error: "invalid_events".to_string(),
				message: error,
			}),
		)
			.into_response();
	}

	if payload.secret.is_empty() {
		return (
			StatusCode::BAD_REQUEST,
			Json(WebhookErrorResponse {
				error: "invalid_secret".to_string(),
				message: t(locale, "server.api.scm.webhook.secret_required").to_string(),
			}),
		)
			.into_response();
	}

	let webhook_store = match state.scm_webhook_store.as_ref() {
		Some(store) => store,
		None => {
			return (
				StatusCode::INTERNAL_SERVER_ERROR,
				Json(WebhookErrorResponse {
					error: "not_configured".to_string(),
					message: t(locale, "server.api.error.not_configured").to_string(),
				}),
			)
				.into_response();
		}
	};

	let webhook = Webhook::new(
		WebhookOwnerType::Repo,
		id,
		payload.url,
		SecretString::new(payload.secret),
		payload.payload_format.into(),
		payload.events,
	);

	match webhook_store.create(&webhook).await {
		Ok(created) => {
			state.audit_service.log(
				AuditLogBuilder::new(AuditEventType::WebhookReceived)
					.actor(AuditUserId::new(current_user.user.id.into_inner()))
					.resource("webhook", created.id.to_string())
					.details(serde_json::json!({
						"action": "repo_webhook_created",
						"repo_id": id.to_string(),
						"url": &created.url,
					}))
					.build(),
			);

			tracing::info!(
				repo_id = %id,
				webhook_id = %created.id,
				url = %created.url,
				created_by = %current_user.user.id,
				"Webhook created"
			);
			(StatusCode::CREATED, Json(WebhookResponse::from(created))).into_response()
		}
		Err(e) => {
			tracing::error!(error = %e, "Failed to create webhook");
			(
				StatusCode::INTERNAL_SERVER_ERROR,
				Json(WebhookErrorResponse {
					error: "internal_error".to_string(),
					message: t(locale, "server.api.error.internal").to_string(),
				}),
			)
				.into_response()
		}
	}
}

#[utoipa::path(
	delete,
	path = "/api/v1/repos/{id}/webhooks/{wid}",
	params(
		("id" = Uuid, Path, description = "Repository ID"),
		("wid" = Uuid, Path, description = "Webhook ID")
	),
	responses(
		(status = 204, description = "Webhook deleted"),
		(status = 401, description = "Not authenticated", body = WebhookErrorResponse),
		(status = 403, description = "Not authorized", body = WebhookErrorResponse),
		(status = 404, description = "Repository or webhook not found", body = WebhookErrorResponse)
	),
	tag = "webhooks"
)]
#[tracing::instrument(skip(state), fields(repo_id = %id, webhook_id = %wid))]
pub async fn delete_repo_webhook(
	RequireAuth(current_user): RequireAuth,
	State(state): State<AppState>,
	Path((id, wid)): Path<(Uuid, Uuid)>,
) -> impl IntoResponse {
	let locale = resolve_user_locale(&current_user, &state.default_locale);

	if let Err(e) = check_repo_admin(id, &current_user, &state, locale).await {
		return e.into_response();
	}

	let webhook_store = match state.scm_webhook_store.as_ref() {
		Some(store) => store,
		None => {
			return (
				StatusCode::INTERNAL_SERVER_ERROR,
				Json(WebhookErrorResponse {
					error: "not_configured".to_string(),
					message: t(locale, "server.api.error.not_configured").to_string(),
				}),
			)
				.into_response();
		}
	};

	let webhook = match webhook_store.get_by_id(wid).await {
		Ok(Some(w)) => w,
		Ok(None) => {
			return (
				StatusCode::NOT_FOUND,
				Json(WebhookErrorResponse {
					error: "not_found".to_string(),
					message: t(locale, "server.api.scm.webhook.not_found").to_string(),
				}),
			)
				.into_response();
		}
		Err(e) => {
			tracing::error!(error = %e, "Failed to get webhook");
			return (
				StatusCode::INTERNAL_SERVER_ERROR,
				Json(WebhookErrorResponse {
					error: "internal_error".to_string(),
					message: t(locale, "server.api.error.internal").to_string(),
				}),
			)
				.into_response();
		}
	};

	if webhook.owner_type != WebhookOwnerType::Repo || webhook.owner_id != id {
		return (
			StatusCode::NOT_FOUND,
			Json(WebhookErrorResponse {
				error: "not_found".to_string(),
				message: t(locale, "server.api.scm.webhook.not_found").to_string(),
			}),
		)
			.into_response();
	}

	match webhook_store.delete(wid).await {
		Ok(()) => {
			state.audit_service.log(
				AuditLogBuilder::new(AuditEventType::WebhookReceived)
					.actor(AuditUserId::new(current_user.user.id.into_inner()))
					.resource("webhook", wid.to_string())
					.details(serde_json::json!({
						"action": "repo_webhook_deleted",
						"repo_id": id.to_string(),
					}))
					.build(),
			);

			tracing::info!(
				repo_id = %id,
				webhook_id = %wid,
				deleted_by = %current_user.user.id,
				"Webhook deleted"
			);
			StatusCode::NO_CONTENT.into_response()
		}
		Err(loom_server_scm::ScmError::NotFound) => (
			StatusCode::NOT_FOUND,
			Json(WebhookErrorResponse {
				error: "not_found".to_string(),
				message: t(locale, "server.api.scm.webhook.not_found").to_string(),
			}),
		)
			.into_response(),
		Err(e) => {
			tracing::error!(error = %e, "Failed to delete webhook");
			(
				StatusCode::INTERNAL_SERVER_ERROR,
				Json(WebhookErrorResponse {
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
	path = "/api/v1/orgs/{id}/webhooks",
	params(
		("id" = Uuid, Path, description = "Organization ID")
	),
	responses(
		(status = 200, description = "List of webhooks", body = ListWebhooksResponse),
		(status = 401, description = "Not authenticated", body = WebhookErrorResponse),
		(status = 403, description = "Not authorized", body = WebhookErrorResponse),
		(status = 404, description = "Organization not found", body = WebhookErrorResponse)
	),
	tag = "webhooks"
)]
#[tracing::instrument(skip(state), fields(org_id = %id))]
pub async fn list_org_webhooks(
	RequireAuth(current_user): RequireAuth,
	State(state): State<AppState>,
	Path(id): Path<Uuid>,
) -> impl IntoResponse {
	let locale = resolve_user_locale(&current_user, &state.default_locale);

	if let Err(e) = check_org_admin(id, &current_user, &state, locale).await {
		return e.into_response();
	}

	let webhook_store = match state.scm_webhook_store.as_ref() {
		Some(store) => store,
		None => {
			return (
				StatusCode::INTERNAL_SERVER_ERROR,
				Json(WebhookErrorResponse {
					error: "not_configured".to_string(),
					message: t(locale, "server.api.error.not_configured").to_string(),
				}),
			)
				.into_response();
		}
	};

	match webhook_store.list_by_org(id).await {
		Ok(webhooks) => {
			let response = ListWebhooksResponse {
				webhooks: webhooks.into_iter().map(Into::into).collect(),
			};
			(StatusCode::OK, Json(response)).into_response()
		}
		Err(e) => {
			tracing::error!(error = %e, "Failed to list webhooks");
			(
				StatusCode::INTERNAL_SERVER_ERROR,
				Json(WebhookErrorResponse {
					error: "internal_error".to_string(),
					message: t(locale, "server.api.error.internal").to_string(),
				}),
			)
				.into_response()
		}
	}
}

#[utoipa::path(
	post,
	path = "/api/v1/orgs/{id}/webhooks",
	params(
		("id" = Uuid, Path, description = "Organization ID")
	),
	request_body = CreateWebhookRequest,
	responses(
		(status = 201, description = "Webhook created", body = WebhookResponse),
		(status = 400, description = "Invalid request", body = WebhookErrorResponse),
		(status = 401, description = "Not authenticated", body = WebhookErrorResponse),
		(status = 403, description = "Not authorized", body = WebhookErrorResponse),
		(status = 404, description = "Organization not found", body = WebhookErrorResponse)
	),
	tag = "webhooks"
)]
#[tracing::instrument(skip(state, payload), fields(org_id = %id))]
pub async fn create_org_webhook(
	RequireAuth(current_user): RequireAuth,
	State(state): State<AppState>,
	Path(id): Path<Uuid>,
	Json(payload): Json<CreateWebhookRequest>,
) -> impl IntoResponse {
	let locale = resolve_user_locale(&current_user, &state.default_locale);

	if let Err(e) = check_org_admin(id, &current_user, &state, locale).await {
		return e.into_response();
	}

	if let Some(error) = validate_url(&payload.url, locale) {
		return (
			StatusCode::BAD_REQUEST,
			Json(WebhookErrorResponse {
				error: "invalid_url".to_string(),
				message: error,
			}),
		)
			.into_response();
	}

	if let Some(error) = validate_events(&payload.events, locale) {
		return (
			StatusCode::BAD_REQUEST,
			Json(WebhookErrorResponse {
				error: "invalid_events".to_string(),
				message: error,
			}),
		)
			.into_response();
	}

	if payload.secret.is_empty() {
		return (
			StatusCode::BAD_REQUEST,
			Json(WebhookErrorResponse {
				error: "invalid_secret".to_string(),
				message: t(locale, "server.api.scm.webhook.secret_required").to_string(),
			}),
		)
			.into_response();
	}

	let webhook_store = match state.scm_webhook_store.as_ref() {
		Some(store) => store,
		None => {
			return (
				StatusCode::INTERNAL_SERVER_ERROR,
				Json(WebhookErrorResponse {
					error: "not_configured".to_string(),
					message: t(locale, "server.api.error.not_configured").to_string(),
				}),
			)
				.into_response();
		}
	};

	let webhook = Webhook::new(
		WebhookOwnerType::Org,
		id,
		payload.url,
		SecretString::new(payload.secret),
		payload.payload_format.into(),
		payload.events,
	);

	match webhook_store.create(&webhook).await {
		Ok(created) => {
			state.audit_service.log(
				AuditLogBuilder::new(AuditEventType::WebhookReceived)
					.actor(AuditUserId::new(current_user.user.id.into_inner()))
					.resource("webhook", created.id.to_string())
					.details(serde_json::json!({
						"action": "org_webhook_created",
						"org_id": id.to_string(),
						"url": &created.url,
					}))
					.build(),
			);

			tracing::info!(
				org_id = %id,
				webhook_id = %created.id,
				url = %created.url,
				created_by = %current_user.user.id,
				"Org webhook created"
			);
			(StatusCode::CREATED, Json(WebhookResponse::from(created))).into_response()
		}
		Err(e) => {
			tracing::error!(error = %e, "Failed to create webhook");
			(
				StatusCode::INTERNAL_SERVER_ERROR,
				Json(WebhookErrorResponse {
					error: "internal_error".to_string(),
					message: t(locale, "server.api.error.internal").to_string(),
				}),
			)
				.into_response()
		}
	}
}

#[utoipa::path(
	delete,
	path = "/api/v1/orgs/{id}/webhooks/{wid}",
	params(
		("id" = Uuid, Path, description = "Organization ID"),
		("wid" = Uuid, Path, description = "Webhook ID")
	),
	responses(
		(status = 204, description = "Webhook deleted"),
		(status = 401, description = "Not authenticated", body = WebhookErrorResponse),
		(status = 403, description = "Not authorized", body = WebhookErrorResponse),
		(status = 404, description = "Organization or webhook not found", body = WebhookErrorResponse)
	),
	tag = "webhooks"
)]
#[tracing::instrument(skip(state), fields(org_id = %id, webhook_id = %wid))]
pub async fn delete_org_webhook(
	RequireAuth(current_user): RequireAuth,
	State(state): State<AppState>,
	Path((id, wid)): Path<(Uuid, Uuid)>,
) -> impl IntoResponse {
	let locale = resolve_user_locale(&current_user, &state.default_locale);

	if let Err(e) = check_org_admin(id, &current_user, &state, locale).await {
		return e.into_response();
	}

	let webhook_store = match state.scm_webhook_store.as_ref() {
		Some(store) => store,
		None => {
			return (
				StatusCode::INTERNAL_SERVER_ERROR,
				Json(WebhookErrorResponse {
					error: "not_configured".to_string(),
					message: t(locale, "server.api.error.not_configured").to_string(),
				}),
			)
				.into_response();
		}
	};

	let webhook = match webhook_store.get_by_id(wid).await {
		Ok(Some(w)) => w,
		Ok(None) => {
			return (
				StatusCode::NOT_FOUND,
				Json(WebhookErrorResponse {
					error: "not_found".to_string(),
					message: t(locale, "server.api.scm.webhook.not_found").to_string(),
				}),
			)
				.into_response();
		}
		Err(e) => {
			tracing::error!(error = %e, "Failed to get webhook");
			return (
				StatusCode::INTERNAL_SERVER_ERROR,
				Json(WebhookErrorResponse {
					error: "internal_error".to_string(),
					message: t(locale, "server.api.error.internal").to_string(),
				}),
			)
				.into_response();
		}
	};

	if webhook.owner_type != WebhookOwnerType::Org || webhook.owner_id != id {
		return (
			StatusCode::NOT_FOUND,
			Json(WebhookErrorResponse {
				error: "not_found".to_string(),
				message: t(locale, "server.api.scm.webhook.not_found").to_string(),
			}),
		)
			.into_response();
	}

	match webhook_store.delete(wid).await {
		Ok(()) => {
			state.audit_service.log(
				AuditLogBuilder::new(AuditEventType::WebhookReceived)
					.actor(AuditUserId::new(current_user.user.id.into_inner()))
					.resource("webhook", wid.to_string())
					.details(serde_json::json!({
						"action": "org_webhook_deleted",
						"org_id": id.to_string(),
					}))
					.build(),
			);

			tracing::info!(
				org_id = %id,
				webhook_id = %wid,
				deleted_by = %current_user.user.id,
				"Org webhook deleted"
			);
			StatusCode::NO_CONTENT.into_response()
		}
		Err(loom_server_scm::ScmError::NotFound) => (
			StatusCode::NOT_FOUND,
			Json(WebhookErrorResponse {
				error: "not_found".to_string(),
				message: t(locale, "server.api.scm.webhook.not_found").to_string(),
			}),
		)
			.into_response(),
		Err(e) => {
			tracing::error!(error = %e, "Failed to delete webhook");
			(
				StatusCode::INTERNAL_SERVER_ERROR,
				Json(WebhookErrorResponse {
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

	#[test]
	fn test_ssrf_protection_blocks_localhost() {
		assert!(validate_url("http://localhost/hook", "en").is_some());
		assert!(validate_url("http://127.0.0.1/hook", "en").is_some());
		assert!(validate_url("https://localhost:8080/hook", "en").is_some());
		assert!(validate_url("http://127.0.0.1:3000/hook", "en").is_some());
	}

	#[test]
	fn test_ssrf_protection_blocks_ipv6_localhost() {
		assert!(validate_url("http://[::1]/hook", "en").is_some());
		assert!(validate_url("https://[::1]:8080/hook", "en").is_some());
	}

	#[test]
	fn test_ssrf_protection_blocks_private_ipv4() {
		assert!(validate_url("http://10.0.0.1/hook", "en").is_some());
		assert!(validate_url("http://10.255.255.255/hook", "en").is_some());
		assert!(validate_url("http://172.16.0.1/hook", "en").is_some());
		assert!(validate_url("http://172.31.255.255/hook", "en").is_some());
		assert!(validate_url("http://192.168.1.1/hook", "en").is_some());
		assert!(validate_url("http://192.168.0.1/hook", "en").is_some());
	}

	#[test]
	fn test_ssrf_protection_blocks_link_local() {
		assert!(validate_url("http://169.254.1.1/hook", "en").is_some());
		assert!(validate_url("http://169.254.169.254/metadata", "en").is_some());
	}

	#[test]
	fn test_ssrf_protection_blocks_special_addresses() {
		assert!(validate_url("http://0.0.0.0/hook", "en").is_some());
		assert!(validate_url("http://255.255.255.255/hook", "en").is_some());
	}

	#[test]
	fn test_ssrf_protection_blocks_private_ipv6() {
		assert!(validate_url("http://[fe80::1]/hook", "en").is_some());
		assert!(validate_url("http://[fc00::1]/hook", "en").is_some());
		assert!(validate_url("http://[fd00::1]/hook", "en").is_some());
	}

	#[test]
	fn test_ssrf_protection_allows_public_urls() {
		assert!(validate_url("https://example.com/hook", "en").is_none());
		assert!(validate_url("https://api.github.com/webhook", "en").is_none());
		assert!(validate_url("http://webhook.site/abc123", "en").is_none());
		assert!(validate_url("https://8.8.8.8/hook", "en").is_none());
	}

	#[test]
	fn test_validate_url_rejects_invalid_protocol() {
		assert!(validate_url("ftp://example.com/hook", "en").is_some());
		assert!(validate_url("file:///etc/passwd", "en").is_some());
	}

	#[test]
	fn test_validate_url_rejects_empty() {
		assert!(validate_url("", "en").is_some());
	}

	#[test]
	fn test_is_private_or_reserved_v4() {
		assert!(is_private_or_reserved_v4(&"127.0.0.1".parse().unwrap()));
		assert!(is_private_or_reserved_v4(&"10.0.0.1".parse().unwrap()));
		assert!(is_private_or_reserved_v4(&"172.16.0.1".parse().unwrap()));
		assert!(is_private_or_reserved_v4(&"192.168.1.1".parse().unwrap()));
		assert!(is_private_or_reserved_v4(
			&"169.254.169.254".parse().unwrap()
		));
		assert!(is_private_or_reserved_v4(&"0.0.0.0".parse().unwrap()));

		assert!(!is_private_or_reserved_v4(&"8.8.8.8".parse().unwrap()));
		assert!(!is_private_or_reserved_v4(&"1.1.1.1".parse().unwrap()));
		assert!(!is_private_or_reserved_v4(&"172.32.0.1".parse().unwrap()));
	}

	#[test]
	fn test_is_private_or_reserved_v6() {
		assert!(is_private_or_reserved_v6(&"::1".parse().unwrap()));
		assert!(is_private_or_reserved_v6(&"::".parse().unwrap()));
		assert!(is_private_or_reserved_v6(&"fe80::1".parse().unwrap()));
		assert!(is_private_or_reserved_v6(&"fc00::1".parse().unwrap()));
		assert!(is_private_or_reserved_v6(&"fd00::1".parse().unwrap()));

		assert!(!is_private_or_reserved_v6(
			&"2001:4860:4860::8888".parse().unwrap()
		));
	}
}
