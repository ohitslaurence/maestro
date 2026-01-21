// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights
// reserved. SPDX-License-Identifier: Proprietary

//! WebSocket connection handler with first-message authentication.

use super::auth::{
	auth_timeout, close_code_for_error, AuthError, AuthMessage, AuthResponse, AuthenticatedContext,
	WebSocketAuthState,
};
use super::config;
use crate::AppState;
use axum::{
	extract::{
		ws::{CloseFrame, Message, WebSocket},
		Path, State, WebSocketUpgrade,
	},
	response::IntoResponse,
};
use chrono::Utc;
use futures::{SinkExt, StreamExt};
use loom_server_auth::{
	hash_token,
	middleware::{identify_bearer_token, BearerTokenType, CurrentUser},
};
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use tracing::{debug, error, info, warn};

#[derive(Debug, Clone, serde::Deserialize)]
pub struct WsQueryParams {
	pub protocol_version: Option<String>,
	pub features: Option<String>,
	pub reconnect_token: Option<String>,
}

#[derive(Clone)]
pub struct WebSocketConnection {
	session_id: String,
	auth_state: Arc<RwLock<WebSocketAuthState>>,
}

impl WebSocketConnection {
	pub fn new(session_id: String) -> Self {
		Self {
			session_id,
			auth_state: Arc::new(RwLock::new(WebSocketAuthState::Pending)),
		}
	}

	pub fn session_id(&self) -> &str {
		&self.session_id
	}

	pub async fn auth_state(&self) -> WebSocketAuthState {
		self.auth_state.read().await.clone()
	}

	pub async fn is_authenticated(&self) -> bool {
		self.auth_state.read().await.is_authenticated()
	}

	pub async fn is_closed(&self) -> bool {
		self.auth_state.read().await.is_closed()
	}

	pub async fn set_authenticated(&self, user: CurrentUser) {
		let ctx = AuthenticatedContext::new(user);
		*self.auth_state.write().await = WebSocketAuthState::Authenticated(Box::new(ctx));
	}

	pub async fn set_closed(&self) {
		*self.auth_state.write().await = WebSocketAuthState::Closed;
	}
}

#[utoipa::path(
    get,
    path = "/v1/ws/sessions/{session_id}",
    tag = "websocket",
    params(
        ("session_id" = String, Path, description = "Session ID for the WebSocket connection")
    ),
    responses(
        (status = 101, description = "WebSocket connection established"),
        (status = 400, description = "Bad request - invalid upgrade request"),
    )
)]
pub async fn ws_upgrade_handler(
	ws: WebSocketUpgrade,
	Path(session_id): Path<String>,
	State(state): State<AppState>,
) -> impl IntoResponse {
	info!(session_id = %session_id, "WebSocket upgrade request received (first-message auth required)");

	ws.on_upgrade(move |socket| handle_ws_connection(socket, session_id, state))
}

async fn handle_ws_connection(socket: WebSocket, session_id: String, state: AppState) {
	let (mut sender, mut receiver) = socket.split();
	let session_id_for_log = session_id.clone();

	let conn = Arc::new(WebSocketConnection::new(session_id.clone()));
	let (tx, mut rx) = mpsc::channel::<Message>(config::MAX_QUEUE_SIZE);

	let conn_send = conn.clone();
	let send_task = tokio::spawn(async move {
		while let Some(msg) = rx.recv().await {
			if let Err(e) = sender.send(msg).await {
				debug!(error = %e, "Failed to send WebSocket message");
				break;
			}
		}
		conn_send.set_closed().await;
	});

	let conn_recv = conn.clone();
	let session_repo = state.session_repo.clone();
	let api_key_repo = state.api_key_repo.clone();
	let user_repo = state.user_repo.clone();
	let tx_clone = tx.clone();
	let session_id_recv = session_id;
	let recv_task = tokio::spawn(async move {
		let auth_deadline = tokio::time::Instant::now() + auth_timeout();
		let mut ping_interval = tokio::time::interval(config::PING_INTERVAL);
		ping_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

		loop {
			if conn_recv.is_closed().await {
				break;
			}

			tokio::select! {
				_ = tokio::time::sleep_until(auth_deadline), if !conn_recv.is_authenticated().await && !conn_recv.is_closed().await => {
					warn!(session_id = %session_id_recv, "WebSocket auth timeout (5s)");
					let response = AuthResponse::error("authentication timeout");
					if let Ok(json) = serde_json::to_string(&response) {
						let _ = tx_clone.send(Message::Text(json.into())).await;
					}
					let close_frame = CloseFrame {
						code: close_code_for_error(&AuthError::Timeout),
						reason: "authentication timeout".into(),
					};
					let _ = tx_clone.send(Message::Close(Some(close_frame))).await;
					conn_recv.set_closed().await;
					break;
				}
				_ = ping_interval.tick() => {
					if conn_recv.is_authenticated().await {
						let ping_msg = serde_json::json!({
							"type": "ping",
							"timestamp": Utc::now().timestamp()
						});
						if let Ok(json) = serde_json::to_string(&ping_msg) {
							let _ = tx_clone.send(Message::Text(json.into())).await;
						}
					}
				}
				msg = receiver.next() => {
					match msg {
						Some(Ok(Message::Text(text))) => {
							let text_str: &str = &text;
							if let Err(e) = handle_message(
								text_str,
								&conn_recv,
								&tx_clone,
								&session_repo,
								&api_key_repo,
								&user_repo,
							).await {
								debug!(error = %e, "Error handling WebSocket message");
								if conn_recv.is_closed().await {
									break;
								}
							}
						}
						Some(Ok(Message::Binary(data))) => {
							if let Ok(text) = String::from_utf8(data.to_vec()) {
								if let Err(e) = handle_message(
									&text,
									&conn_recv,
									&tx_clone,
									&session_repo,
									&api_key_repo,
									&user_repo,
								).await {
									debug!(error = %e, "Error handling WebSocket binary message");
									if conn_recv.is_closed().await {
										break;
									}
								}
							}
						}
						Some(Ok(Message::Ping(data))) => {
							let _ = tx_clone.send(Message::Pong(data)).await;
							debug!("Received ping, sending pong");
						}
						Some(Ok(Message::Pong(_))) => {
							debug!("Received pong");
						}
						Some(Ok(Message::Close(_))) => {
							info!(session_id = %session_id_recv, "WebSocket close received");
							conn_recv.set_closed().await;
							break;
						}
						Some(Err(e)) => {
							debug!(error = %e, "WebSocket error");
							conn_recv.set_closed().await;
							break;
						}
						None => {
							info!(session_id = %session_id_recv, "WebSocket connection closed");
							conn_recv.set_closed().await;
							break;
						}
					}
				}
			}
		}
	});

	tokio::select! {
		_ = send_task => {}
		_ = recv_task => {}
	}

	info!(session_id = %session_id_for_log, "WebSocket connection terminated");
}

async fn handle_message(
	text: &str,
	conn: &Arc<WebSocketConnection>,
	tx: &mpsc::Sender<Message>,
	session_repo: &Arc<crate::db::SessionRepository>,
	api_key_repo: &Arc<crate::db::ApiKeyRepository>,
	user_repo: &Arc<crate::db::UserRepository>,
) -> Result<(), String> {
	let is_authenticated = conn.is_authenticated().await;

	if !is_authenticated {
		return handle_auth_message(text, conn, tx, session_repo, api_key_repo, user_repo).await;
	}

	handle_authenticated_message(text, conn, tx).await
}

async fn handle_auth_message(
	text: &str,
	conn: &Arc<WebSocketConnection>,
	tx: &mpsc::Sender<Message>,
	session_repo: &Arc<crate::db::SessionRepository>,
	api_key_repo: &Arc<crate::db::ApiKeyRepository>,
	user_repo: &Arc<crate::db::UserRepository>,
) -> Result<(), String> {
	let auth_msg: AuthMessage = match serde_json::from_str(text) {
		Ok(msg) => msg,
		Err(e) => {
			warn!(session_id = %conn.session_id(), error = %e, "Invalid auth message JSON");
			send_auth_error_and_close(tx, conn, AuthError::InvalidMessage).await;
			return Err("invalid auth message".to_string());
		}
	};

	if !auth_msg.is_valid() {
		warn!(session_id = %conn.session_id(), "Invalid auth message format");
		send_auth_error_and_close(tx, conn, AuthError::InvalidMessage).await;
		return Err("invalid auth message format".to_string());
	}

	if let Some(token) = auth_msg.token() {
		return handle_token_auth(token, conn, tx, session_repo, api_key_repo, user_repo).await;
	}

	if let Some(session_token) = auth_msg.session_token() {
		return handle_session_auth(session_token, conn, tx, session_repo, user_repo).await;
	}

	send_auth_error_and_close(tx, conn, AuthError::InvalidMessage).await;
	Err("no token provided".to_string())
}

async fn handle_token_auth(
	token: &str,
	conn: &Arc<WebSocketConnection>,
	tx: &mpsc::Sender<Message>,
	session_repo: &Arc<crate::db::SessionRepository>,
	api_key_repo: &Arc<crate::db::ApiKeyRepository>,
	user_repo: &Arc<crate::db::UserRepository>,
) -> Result<(), String> {
	let token_type = identify_bearer_token(token);

	let user_result = match token_type {
		BearerTokenType::AccessToken => validate_access_token(token, session_repo, user_repo).await,
		BearerTokenType::ApiKey => validate_api_key(token, api_key_repo, user_repo).await,
		BearerTokenType::WsToken => validate_ws_token(token, session_repo, user_repo).await,
		BearerTokenType::Unknown => {
			send_auth_error_and_close(tx, conn, AuthError::InvalidToken).await;
			return Err("unknown token type".to_string());
		}
	};

	match user_result {
		Some(user) => {
			info!(session_id = %conn.session_id(), user_id = %user.user.id, "WebSocket authenticated");
			conn.set_authenticated(user.clone()).await;
			let response = AuthResponse::ok(&user.user.id);
			if let Ok(json) = serde_json::to_string(&response) {
				let _ = tx.send(Message::Text(json.into())).await;
			}
			Ok(())
		}
		None => {
			let error = match token_type {
				BearerTokenType::AccessToken => AuthError::InvalidToken,
				BearerTokenType::ApiKey => AuthError::InvalidApiKey,
				BearerTokenType::WsToken => AuthError::InvalidToken,
				BearerTokenType::Unknown => AuthError::InvalidToken,
			};
			send_auth_error_and_close(tx, conn, error).await;
			Err("token validation failed".to_string())
		}
	}
}

async fn handle_session_auth(
	session_token: &str,
	conn: &Arc<WebSocketConnection>,
	tx: &mpsc::Sender<Message>,
	session_repo: &Arc<crate::db::SessionRepository>,
	user_repo: &Arc<crate::db::UserRepository>,
) -> Result<(), String> {
	match validate_session_token(session_token, session_repo, user_repo).await {
		Some(user) => {
			info!(session_id = %conn.session_id(), user_id = %user.user.id, "WebSocket authenticated via session");
			conn.set_authenticated(user.clone()).await;
			let response = AuthResponse::ok(&user.user.id);
			if let Ok(json) = serde_json::to_string(&response) {
				let _ = tx.send(Message::Text(json.into())).await;
			}
			Ok(())
		}
		None => {
			send_auth_error_and_close(tx, conn, AuthError::InvalidSession).await;
			Err("session validation failed".to_string())
		}
	}
}

async fn send_auth_error_and_close(
	tx: &mpsc::Sender<Message>,
	conn: &Arc<WebSocketConnection>,
	error: AuthError,
) {
	let response = AuthResponse::error(error.to_string());
	if let Ok(json) = serde_json::to_string(&response) {
		let _ = tx.send(Message::Text(json.into())).await;
	}
	let close_frame = CloseFrame {
		code: close_code_for_error(&error),
		reason: error.to_string().into(),
	};
	let _ = tx.send(Message::Close(Some(close_frame))).await;
	conn.set_closed().await;
}

async fn handle_authenticated_message(
	text: &str,
	conn: &Arc<WebSocketConnection>,
	tx: &mpsc::Sender<Message>,
) -> Result<(), String> {
	#[derive(serde::Deserialize)]
	struct MessageType {
		#[serde(rename = "type")]
		msg_type: String,
	}

	let msg_type: MessageType =
		serde_json::from_str(text).map_err(|e| format!("Invalid JSON: {e}"))?;

	match msg_type.msg_type.as_str() {
		"auth" => {
			let auth_state = conn.auth_state().await;
			if let Some(ctx) = auth_state.user() {
				let response = AuthResponse::ok(ctx.user_id());
				if let Ok(json) = serde_json::to_string(&response) {
					let _ = tx.send(Message::Text(json.into())).await;
				}
			}
			Ok(())
		}
		"query_response" => {
			debug!("Received query response via WebSocket");
			Ok(())
		}
		"pong" => {
			debug!("Received pong");
			Ok(())
		}
		"control" => {
			debug!("Received control message");
			Ok(())
		}
		_ => {
			debug!(msg_type = %msg_type.msg_type, "Received message");
			Ok(())
		}
	}
}

#[tracing::instrument(skip(session_token, session_repo, user_repo))]
async fn validate_session_token(
	session_token: &str,
	session_repo: &Arc<crate::db::SessionRepository>,
	user_repo: &Arc<crate::db::UserRepository>,
) -> Option<CurrentUser> {
	let token_hash = hash_token(session_token);

	let session = match session_repo.get_session_by_token_hash(&token_hash).await {
		Ok(Some(session)) => session,
		Ok(None) => {
			debug!("Session not found for token hash");
			return None;
		}
		Err(e) => {
			error!(error = %e, "Failed to look up session");
			return None;
		}
	};

	if session.expires_at < Utc::now() {
		debug!(session_id = %session.id, "Session expired");
		return None;
	}

	let user = match user_repo.get_user_by_id(&session.user_id).await {
		Ok(Some(user)) => user,
		Ok(None) => {
			warn!(user_id = %session.user_id, "User not found for valid session");
			return None;
		}
		Err(e) => {
			error!(error = %e, "Failed to look up user");
			return None;
		}
	};

	let session_id = session.id;
	let session_repo = session_repo.clone();
	tokio::spawn(async move {
		if let Err(e) = session_repo.update_session_last_used(&session_id).await {
			warn!(error = %e, "Failed to update session last used");
		}
	});

	Some(CurrentUser::from_session(user, session.id))
}

#[tracing::instrument(skip(token, session_repo, user_repo))]
async fn validate_access_token(
	token: &str,
	session_repo: &Arc<crate::db::SessionRepository>,
	user_repo: &Arc<crate::db::UserRepository>,
) -> Option<CurrentUser> {
	let token_hash = hash_token(token);

	let (token_id, user_id) = match session_repo.get_access_token_by_hash(&token_hash).await {
		Ok(Some((id, uid))) => (id, uid),
		Ok(None) => {
			debug!("Access token not found for token hash");
			return None;
		}
		Err(e) => {
			error!(error = %e, "Failed to look up access token");
			return None;
		}
	};

	let user = match user_repo.get_user_by_id(&user_id).await {
		Ok(Some(user)) => user,
		Ok(None) => {
			warn!(user_id = %user_id, "User not found for access token");
			return None;
		}
		Err(e) => {
			error!(error = %e, "Failed to look up user");
			return None;
		}
	};

	let session_repo = session_repo.clone();
	tokio::spawn(async move {
		if let Err(e) = session_repo.update_access_token_last_used(&token_id).await {
			warn!(error = %e, "Failed to update access token last used");
		}
	});

	Some(CurrentUser::from_access_token(user))
}

#[tracing::instrument(skip(token, api_key_repo, user_repo))]
async fn validate_api_key(
	token: &str,
	api_key_repo: &Arc<crate::db::ApiKeyRepository>,
	user_repo: &Arc<crate::db::UserRepository>,
) -> Option<CurrentUser> {
	let token_hash = hash_token(token);

	let api_key = match api_key_repo.get_api_key_by_hash(&token_hash).await {
		Ok(Some(key)) => key,
		Ok(None) => {
			debug!("API key not found for token hash");
			return None;
		}
		Err(e) => {
			error!(error = %e, "Failed to look up API key");
			return None;
		}
	};

	if api_key.revoked_at.is_some() {
		debug!(api_key_id = %api_key.id, "API key is revoked");
		return None;
	}

	let user = match user_repo.get_user_by_id(&api_key.created_by).await {
		Ok(Some(user)) => user,
		Ok(None) => {
			warn!(user_id = %api_key.created_by, "User not found for API key");
			return None;
		}
		Err(e) => {
			error!(error = %e, "Failed to look up user");
			return None;
		}
	};

	let api_key_id = api_key.id.to_string();
	let api_key_repo = api_key_repo.clone();
	tokio::spawn(async move {
		if let Err(e) = api_key_repo.update_last_used(&api_key_id).await {
			warn!(error = %e, "Failed to update API key last used");
		}
	});

	Some(CurrentUser::from_api_key(user, api_key.id.into_inner()))
}

#[tracing::instrument(skip(token, session_repo, user_repo))]
async fn validate_ws_token(
	token: &str,
	session_repo: &Arc<crate::db::SessionRepository>,
	user_repo: &Arc<crate::db::UserRepository>,
) -> Option<CurrentUser> {
	let token_hash = hash_token(token);

	let user_id = match session_repo
		.validate_and_consume_ws_token(&token_hash)
		.await
	{
		Ok(Some(uid)) => uid,
		Ok(None) => {
			debug!("WS token not found, expired, or already used");
			return None;
		}
		Err(e) => {
			error!(error = %e, "Failed to validate WS token");
			return None;
		}
	};

	let user = match user_repo.get_user_by_id(&user_id).await {
		Ok(Some(user)) => user,
		Ok(None) => {
			warn!(user_id = %user_id, "User not found for WS token");
			return None;
		}
		Err(e) => {
			error!(error = %e, "Failed to look up user");
			return None;
		}
	};

	Some(CurrentUser::from_access_token(user))
}

#[cfg(test)]
mod tests {
	use super::*;

	#[tokio::test]
	async fn test_websocket_connection_new() {
		let conn = WebSocketConnection::new("session-123".to_string());
		assert_eq!(conn.session_id(), "session-123");
		assert!(!conn.is_authenticated().await);
		assert!(!conn.is_closed().await);
	}

	#[tokio::test]
	async fn test_connection_state_transitions() {
		use chrono::Utc;
		use loom_server_auth::{Session, SessionType, User, UserId};

		let conn = WebSocketConnection::new("session-456".to_string());

		let state = conn.auth_state().await;
		assert!(state.is_pending());

		let user = User {
			id: UserId::generate(),
			display_name: "Test User".to_string(),
			username: None,
			primary_email: None,
			avatar_url: None,
			email_visible: true,
			is_system_admin: false,
			is_support: false,
			is_auditor: false,
			created_at: Utc::now(),
			updated_at: Utc::now(),
			deleted_at: None,
			locale: None,
		};
		let session = Session::new(user.id, SessionType::Web);
		let current_user = CurrentUser::from_session(user, session.id);

		conn.set_authenticated(current_user).await;
		assert!(conn.is_authenticated().await);
		assert!(!conn.is_closed().await);

		conn.set_closed().await;
		assert!(conn.is_closed().await);
	}

	mod query_params {
		use super::*;

		#[test]
		fn deserialize_full() {
			let json = r#"{"protocol_version":"3.0","features":"compression,ack"}"#;
			let params: WsQueryParams = serde_json::from_str(json).unwrap();
			assert_eq!(params.protocol_version, Some("3.0".to_string()));
			assert_eq!(params.features, Some("compression,ack".to_string()));
			assert!(params.reconnect_token.is_none());
		}

		#[test]
		fn deserialize_empty() {
			let json = r#"{}"#;
			let params: WsQueryParams = serde_json::from_str(json).unwrap();
			assert!(params.protocol_version.is_none());
			assert!(params.features.is_none());
			assert!(params.reconnect_token.is_none());
		}
	}
}
