// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! Session creation service for Loom authentication flows.
//!
//! This module consolidates session creation logic that was duplicated across
//! OAuth and magic link authentication handlers. It handles:
//!
//! - Session object creation with client metadata
//! - Token generation and hashing
//! - Database persistence
//! - Audit logging
//! - Cookie header formatting

use loom_server_audit::{AuditEventType, AuditLogBuilder, AuditService, UserId as AuditUserId};
use loom_server_auth::{generate_session_token, hash_token, Session, SessionType, UserId};
use loom_server_db::SessionRepository;
use std::sync::Arc;
use thiserror::Error;

const SESSION_MAX_AGE_SECONDS: i64 = 60 * 60 * 24 * 60; // 60 days

#[derive(Debug, Error)]
pub enum SessionError {
	#[error("failed to create session: {0}")]
	Database(#[from] loom_server_db::DbError),
}

pub type Result<T> = std::result::Result<T, SessionError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthMethod {
	GitHub,
	Google,
	Okta,
	MagicLink,
	DeviceCode,
}

impl AuthMethod {
	pub fn as_str(&self) -> &'static str {
		match self {
			Self::GitHub => "github",
			Self::Google => "google",
			Self::Okta => "okta",
			Self::MagicLink => "magic_link",
			Self::DeviceCode => "device_code",
		}
	}

	pub fn login_event_type(&self) -> AuditEventType {
		match self {
			Self::MagicLink => AuditEventType::MagicLinkUsed,
			_ => AuditEventType::Login,
		}
	}
}

impl std::fmt::Display for AuthMethod {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(f, "{}", self.as_str())
	}
}

#[derive(Debug, Clone, Default)]
pub struct ClientInfo {
	pub ip_address: Option<String>,
	pub user_agent: Option<String>,
	pub geo_city: Option<String>,
	pub geo_country: Option<String>,
}

#[derive(Debug)]
pub struct SessionRequest {
	pub user_id: UserId,
	pub session_type: SessionType,
	pub auth_method: AuthMethod,
	pub client_info: ClientInfo,
	pub email: Option<String>,
}

impl SessionRequest {
	pub fn new(user_id: UserId, auth_method: AuthMethod, client_info: ClientInfo) -> Self {
		Self {
			user_id,
			session_type: SessionType::Web,
			auth_method,
			client_info,
			email: None,
		}
	}

	pub fn with_session_type(mut self, session_type: SessionType) -> Self {
		self.session_type = session_type;
		self
	}

	pub fn with_email(mut self, email: impl Into<String>) -> Self {
		self.email = Some(email.into());
		self
	}
}

pub struct SessionResponse {
	pub session: Session,
	pub token: String,
	pub cookie_header: String,
}

pub struct SessionService {
	session_repo: Arc<SessionRepository>,
	audit_service: Arc<AuditService>,
	cookie_name: String,
}

impl SessionService {
	pub fn new(
		session_repo: Arc<SessionRepository>,
		audit_service: Arc<AuditService>,
		cookie_name: impl Into<String>,
	) -> Self {
		Self {
			session_repo,
			audit_service,
			cookie_name: cookie_name.into(),
		}
	}

	#[tracing::instrument(skip(self, request), fields(user_id = %request.user_id, auth_method = %request.auth_method))]
	pub async fn create_session(&self, request: SessionRequest) -> Result<SessionResponse> {
		let mut session = Session::new(request.user_id, request.session_type);
		if let Some(ip) = request.client_info.ip_address {
			session = session.with_ip(ip);
		}
		if let Some(ua) = request.client_info.user_agent {
			session = session.with_user_agent(ua);
		}
		session = session.with_geo(
			request.client_info.geo_city,
			request.client_info.geo_country,
		);

		let token = generate_session_token();
		let token_hash = hash_token(&token);

		self
			.session_repo
			.create_session(&session, &token_hash)
			.await?;

		let mut login_details = serde_json::json!({
			"provider": request.auth_method.as_str(),
		});
		if let Some(email) = &request.email {
			login_details["email"] = serde_json::json!(email);
		}

		self.audit_service.log(
			AuditLogBuilder::new(request.auth_method.login_event_type())
				.actor(AuditUserId::new(request.user_id.into_inner()))
				.details(login_details)
				.build(),
		);

		self.audit_service.log(
			AuditLogBuilder::new(AuditEventType::SessionCreated)
				.actor(AuditUserId::new(request.user_id.into_inner()))
				.resource("session", session.id.to_string())
				.details(serde_json::json!({
					"session_type": request.session_type.to_string().to_lowercase(),
					"auth_method": request.auth_method.as_str(),
				}))
				.build(),
		);

		let cookie_header = format!(
			"{}={}; Path=/; Max-Age={}; HttpOnly; Secure; SameSite=Lax",
			self.cookie_name, token, SESSION_MAX_AGE_SECONDS
		);

		tracing::info!(
			user_id = %request.user_id,
			session_id = %session.id,
			auth_method = %request.auth_method,
			"Session created"
		);

		Ok(SessionResponse {
			session,
			token,
			cookie_header,
		})
	}
}
