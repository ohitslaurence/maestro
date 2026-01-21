// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights
// reserved. SPDX-License-Identifier: Proprietary

//! WireGuard tunnel HTTP handlers.
//!
//! Public API routes (require user auth):
//! - `POST /api/wg/devices` - Register a new device
//! - `GET /api/wg/devices` - List user's devices
//! - `DELETE /api/wg/devices/{id}` - Revoke a device
//! - `POST /api/wg/sessions` - Create tunnel session
//! - `GET /api/wg/sessions` - List active sessions
//! - `DELETE /api/wg/sessions/{id}` - Terminate session
//! - `GET /api/wg/derp-map` - Get DERP map
//!
//! Internal API routes (require SVID auth):
//! - `POST /internal/wg/weavers` - Register weaver's WG key
//! - `DELETE /internal/wg/weavers/{id}` - Unregister weaver
//! - `GET /internal/wg/weavers/{id}/peers` - SSE stream of peer updates

use std::convert::Infallible;

use axum::{
	extract::{Path, State},
	http::{HeaderMap, StatusCode},
	response::{sse::Event, IntoResponse, Sse},
	Json,
};
use base64::prelude::*;
use futures::stream::Stream;

use loom_server_wgtunnel::{
	DeviceResponse, PeerEvent, RegisterDeviceRequest, RegisterWeaverRequest, RegisterWeaverResponse,
	SessionListItem, SessionResponse, WeaverResponse, WgError,
};
use tracing::{info, instrument, warn};
use uuid::Uuid;

use crate::{api::AppState, auth_middleware::RequireAuth, error::ServerError};

use super::weaver_auth::ErrorResponse;

fn wg_error_to_response(e: WgError) -> ServerError {
	match e {
		WgError::DeviceNotFound => ServerError::NotFound("Device not found".to_string()),
		WgError::WeaverNotFound => ServerError::NotFound("Weaver not found".to_string()),
		WgError::SessionNotFound => ServerError::NotFound("Session not found".to_string()),
		WgError::DeviceAlreadyExists => {
			ServerError::BadRequest("Device already registered".to_string())
		}
		WgError::DeviceRevoked => ServerError::BadRequest("Device has been revoked".to_string()),
		WgError::WeaverAlreadyRegistered => {
			ServerError::BadRequest("Weaver already registered".to_string())
		}
		WgError::SessionAlreadyExists => ServerError::BadRequest("Session already exists".to_string()),
		WgError::InvalidPublicKey(msg) => ServerError::BadRequest(format!("Invalid public key: {msg}")),
		WgError::IpAllocation(msg) => ServerError::Internal(format!("IP allocation failed: {msg}")),
		WgError::Unauthorized(msg) => ServerError::Unauthorized(msg),
		WgError::Database(e) => ServerError::Db(e),
		WgError::Config(msg) | WgError::DerpMap(msg) | WgError::Internal(msg) => {
			ServerError::Internal(msg)
		}
	}
}

fn extract_bearer_token(headers: &HeaderMap) -> Result<&str, (StatusCode, Json<ErrorResponse>)> {
	let auth_header = headers.get("authorization").ok_or_else(|| {
		(
			StatusCode::UNAUTHORIZED,
			Json(ErrorResponse {
				error: "missing_token".to_string(),
				message: "Authorization header required".to_string(),
			}),
		)
	})?;

	let auth_str = auth_header.to_str().map_err(|_| {
		(
			StatusCode::BAD_REQUEST,
			Json(ErrorResponse {
				error: "invalid_header".to_string(),
				message: "Invalid authorization header encoding".to_string(),
			}),
		)
	})?;

	auth_str.strip_prefix("Bearer ").ok_or_else(|| {
		(
			StatusCode::BAD_REQUEST,
			Json(ErrorResponse {
				error: "invalid_token_format".to_string(),
				message: "Authorization header must be 'Bearer <token>'".to_string(),
			}),
		)
	})
}

fn parse_public_key(key_b64: &str) -> Result<[u8; 32], ServerError> {
	// Try standard base64 first, then no-pad (WireGuard convention)
	let bytes = BASE64_STANDARD
		.decode(key_b64)
		.or_else(|_| BASE64_STANDARD_NO_PAD.decode(key_b64))
		.map_err(|_| ServerError::BadRequest("Invalid base64 public key".to_string()))?;

	bytes
		.try_into()
		.map_err(|_| ServerError::BadRequest("Public key must be exactly 32 bytes".to_string()))
}

// ============================================================================
// Public API Routes (User Auth)
// ============================================================================

/// POST /api/wg/devices - Register a new WireGuard device
#[utoipa::path(
    post,
    path = "/api/wg/devices",
    request_body = RegisterDeviceRequest,
    responses(
        (status = 201, description = "Device registered", body = DeviceResponse),
        (status = 400, description = "Invalid request", body = crate::error::ErrorResponse),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 503, description = "WG tunnel not enabled", body = crate::error::ErrorResponse)
    ),
    tag = "wgtunnel",
    security(("api_key" = []))
)]
#[instrument(skip(state, current_user, request), fields(user_id = %current_user.user.id))]
pub async fn register_device(
	State(state): State<AppState>,
	RequireAuth(current_user): RequireAuth,
	Json(request): Json<RegisterDeviceRequest>,
) -> Result<impl IntoResponse, ServerError> {
	let services = state
		.wg_tunnel_services
		.as_ref()
		.ok_or_else(|| ServerError::ServiceUnavailable("WireGuard tunnel not enabled".to_string()))?;

	let public_key = parse_public_key(&request.public_key)?;
	let user_id: Uuid = current_user.user.id.into_inner();

	let device = services
		.device_service
		.register(user_id, public_key, request.name.clone())
		.await
		.map_err(wg_error_to_response)?;

	info!(device_id = %device.id, "Device registered");

	Ok((
		StatusCode::CREATED,
		Json(DeviceResponse {
			id: device.id.to_string(),
			public_key: device.public_key_base64(),
			name: device.name,
			created_at: device.created_at.to_rfc3339(),
			last_seen_at: device.last_seen_at.map(|t| t.to_rfc3339()),
		}),
	))
}

/// GET /api/wg/devices - List user's WireGuard devices
#[utoipa::path(
    get,
    path = "/api/wg/devices",
    responses(
        (status = 200, description = "List of devices", body = Vec<DeviceResponse>),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 503, description = "WG tunnel not enabled", body = crate::error::ErrorResponse)
    ),
    tag = "wgtunnel",
    security(("api_key" = []))
)]
#[instrument(skip(state, current_user), fields(user_id = %current_user.user.id))]
pub async fn list_devices(
	State(state): State<AppState>,
	RequireAuth(current_user): RequireAuth,
) -> Result<impl IntoResponse, ServerError> {
	let services = state
		.wg_tunnel_services
		.as_ref()
		.ok_or_else(|| ServerError::ServiceUnavailable("WireGuard tunnel not enabled".to_string()))?;

	let user_id: Uuid = current_user.user.id.into_inner();

	let devices = services
		.device_service
		.list(user_id)
		.await
		.map_err(wg_error_to_response)?;

	let response: Vec<DeviceResponse> = devices
		.into_iter()
		.map(|d| DeviceResponse {
			id: d.id.to_string(),
			public_key: d.public_key_base64(),
			name: d.name,
			created_at: d.created_at.to_rfc3339(),
			last_seen_at: d.last_seen_at.map(|t| t.to_rfc3339()),
		})
		.collect();

	Ok(Json(response))
}

/// DELETE /api/wg/devices/{id} - Revoke a device
#[utoipa::path(
    delete,
    path = "/api/wg/devices/{id}",
    params(
        ("id" = String, Path, description = "Device ID")
    ),
    responses(
        (status = 204, description = "Device revoked"),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 404, description = "Device not found", body = crate::error::ErrorResponse),
        (status = 503, description = "WG tunnel not enabled", body = crate::error::ErrorResponse)
    ),
    tag = "wgtunnel",
    security(("api_key" = []))
)]
#[instrument(skip(state, current_user), fields(user_id = %current_user.user.id, device_id = %id))]
pub async fn revoke_device(
	State(state): State<AppState>,
	RequireAuth(current_user): RequireAuth,
	Path(id): Path<String>,
) -> Result<impl IntoResponse, ServerError> {
	let services = state
		.wg_tunnel_services
		.as_ref()
		.ok_or_else(|| ServerError::ServiceUnavailable("WireGuard tunnel not enabled".to_string()))?;

	let device_id: Uuid = id
		.parse()
		.map_err(|_| ServerError::BadRequest(format!("Invalid device ID: {id}")))?;
	let user_id: Uuid = current_user.user.id.into_inner();

	services
		.device_service
		.revoke(device_id, user_id)
		.await
		.map_err(wg_error_to_response)?;

	info!(device_id = %id, "Device revoked");

	Ok(StatusCode::NO_CONTENT)
}

/// POST /api/wg/sessions - Create a tunnel session
#[utoipa::path(
    post,
    path = "/api/wg/sessions",
    request_body = loom_server_wgtunnel::CreateSessionApiRequest,
    responses(
        (status = 201, description = "Session created", body = SessionResponse),
        (status = 400, description = "Invalid request", body = crate::error::ErrorResponse),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 404, description = "Device or weaver not found", body = crate::error::ErrorResponse),
        (status = 503, description = "WG tunnel not enabled", body = crate::error::ErrorResponse)
    ),
    tag = "wgtunnel",
    security(("api_key" = []))
)]
#[instrument(skip(state, current_user, request), fields(user_id = %current_user.user.id))]
pub async fn create_session(
	State(state): State<AppState>,
	RequireAuth(current_user): RequireAuth,
	Json(request): Json<loom_server_wgtunnel::CreateSessionApiRequest>,
) -> Result<impl IntoResponse, ServerError> {
	let services = state
		.wg_tunnel_services
		.as_ref()
		.ok_or_else(|| ServerError::ServiceUnavailable("WireGuard tunnel not enabled".to_string()))?;

	let weaver_id: loom_server_weaver::WeaverId = request
		.weaver_id
		.parse()
		.map_err(|_| ServerError::BadRequest(format!("Invalid weaver ID: {}", request.weaver_id)))?;

	let provisioner = state.provisioner.as_ref().ok_or_else(|| {
		ServerError::ServiceUnavailable("Weaver provisioner not configured".to_string())
	})?;
	let weaver = provisioner.get_weaver(&weaver_id).await?;
	if weaver.owner_user_id != current_user.user.id.to_string() {
		return Err(ServerError::Forbidden(
			"You do not have access to this weaver".to_string(),
		));
	}

	let weaver_id_uuid: Uuid = request
		.weaver_id
		.parse()
		.map_err(|_| ServerError::BadRequest(format!("Invalid weaver ID: {}", request.weaver_id)))?;

	let user_id: Uuid = current_user.user.id.into_inner();
	let devices = services
		.device_service
		.list(user_id)
		.await
		.map_err(wg_error_to_response)?;

	let device = devices.first().ok_or_else(|| {
		ServerError::BadRequest("No registered device. Register a device first.".to_string())
	})?;

	let response = services
		.session_service
		.create(device.id, weaver_id_uuid)
		.await
		.map_err(wg_error_to_response)?;

	info!(session_id = %response.session_id, "Session created");

	Ok((
		StatusCode::CREATED,
		Json(SessionResponse {
			session_id: response.session_id.to_string(),
			client_ip: response.client_ip.to_string(),
			weaver_ip: response.weaver_ip.to_string(),
			weaver_public_key: response.weaver_public_key,
			derp_map: serde_json::to_value(&response.derp_map).unwrap_or_default(),
		}),
	))
}

/// GET /api/wg/sessions - List active sessions
#[utoipa::path(
    get,
    path = "/api/wg/sessions",
    responses(
        (status = 200, description = "List of sessions", body = Vec<SessionListItem>),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 503, description = "WG tunnel not enabled", body = crate::error::ErrorResponse)
    ),
    tag = "wgtunnel",
    security(("api_key" = []))
)]
#[instrument(skip(state, current_user), fields(user_id = %current_user.user.id))]
pub async fn list_sessions(
	State(state): State<AppState>,
	RequireAuth(current_user): RequireAuth,
) -> Result<impl IntoResponse, ServerError> {
	let services = state
		.wg_tunnel_services
		.as_ref()
		.ok_or_else(|| ServerError::ServiceUnavailable("WireGuard tunnel not enabled".to_string()))?;

	let user_id: Uuid = current_user.user.id.into_inner();
	let devices = services
		.device_service
		.list(user_id)
		.await
		.map_err(wg_error_to_response)?;

	let mut all_sessions = Vec::new();
	for device in devices {
		let sessions = services
			.session_service
			.list_for_device(device.id)
			.await
			.map_err(wg_error_to_response)?;

		for session in sessions {
			all_sessions.push(SessionListItem {
				session_id: session.id.to_string(),
				device_id: session.device_id.to_string(),
				weaver_id: session.weaver_id.to_string(),
				client_ip: session.client_ip.to_string(),
				created_at: session.created_at.to_rfc3339(),
				last_handshake_at: session.last_handshake_at.map(|t| t.to_rfc3339()),
			});
		}
	}

	Ok(Json(all_sessions))
}

/// DELETE /api/wg/sessions/{id} - Terminate a session
#[utoipa::path(
    delete,
    path = "/api/wg/sessions/{id}",
    params(
        ("id" = String, Path, description = "Session ID")
    ),
    responses(
        (status = 204, description = "Session terminated"),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 404, description = "Session not found", body = crate::error::ErrorResponse),
        (status = 503, description = "WG tunnel not enabled", body = crate::error::ErrorResponse)
    ),
    tag = "wgtunnel",
    security(("api_key" = []))
)]
#[instrument(skip(state, current_user), fields(user_id = %current_user.user.id, session_id = %id))]
pub async fn terminate_session(
	State(state): State<AppState>,
	RequireAuth(current_user): RequireAuth,
	Path(id): Path<String>,
) -> Result<impl IntoResponse, ServerError> {
	let services = state
		.wg_tunnel_services
		.as_ref()
		.ok_or_else(|| ServerError::ServiceUnavailable("WireGuard tunnel not enabled".to_string()))?;

	let session_id: Uuid = id
		.parse()
		.map_err(|_| ServerError::BadRequest(format!("Invalid session ID: {id}")))?;

	let session = services
		.session_service
		.get(session_id)
		.await
		.map_err(wg_error_to_response)?
		.ok_or_else(|| ServerError::NotFound("Session not found".to_string()))?;

	let user_id: Uuid = current_user.user.id.into_inner();
	let device = services
		.device_service
		.get(session.device_id)
		.await
		.map_err(wg_error_to_response)?
		.ok_or_else(|| ServerError::NotFound("Device not found".to_string()))?;

	if device.user_id != user_id && !current_user.user.is_system_admin() {
		return Err(ServerError::Forbidden(
			"Not authorized to terminate this session".to_string(),
		));
	}

	services
		.session_service
		.terminate(session_id)
		.await
		.map_err(wg_error_to_response)?;

	info!(session_id = %id, "Session terminated");

	Ok(StatusCode::NO_CONTENT)
}

/// GET /api/wg/derp-map - Get the DERP relay map
#[utoipa::path(
    get,
    path = "/api/wg/derp-map",
    responses(
        (status = 200, description = "DERP map"),
        (status = 401, description = "Unauthorized", body = crate::error::ErrorResponse),
        (status = 503, description = "WG tunnel not enabled", body = crate::error::ErrorResponse)
    ),
    tag = "wgtunnel",
    security(("api_key" = []))
)]
#[instrument(skip(state, _current_user))]
pub async fn get_derp_map(
	State(state): State<AppState>,
	RequireAuth(_current_user): RequireAuth,
) -> Result<impl IntoResponse, ServerError> {
	let services = state
		.wg_tunnel_services
		.as_ref()
		.ok_or_else(|| ServerError::ServiceUnavailable("WireGuard tunnel not enabled".to_string()))?;

	let derp_map = services
		.derp_service
		.get_derp_map()
		.await
		.map_err(wg_error_to_response)?;

	Ok(Json(derp_map))
}

// ============================================================================
// Internal API Routes (SVID Auth)
// ============================================================================

/// POST /internal/wg/weavers - Register a weaver's WireGuard key
#[utoipa::path(
    post,
    path = "/internal/wg/weavers",
    request_body = RegisterWeaverRequest,
    responses(
        (status = 201, description = "Weaver registered", body = RegisterWeaverResponse),
        (status = 400, description = "Invalid request", body = ErrorResponse),
        (status = 401, description = "Invalid SVID", body = ErrorResponse),
        (status = 503, description = "WG tunnel not enabled", body = ErrorResponse)
    ),
    tag = "wgtunnel-internal"
)]
#[instrument(skip(state, headers, request), fields(weaver_id = %request.weaver_id))]
pub async fn register_weaver(
	State(state): State<AppState>,
	headers: HeaderMap,
	Json(request): Json<RegisterWeaverRequest>,
) -> impl IntoResponse {
	let svid_issuer = match state.svid_issuer.as_ref() {
		Some(issuer) => issuer,
		None => {
			warn!("SVID issuer not configured");
			return (
				StatusCode::SERVICE_UNAVAILABLE,
				Json(ErrorResponse {
					error: "service_unavailable".to_string(),
					message: "SVID validation not configured".to_string(),
				}),
			)
				.into_response();
		}
	};

	let services = match state.wg_tunnel_services.as_ref() {
		Some(s) => s,
		None => {
			return (
				StatusCode::SERVICE_UNAVAILABLE,
				Json(ErrorResponse {
					error: "service_unavailable".to_string(),
					message: "WireGuard tunnel not enabled".to_string(),
				}),
			)
				.into_response();
		}
	};

	let token = match extract_bearer_token(&headers) {
		Ok(t) => t,
		Err(resp) => return resp.into_response(),
	};

	let claims = match svid_issuer.verify_svid(token).await {
		Ok(c) => c,
		Err(e) => {
			warn!(error = %e, "SVID validation failed");
			return (
				StatusCode::UNAUTHORIZED,
				Json(ErrorResponse {
					error: "svid_validation_failed".to_string(),
					message: "SVID validation failed".to_string(),
				}),
			)
				.into_response();
		}
	};

	if claims.weaver_id != request.weaver_id {
		return (
			StatusCode::FORBIDDEN,
			Json(ErrorResponse {
				error: "weaver_id_mismatch".to_string(),
				message: "SVID weaver_id does not match request".to_string(),
			}),
		)
			.into_response();
	}

	let weaver_id: Uuid = match request.weaver_id.parse() {
		Ok(id) => id,
		Err(_) => {
			return (
				StatusCode::BAD_REQUEST,
				Json(ErrorResponse {
					error: "invalid_weaver_id".to_string(),
					message: "Invalid weaver ID format".to_string(),
				}),
			)
				.into_response();
		}
	};

	let public_key = match parse_public_key(&request.public_key) {
		Ok(k) => k,
		Err(e) => {
			return (
				StatusCode::BAD_REQUEST,
				Json(ErrorResponse {
					error: "invalid_public_key".to_string(),
					message: e.to_string(),
				}),
			)
				.into_response();
		}
	};

	let weaver = match services
		.weaver_service
		.register(weaver_id, public_key, request.derp_home_region)
		.await
	{
		Ok(w) => w,
		Err(e) => {
			let (status, error, message) = match &e {
				WgError::WeaverAlreadyRegistered => {
					(StatusCode::CONFLICT, "already_registered", e.to_string())
				}
				WgError::IpAllocation(msg) => (
					StatusCode::INTERNAL_SERVER_ERROR,
					"ip_allocation_failed",
					msg.clone(),
				),
				_ => (
					StatusCode::INTERNAL_SERVER_ERROR,
					"internal_error",
					e.to_string(),
				),
			};
			return (
				status,
				Json(ErrorResponse {
					error: error.to_string(),
					message,
				}),
			)
				.into_response();
		}
	};

	info!(weaver_id = %weaver.weaver_id, assigned_ip = %weaver.assigned_ip, "Weaver registered for WG tunnel");

	let base_url = &state.base_url;

	(
		StatusCode::CREATED,
		Json(RegisterWeaverResponse {
			assigned_ip: weaver.assigned_ip.to_string(),
			derp_map_url: format!("{base_url}/api/wg/derp-map"),
			peers_stream_url: format!("{base_url}/internal/wg/weavers/{}/peers", weaver.weaver_id),
		}),
	)
		.into_response()
}

/// DELETE /internal/wg/weavers/{id} - Unregister a weaver
#[utoipa::path(
    delete,
    path = "/internal/wg/weavers/{id}",
    params(
        ("id" = String, Path, description = "Weaver ID")
    ),
    responses(
        (status = 204, description = "Weaver unregistered"),
        (status = 401, description = "Invalid SVID", body = ErrorResponse),
        (status = 404, description = "Weaver not found", body = ErrorResponse),
        (status = 503, description = "WG tunnel not enabled", body = ErrorResponse)
    ),
    tag = "wgtunnel-internal"
)]
#[instrument(skip(state, headers), fields(weaver_id = %id))]
pub async fn unregister_weaver(
	State(state): State<AppState>,
	headers: HeaderMap,
	Path(id): Path<String>,
) -> impl IntoResponse {
	let svid_issuer = match state.svid_issuer.as_ref() {
		Some(issuer) => issuer,
		None => {
			return (
				StatusCode::SERVICE_UNAVAILABLE,
				Json(ErrorResponse {
					error: "service_unavailable".to_string(),
					message: "SVID validation not configured".to_string(),
				}),
			)
				.into_response();
		}
	};

	let services = match state.wg_tunnel_services.as_ref() {
		Some(s) => s,
		None => {
			return (
				StatusCode::SERVICE_UNAVAILABLE,
				Json(ErrorResponse {
					error: "service_unavailable".to_string(),
					message: "WireGuard tunnel not enabled".to_string(),
				}),
			)
				.into_response();
		}
	};

	let token = match extract_bearer_token(&headers) {
		Ok(t) => t,
		Err(resp) => return resp.into_response(),
	};

	let claims = match svid_issuer.verify_svid(token).await {
		Ok(c) => c,
		Err(e) => {
			warn!(error = %e, "SVID validation failed");
			return (
				StatusCode::UNAUTHORIZED,
				Json(ErrorResponse {
					error: "svid_validation_failed".to_string(),
					message: "SVID validation failed".to_string(),
				}),
			)
				.into_response();
		}
	};

	if claims.weaver_id != id {
		return (
			StatusCode::FORBIDDEN,
			Json(ErrorResponse {
				error: "weaver_id_mismatch".to_string(),
				message: "SVID weaver_id does not match request".to_string(),
			}),
		)
			.into_response();
	}

	let weaver_id: Uuid = match id.parse() {
		Ok(id) => id,
		Err(_) => {
			return (
				StatusCode::BAD_REQUEST,
				Json(ErrorResponse {
					error: "invalid_weaver_id".to_string(),
					message: "Invalid weaver ID format".to_string(),
				}),
			)
				.into_response();
		}
	};

	services.peer_notifier.unregister(weaver_id).await;

	match services.weaver_service.unregister(weaver_id).await {
		Ok(()) => {
			info!(weaver_id = %id, "Weaver unregistered from WG tunnel");
			StatusCode::NO_CONTENT.into_response()
		}
		Err(WgError::WeaverNotFound) => (
			StatusCode::NOT_FOUND,
			Json(ErrorResponse {
				error: "not_found".to_string(),
				message: "Weaver not registered".to_string(),
			}),
		)
			.into_response(),
		Err(e) => (
			StatusCode::INTERNAL_SERVER_ERROR,
			Json(ErrorResponse {
				error: "internal_error".to_string(),
				message: e.to_string(),
			}),
		)
			.into_response(),
	}
}

/// GET /internal/wg/weavers/{id}/peers - SSE stream of peer updates
#[utoipa::path(
    get,
    path = "/internal/wg/weavers/{id}/peers",
    params(
        ("id" = String, Path, description = "Weaver ID")
    ),
    responses(
        (status = 200, description = "SSE stream of peer events"),
        (status = 401, description = "Invalid SVID", body = ErrorResponse),
        (status = 503, description = "WG tunnel not enabled", body = ErrorResponse)
    ),
    tag = "wgtunnel-internal"
)]
#[instrument(skip(state, headers), fields(weaver_id = %id))]
pub async fn stream_peers(
	State(state): State<AppState>,
	headers: HeaderMap,
	Path(id): Path<String>,
) -> impl IntoResponse {
	let svid_issuer = match state.svid_issuer.as_ref() {
		Some(issuer) => issuer,
		None => {
			return (
				StatusCode::SERVICE_UNAVAILABLE,
				Json(ErrorResponse {
					error: "service_unavailable".to_string(),
					message: "SVID validation not configured".to_string(),
				}),
			)
				.into_response();
		}
	};

	let services = match state.wg_tunnel_services.as_ref() {
		Some(s) => s.clone(),
		None => {
			return (
				StatusCode::SERVICE_UNAVAILABLE,
				Json(ErrorResponse {
					error: "service_unavailable".to_string(),
					message: "WireGuard tunnel not enabled".to_string(),
				}),
			)
				.into_response();
		}
	};

	let token = match extract_bearer_token(&headers) {
		Ok(t) => t.to_string(),
		Err(resp) => return resp.into_response(),
	};

	let claims = match svid_issuer.verify_svid(&token).await {
		Ok(c) => c,
		Err(e) => {
			warn!(error = %e, "SVID validation failed");
			return (
				StatusCode::UNAUTHORIZED,
				Json(ErrorResponse {
					error: "svid_validation_failed".to_string(),
					message: "SVID validation failed".to_string(),
				}),
			)
				.into_response();
		}
	};

	if claims.weaver_id != id {
		return (
			StatusCode::FORBIDDEN,
			Json(ErrorResponse {
				error: "weaver_id_mismatch".to_string(),
				message: "SVID weaver_id does not match request".to_string(),
			}),
		)
			.into_response();
	}

	let weaver_id: Uuid = match id.parse() {
		Ok(id) => id,
		Err(_) => {
			return (
				StatusCode::BAD_REQUEST,
				Json(ErrorResponse {
					error: "invalid_weaver_id".to_string(),
					message: "Invalid weaver ID format".to_string(),
				}),
			)
				.into_response();
		}
	};

	let rx = services.peer_notifier.subscribe(weaver_id).await;

	info!(weaver_id = %id, "Starting peer stream");

	let stream = peer_event_stream(rx);
	Sse::new(stream).into_response()
}

fn peer_event_stream(
	mut rx: tokio::sync::broadcast::Receiver<PeerEvent>,
) -> impl Stream<Item = Result<Event, Infallible>> {
	async_stream::stream! {
		loop {
			match rx.recv().await {
				Ok(event) => {
					let event_type = match &event {
						PeerEvent::PeerAdded { .. } => "peer_added",
						PeerEvent::PeerRemoved { .. } => "peer_removed",
					};
					match serde_json::to_string(&event) {
						Ok(data) => {
							yield Ok(Event::default().event(event_type).data(data));
						}
						Err(_) => {
							warn!(event_type = event_type, "Failed to serialize peer event");
						}
					}
				}
				Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
					warn!(lagged = n, "Peer stream lagged");
				}
				Err(tokio::sync::broadcast::error::RecvError::Closed) => {
					break;
				}
			}
		}
	}
}

/// GET /internal/wg/weavers/{id} - Get weaver info (for debugging)
#[utoipa::path(
    get,
    path = "/internal/wg/weavers/{id}",
    params(
        ("id" = String, Path, description = "Weaver ID")
    ),
    responses(
        (status = 200, description = "Weaver info", body = WeaverResponse),
        (status = 401, description = "Invalid SVID", body = ErrorResponse),
        (status = 404, description = "Weaver not found", body = ErrorResponse),
        (status = 503, description = "WG tunnel not enabled", body = ErrorResponse)
    ),
    tag = "wgtunnel-internal"
)]
#[instrument(skip(state, headers), fields(weaver_id = %id))]
pub async fn get_weaver(
	State(state): State<AppState>,
	headers: HeaderMap,
	Path(id): Path<String>,
) -> impl IntoResponse {
	let svid_issuer = match state.svid_issuer.as_ref() {
		Some(issuer) => issuer,
		None => {
			return (
				StatusCode::SERVICE_UNAVAILABLE,
				Json(ErrorResponse {
					error: "service_unavailable".to_string(),
					message: "SVID validation not configured".to_string(),
				}),
			)
				.into_response();
		}
	};

	let services = match state.wg_tunnel_services.as_ref() {
		Some(s) => s,
		None => {
			return (
				StatusCode::SERVICE_UNAVAILABLE,
				Json(ErrorResponse {
					error: "service_unavailable".to_string(),
					message: "WireGuard tunnel not enabled".to_string(),
				}),
			)
				.into_response();
		}
	};

	let token = match extract_bearer_token(&headers) {
		Ok(t) => t,
		Err(resp) => return resp.into_response(),
	};

	let claims = match svid_issuer.verify_svid(token).await {
		Ok(c) => c,
		Err(e) => {
			warn!(error = %e, "SVID validation failed");
			return (
				StatusCode::UNAUTHORIZED,
				Json(ErrorResponse {
					error: "svid_validation_failed".to_string(),
					message: "SVID validation failed".to_string(),
				}),
			)
				.into_response();
		}
	};

	if claims.weaver_id != id {
		return (
			StatusCode::FORBIDDEN,
			Json(ErrorResponse {
				error: "weaver_id_mismatch".to_string(),
				message: "SVID weaver_id does not match request".to_string(),
			}),
		)
			.into_response();
	}

	let weaver_id: Uuid = match id.parse() {
		Ok(id) => id,
		Err(_) => {
			return (
				StatusCode::BAD_REQUEST,
				Json(ErrorResponse {
					error: "invalid_weaver_id".to_string(),
					message: "Invalid weaver ID format".to_string(),
				}),
			)
				.into_response();
		}
	};

	match services.weaver_service.get(weaver_id).await {
		Ok(Some(w)) => (
			StatusCode::OK,
			Json(WeaverResponse {
				weaver_id: w.weaver_id.to_string(),
				public_key: w.public_key_base64(),
				assigned_ip: w.assigned_ip.to_string(),
				derp_home_region: w.derp_home_region,
				endpoint: w.endpoint,
				registered_at: w.registered_at.to_rfc3339(),
				last_seen_at: w.last_seen_at.map(|t| t.to_rfc3339()),
			}),
		)
			.into_response(),
		Ok(None) => (
			StatusCode::NOT_FOUND,
			Json(ErrorResponse {
				error: "not_found".to_string(),
				message: "Weaver not registered".to_string(),
			}),
		)
			.into_response(),
		Err(e) => (
			StatusCode::INTERNAL_SERVER_ERROR,
			Json(ErrorResponse {
				error: "internal_error".to_string(),
				message: e.to_string(),
			}),
		)
			.into_response(),
	}
}
