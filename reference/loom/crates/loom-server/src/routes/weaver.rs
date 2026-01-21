// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights
// reserved. SPDX-License-Identifier: Proprietary

//! Weaver provisioning HTTP handlers.

use std::collections::HashMap;
use std::convert::Infallible;

use axum::{
	extract::{
		ws::{Message, WebSocket, WebSocketUpgrade},
		Path, Query, State,
	},
	http::StatusCode,
	response::{sse::Event, IntoResponse, Sse},
	routing::{delete, get, post},
	Json, Router,
};
use futures::{
	stream::{Stream, StreamExt},
	SinkExt,
};
use loom_server_audit::{AuditEventType, AuditLogBuilder, UserId as AuditUserId};
use loom_server_auth::{CurrentUser, OrgId};
use loom_server_weaver::{CreateWeaverRequest, LogStreamOptions, ResourceSpec, Weaver, WeaverId};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use uuid::Uuid;

pub use loom_server_api::weaver::*;

use crate::{
	api::AppState,
	auth_middleware::RequireAuth,
	error::ServerError,
	i18n::{resolve_user_locale, t, t_fmt},
};

// ============================================================================
// Helper functions
// ============================================================================

/// Access level for weaver operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WeaverAccess {
	/// Full read/write access (owner or system admin)
	Full,
	/// Read-only access (support role)
	ReadOnly,
	/// No access
	None,
}

fn get_weaver_access(current_user: &CurrentUser, weaver: &Weaver) -> WeaverAccess {
	if current_user.user.is_system_admin() {
		return WeaverAccess::Full;
	}
	if weaver.owner_user_id == current_user.user.id.to_string() {
		return WeaverAccess::Full;
	}
	if current_user.user.is_support() {
		return WeaverAccess::ReadOnly;
	}
	WeaverAccess::None
}

fn is_weaver_owner_or_admin(current_user: &CurrentUser, weaver: &Weaver) -> bool {
	matches!(get_weaver_access(current_user, weaver), WeaverAccess::Full)
}

fn can_read_weaver(current_user: &CurrentUser, weaver: &Weaver) -> bool {
	!matches!(get_weaver_access(current_user, weaver), WeaverAccess::None)
}

// ============================================================================
// Route handlers
// ============================================================================

/// POST /api/weaver - Create a new weaver.
#[utoipa::path(
    post,
    path = "/api/weaver",
    request_body = CreateWeaverApiRequest,
    responses(
        (status = 201, description = "Weaver created", body = WeaverApiResponse),
        (status = 400, description = "Invalid request", body = crate::error::ErrorResponse),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 500, description = "Internal server error", body = crate::error::ErrorResponse)
    ),
    tag = "weavers",
    security(("api_key" = []))
)]
#[axum::debug_handler]
pub async fn create_weaver(
	State(state): State<AppState>,
	RequireAuth(current_user): RequireAuth,
	Json(request): Json<CreateWeaverApiRequest>,
) -> Result<impl IntoResponse, ServerError> {
	let locale = resolve_user_locale(&current_user, &state.default_locale);

	let provisioner = state.provisioner.as_ref().ok_or_else(|| {
		ServerError::Internal(t(locale, "server.api.weaver.provisioner_not_configured"))
	})?;

	let org_uuid = Uuid::parse_str(&request.org_id).map_err(|_| {
		ServerError::BadRequest(t_fmt(
			locale,
			"server.api.weaver.invalid_org_id",
			&[("id", &request.org_id)],
		))
	})?;
	let org_id = OrgId::new(org_uuid);

	if !current_user.user.is_system_admin() {
		match state
			.org_repo
			.get_membership(&org_id, &current_user.user.id)
			.await
		{
			Ok(Some(_)) => {}
			Ok(None) => {
				return Err(ServerError::Forbidden(t(
					locale,
					"server.api.weaver.not_org_member",
				)));
			}
			Err(e) => {
				tracing::error!(error = %e, org_id = %request.org_id, "Failed to check org membership");
				return Err(ServerError::Internal(t(
					locale,
					"server.api.weaver.membership_check_failed",
				)));
			}
		}
	}

	let actor_id = current_user.user.id.to_string();
	let image_for_audit = request.image.clone();
	let org_id_for_audit = request.org_id.clone();
	tracing::info!(image = %request.image, org_id = %request.org_id, actor_id = %actor_id, "Creating weaver");

	let create_request = CreateWeaverRequest {
		image: request.image,
		env: request.env,
		resources: ResourceSpec {
			memory_limit: request.resources.memory_limit,
			cpu_limit: request.resources.cpu_limit,
		},
		tags: request.tags,
		lifetime_hours: request.lifetime_hours,
		command: request.command,
		args: request.args,
		workdir: request.workdir,
		repo: None,
		branch: None,
		owner_user_id: Some(actor_id.clone()),
		org_id: request.org_id,
		repo_id: request.repo_id,
	};

	let weaver = provisioner.create_weaver(create_request).await?;

	state.audit_service.log(
		AuditLogBuilder::new(AuditEventType::WeaverCreated)
			.actor(AuditUserId::new(current_user.user.id.into_inner()))
			.resource("weaver", weaver.id.to_string())
			.details(serde_json::json!({
				"image": &image_for_audit,
				"org_id": &org_id_for_audit,
				"pod_name": &weaver.pod_name,
			}))
			.build(),
	);

	tracing::info!(weaver_id = %weaver.id, pod_name = %weaver.pod_name, actor_id = %actor_id, "Weaver created");

	Ok((StatusCode::CREATED, Json(WeaverApiResponse::from(weaver))))
}

/// GET /api/weavers - List all weavers.
#[utoipa::path(
    get,
    path = "/api/weavers",
    params(ListWeaversParams),
    responses(
        (status = 200, description = "List of weavers", body = ListWeaversApiResponse),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 500, description = "Internal server error", body = crate::error::ErrorResponse)
    ),
    tag = "weavers",
    security(("api_key" = []))
)]
#[axum::debug_handler]
pub async fn list_weavers(
	State(state): State<AppState>,
	RequireAuth(current_user): RequireAuth,
	Query(params): Query<ListWeaversParams>,
) -> Result<impl IntoResponse, ServerError> {
	let locale = resolve_user_locale(&current_user, &state.default_locale);

	let provisioner = state.provisioner.as_ref().ok_or_else(|| {
		ServerError::Internal(t(locale, "server.api.weaver.provisioner_not_configured"))
	})?;

	let tag_filter = parse_tag_filter(params.tag);

	let weavers = if current_user.user.is_system_admin() || current_user.user.is_support() {
		provisioner.list_weavers(tag_filter).await?
	} else {
		let user_id = current_user.user.id.to_string();
		let all_weavers = provisioner.list_weavers_for_user(&user_id).await?;
		if let Some(tags) = tag_filter {
			all_weavers
				.into_iter()
				.filter(|w| tags.iter().all(|(k, v)| w.tags.get(k) == Some(v)))
				.collect()
		} else {
			all_weavers
		}
	};
	let count = weavers.len() as u32;

	let response = ListWeaversApiResponse {
		weavers: weavers.into_iter().map(WeaverApiResponse::from).collect(),
		count,
	};

	Ok(Json(response))
}

/// GET /api/weaver/{id} - Get a specific weaver.
#[utoipa::path(
    get,
    path = "/api/weaver/{id}",
    params(
        ("id" = String, Path, description = "Weaver ID")
    ),
    responses(
        (status = 200, description = "Weaver details", body = WeaverApiResponse),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 403, description = "Forbidden", body = crate::error::ErrorResponse),
        (status = 404, description = "Weaver not found", body = crate::error::ErrorResponse),
        (status = 500, description = "Internal server error", body = crate::error::ErrorResponse)
    ),
    tag = "weavers",
    security(("api_key" = []))
)]
#[axum::debug_handler]
pub async fn get_weaver(
	State(state): State<AppState>,
	RequireAuth(current_user): RequireAuth,
	Path(id): Path<String>,
) -> Result<impl IntoResponse, ServerError> {
	let locale = resolve_user_locale(&current_user, &state.default_locale);

	let provisioner = state.provisioner.as_ref().ok_or_else(|| {
		ServerError::Internal(t(locale, "server.api.weaver.provisioner_not_configured"))
	})?;

	let weaver_id: WeaverId = id.parse().map_err(|_| {
		ServerError::BadRequest(t_fmt(
			locale,
			"server.api.weaver.invalid_id",
			&[("id", &id)],
		))
	})?;

	let weaver = provisioner.get_weaver(&weaver_id).await?;

	if !can_read_weaver(&current_user, &weaver) {
		return Err(ServerError::Forbidden(t(
			locale,
			"server.api.weaver.access_denied",
		)));
	}

	Ok(Json(WeaverApiResponse::from(weaver)))
}

/// DELETE /api/weaver/{id} - Delete a weaver.
#[utoipa::path(
    delete,
    path = "/api/weaver/{id}",
    params(
        ("id" = String, Path, description = "Weaver ID")
    ),
    responses(
        (status = 204, description = "Weaver deleted"),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 403, description = "Forbidden", body = crate::error::ErrorResponse),
        (status = 404, description = "Weaver not found", body = crate::error::ErrorResponse),
        (status = 500, description = "Internal server error", body = crate::error::ErrorResponse)
    ),
    tag = "weavers",
    security(("api_key" = []))
)]
#[axum::debug_handler]
pub async fn delete_weaver(
	State(state): State<AppState>,
	RequireAuth(current_user): RequireAuth,
	Path(id): Path<String>,
) -> Result<impl IntoResponse, ServerError> {
	let locale = resolve_user_locale(&current_user, &state.default_locale);

	let provisioner = state.provisioner.as_ref().ok_or_else(|| {
		ServerError::Internal(t(locale, "server.api.weaver.provisioner_not_configured"))
	})?;

	let weaver_id: WeaverId = id.parse().map_err(|_| {
		ServerError::BadRequest(t_fmt(
			locale,
			"server.api.weaver.invalid_id",
			&[("id", &id)],
		))
	})?;

	let weaver = provisioner.get_weaver(&weaver_id).await?;

	if !is_weaver_owner_or_admin(&current_user, &weaver) {
		return Err(ServerError::Forbidden(t(
			locale,
			"server.api.weaver.delete_denied",
		)));
	}

	let actor_id = current_user.user.id.to_string();
	tracing::info!(weaver_id = %id, actor_id = %actor_id, "Deleting weaver");

	provisioner.delete_weaver(&weaver_id).await?;

	state.audit_service.log(
		AuditLogBuilder::new(AuditEventType::WeaverDeleted)
			.actor(AuditUserId::new(current_user.user.id.into_inner()))
			.resource("weaver", id.clone())
			.details(serde_json::json!({
				"pod_name": &weaver.pod_name,
			}))
			.build(),
	);

	tracing::info!(weaver_id = %id, actor_id = %actor_id, "Weaver deleted");

	Ok(StatusCode::NO_CONTENT)
}

/// GET /api/weaver/{id}/logs - Stream weaver logs via SSE.
#[utoipa::path(
    get,
    path = "/api/weaver/{id}/logs",
    params(
        ("id" = String, Path, description = "Weaver ID"),
        LogStreamParams
    ),
    responses(
        (status = 200, description = "SSE log stream", content_type = "text/event-stream"),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 403, description = "Forbidden", body = crate::error::ErrorResponse),
        (status = 404, description = "Weaver not found", body = crate::error::ErrorResponse),
        (status = 500, description = "Internal server error", body = crate::error::ErrorResponse)
    ),
    tag = "weavers",
    security(("api_key" = []))
)]
#[axum::debug_handler]
pub async fn stream_logs(
	State(state): State<AppState>,
	RequireAuth(current_user): RequireAuth,
	Path(id): Path<String>,
	Query(params): Query<LogStreamParams>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, ServerError> {
	let locale = resolve_user_locale(&current_user, &state.default_locale);

	let provisioner = state.provisioner.as_ref().ok_or_else(|| {
		ServerError::Internal(t(locale, "server.api.weaver.provisioner_not_configured"))
	})?;

	let weaver_id: WeaverId = id
		.parse()
		.map_err(|_| ServerError::BadRequest(format!("Invalid weaver ID: {id}")))?;

	let weaver = provisioner.get_weaver(&weaver_id).await?;

	if !can_read_weaver(&current_user, &weaver) {
		return Err(ServerError::Forbidden(t(
			locale,
			"server.api.weaver.logs_denied",
		)));
	}

	tracing::debug!(weaver_id = %id, tail = params.tail, timestamps = params.timestamps, "Starting log stream");

	let opts = LogStreamOptions {
		tail: params.tail,
		timestamps: params.timestamps,
	};

	let log_stream = provisioner.stream_logs(&weaver_id, opts).await?;

	let sse_stream = log_stream.map(|result| {
		let event = match result {
			Ok(bytes) => {
				let line = String::from_utf8_lossy(&bytes).into_owned();
				Event::default().data(line)
			}
			Err(e) => Event::default().event("error").data(e.to_string()),
		};
		Ok::<_, Infallible>(event)
	});

	Ok(
		Sse::new(sse_stream).keep_alive(
			axum::response::sse::KeepAlive::new()
				.interval(std::time::Duration::from_secs(15))
				.text("keep-alive"),
		),
	)
}

/// POST /api/weavers/cleanup - Trigger cleanup of expired weavers.
#[utoipa::path(
    post,
    path = "/api/weavers/cleanup",
    params(CleanupParams),
    responses(
        (status = 200, description = "Cleanup result", body = CleanupApiResponse),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 403, description = "Forbidden - system admin required", body = crate::error::ErrorResponse),
        (status = 500, description = "Internal server error", body = crate::error::ErrorResponse)
    ),
    tag = "weavers",
    security(("api_key" = []))
)]
#[axum::debug_handler]
pub async fn trigger_cleanup(
	State(state): State<AppState>,
	RequireAuth(current_user): RequireAuth,
	Query(params): Query<CleanupParams>,
) -> Result<impl IntoResponse, ServerError> {
	if !current_user.user.is_system_admin() {
		return Err(ServerError::Forbidden(
			"System admin access required for cleanup operations".to_string(),
		));
	}

	let provisioner = state
		.provisioner
		.as_ref()
		.ok_or_else(|| ServerError::Internal("Weaver provisioner not configured".to_string()))?;

	let actor_id = current_user.user.id.to_string();

	if params.dry_run {
		tracing::info!(actor_id = %actor_id, "Performing dry-run cleanup check");

		let expired = provisioner.find_expired_weavers().await?;
		let weaver_ids: Vec<String> = expired.iter().map(|w| w.id.to_string()).collect();
		let count = weaver_ids.len() as u32;

		tracing::info!(count = count, actor_id = %actor_id, "Found expired weavers (dry run)");

		Ok(Json(CleanupApiResponse {
			dry_run: true,
			deleted: None,
			would_delete: Some(weaver_ids),
			count,
		}))
	} else {
		tracing::info!(actor_id = %actor_id, "Triggering cleanup of expired weavers");

		let result = provisioner.cleanup_expired_weavers().await?;
		let weaver_ids: Vec<String> = result.deleted.iter().map(|id| id.to_string()).collect();

		tracing::info!(count = result.count, actor_id = %actor_id, "Cleanup completed");

		Ok(Json(CleanupApiResponse {
			dry_run: false,
			deleted: Some(weaver_ids),
			would_delete: None,
			count: result.count,
		}))
	}
}

/// GET /api/weaver/{id}/attach - WebSocket terminal attach.
///
/// Support users get read-only access (can view output but not send input).
#[utoipa::path(
    get,
    path = "/api/weaver/{id}/attach",
    params(
        ("id" = String, Path, description = "Weaver ID")
    ),
    responses(
        (status = 101, description = "WebSocket upgrade for terminal I/O"),
        (status = 401, description = "Unauthorized"),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Weaver not found")
    ),
    tag = "weavers"
)]
pub async fn attach_weaver(
	State(state): State<AppState>,
	RequireAuth(current_user): RequireAuth,
	Path(id): Path<String>,
	ws: WebSocketUpgrade,
) -> Result<impl IntoResponse, ServerError> {
	let locale = resolve_user_locale(&current_user, &state.default_locale);

	let provisioner = state
		.provisioner
		.as_ref()
		.ok_or_else(|| {
			ServerError::Internal(t(locale, "server.api.weaver.provisioner_not_configured"))
		})?
		.clone();

	let weaver_id: WeaverId = id.parse().map_err(|_| {
		ServerError::BadRequest(t_fmt(
			locale,
			"server.api.weaver.invalid_id",
			&[("id", &id)],
		))
	})?;

	let weaver = provisioner.get_weaver(&weaver_id).await.map_err(|_| {
		ServerError::NotFound(t_fmt(locale, "server.api.weaver.not_found", &[("id", &id)]))
	})?;

	let access = get_weaver_access(&current_user, &weaver);
	if access == WeaverAccess::None {
		return Err(ServerError::Forbidden(t(
			locale,
			"server.api.weaver.attach_denied",
		)));
	}

	let read_only = access == WeaverAccess::ReadOnly;
	let locale = locale.to_string();
	let actor_id = current_user.user.id.to_string();

	state.audit_service.log(
		AuditLogBuilder::new(AuditEventType::WeaverAttached)
			.actor(AuditUserId::new(current_user.user.id.into_inner()))
			.resource("weaver", id.clone())
			.details(serde_json::json!({
				"read_only": read_only,
			}))
			.build(),
	);

	tracing::info!(
		weaver_id = %id,
		actor_id = %actor_id,
		read_only = %read_only,
		"WebSocket attach requested"
	);

	Ok(ws.on_upgrade(move |socket| {
		handle_attach_websocket(socket, provisioner, weaver_id, read_only, locale)
	}))
}

async fn handle_attach_websocket(
	socket: WebSocket,
	provisioner: std::sync::Arc<loom_server_weaver::Provisioner>,
	weaver_id: WeaverId,
	read_only: bool,
	locale: String,
) {
	if let Err(e) =
		handle_attach_websocket_inner(socket, provisioner, weaver_id, read_only, &locale).await
	{
		tracing::error!(error = %e, "WebSocket attach error");
	}
}

async fn handle_attach_websocket_inner(
	mut socket: WebSocket,
	provisioner: std::sync::Arc<loom_server_weaver::Provisioner>,
	weaver_id: WeaverId,
	read_only: bool,
	locale: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
	let attached = match provisioner.attach_weaver(&weaver_id).await {
		Ok(a) => a,
		Err(e) => {
			let _ = socket
				.send(Message::Text(format!("ERROR: {e}").into()))
				.await;
			return Err(Box::new(e));
		}
	};
	let loom_server_weaver::AttachedProcess { stdin, stdout } = attached;

	let (mut ws_sender, mut ws_receiver) = socket.split();
	let mut stdin = stdin;
	let mut stdout = stdout;

	if read_only {
		let notice = t(locale, "server.api.weaver.read_only_attach");
		let _ = ws_sender
			.send(Message::Text(format!("\r\n*** {notice} ***\r\n").into()))
			.await;
	}

	let ws_to_pod = async {
		while let Some(msg) = ws_receiver.next().await {
			match msg {
				Ok(Message::Binary(data)) => {
					if read_only {
						continue;
					}
					if stdin.write_all(&data).await.is_err() {
						break;
					}
				}
				Ok(Message::Text(text)) => {
					if read_only {
						continue;
					}
					if stdin.write_all(text.as_bytes()).await.is_err() {
						break;
					}
				}
				Ok(Message::Close(_)) | Err(_) => break,
				_ => {}
			}
		}
	};

	let pod_to_ws = async {
		let mut buf = [0u8; 4096];
		loop {
			match stdout.read(&mut buf).await {
				Ok(0) => break,
				Ok(n) => {
					if ws_sender
						.send(Message::Binary(buf[..n].to_vec().into()))
						.await
						.is_err()
					{
						break;
					}
				}
				Err(_) => break,
			}
		}
	};

	tokio::select! {
		_ = ws_to_pod => {}
		_ = pod_to_ws => {}
	}

	tracing::debug!(weaver_id = %weaver_id, "WebSocket attach session ended");
	Ok(())
}

// ============================================================================
// Router
// ============================================================================

/// Create the weaver routes router.
pub fn weaver_routes(state: AppState) -> Router {
	Router::new()
		.route("/api/weaver", post(create_weaver))
		.route("/api/weavers", get(list_weavers))
		.route("/api/weaver/{id}", get(get_weaver))
		.route("/api/weaver/{id}", delete(delete_weaver))
		.route("/api/weaver/{id}/logs", get(stream_logs))
		.route("/api/weaver/{id}/attach", get(attach_weaver))
		.route("/api/weavers/cleanup", post(trigger_cleanup))
		.with_state(state)
}

// ============================================================================
// Helper functions
// ============================================================================

/// Parse tag filter from query parameters.
/// Tags are provided as "key:value" strings.
fn parse_tag_filter(tags: Option<Vec<String>>) -> Option<HashMap<String, String>> {
	tags
		.map(|tag_list| {
			tag_list
				.into_iter()
				.filter_map(|t| {
					let parts: Vec<&str> = t.splitn(2, ':').collect();
					if parts.len() == 2 {
						Some((parts[0].to_string(), parts[1].to_string()))
					} else {
						None
					}
				})
				.collect()
		})
		.filter(|m: &HashMap<String, String>| !m.is_empty())
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_parse_tag_filter_none() {
		assert_eq!(parse_tag_filter(None), None);
	}

	#[test]
	fn test_parse_tag_filter_empty() {
		assert_eq!(parse_tag_filter(Some(vec![])), None);
	}

	#[test]
	fn test_parse_tag_filter_single() {
		let result = parse_tag_filter(Some(vec!["project:ai-worker".to_string()]));
		let mut expected = HashMap::new();
		expected.insert("project".to_string(), "ai-worker".to_string());
		assert_eq!(result, Some(expected));
	}

	#[test]
	fn test_parse_tag_filter_multiple() {
		let result = parse_tag_filter(Some(vec![
			"project:ai-worker".to_string(),
			"env:prod".to_string(),
		]));
		let mut expected = HashMap::new();
		expected.insert("project".to_string(), "ai-worker".to_string());
		expected.insert("env".to_string(), "prod".to_string());
		assert_eq!(result, Some(expected));
	}

	#[test]
	fn test_parse_tag_filter_invalid() {
		let result = parse_tag_filter(Some(vec!["invalid-no-colon".to_string()]));
		assert_eq!(result, None);
	}

	#[test]
	fn test_parse_tag_filter_value_with_colon() {
		let result = parse_tag_filter(Some(vec!["url:https://example.com".to_string()]));
		let mut expected = HashMap::new();
		expected.insert("url".to_string(), "https://example.com".to_string());
		assert_eq!(result, Some(expected));
	}
}
