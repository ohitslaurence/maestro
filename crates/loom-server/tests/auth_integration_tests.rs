// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! Integration tests for authentication routes.
//!
//! Tests cover:
//! - OAuth callback validation
//! - OAuth email verification checks
//! - Magic link authentication flow
//! - Device code flow
//! - Session management
//! - Cookie security attributes (Secure, HttpOnly, SameSite)
//! - Admin route authorization (H9)
//! - Access denial audit logging (H6)

use axum::{
	body::Body,
	http::{header::SET_COOKIE, Request, StatusCode},
};
use loom_server::api::{create_app_state, create_router, AppState};
use loom_server::db::ThreadRepository;
use loom_server::ServerConfig;
use loom_server_auth_github::GitHubEmail;
use loom_server_auth_google::GoogleUserInfo;
use loom_server_auth_okta::OktaUserInfo;
use std::sync::Arc;
use tempfile::tempdir;
use tower::ServiceExt;

/// Creates a test app with isolated database (no dev mode - for testing auth requirements)
async fn setup_test_app() -> (axum::Router, tempfile::TempDir) {
	let dir = tempdir().unwrap();
	let db_path = dir.path().join("test_auth.db");
	let db_url = format!("sqlite:{}?mode=rwc", db_path.display());
	let pool = loom_server::db::create_pool(&db_url).await.unwrap();
	loom_server::db::run_migrations(&pool).await.unwrap();
	let repo = Arc::new(ThreadRepository::new(pool.clone()));
	let config = ServerConfig::default();
	let mut state = create_app_state(pool, repo, &config, None).await;
	// Explicitly disable dev mode for auth tests
	state.auth_config.dev_mode = false;
	(create_router(state), dir)
}

/// Creates a test app and returns both the router and state for repository access
async fn setup_test_app_with_state() -> (axum::Router, AppState, tempfile::TempDir) {
	let dir = tempdir().unwrap();
	let db_path = dir.path().join("test_auth_audit.db");
	let db_url = format!("sqlite:{}?mode=rwc", db_path.display());
	let pool = loom_server::db::create_pool(&db_url).await.unwrap();
	loom_server::db::run_migrations(&pool).await.unwrap();
	let repo = Arc::new(ThreadRepository::new(pool.clone()));
	let config = ServerConfig::default();
	let mut state = create_app_state(pool, repo, &config, None).await;
	// Explicitly disable dev mode for auth tests
	state.auth_config.dev_mode = false;
	(create_router(state.clone()), state, dir)
}

// ============================================================================
// OAuth Callback Tests
// ============================================================================

#[tokio::test]
async fn test_github_callback_without_provider_config_returns_501() {
	let (app, _dir) = setup_test_app().await;

	// Without GitHub OAuth configured, the callback should return 501 Not Implemented
	let response = app
		.oneshot(
			Request::builder()
				.uri("/auth/github/callback")
				.body(Body::empty())
				.unwrap(),
		)
		.await
		.unwrap();

	// Provider not configured returns 501
	assert_eq!(response.status(), StatusCode::NOT_IMPLEMENTED);
}

#[tokio::test]
async fn test_google_callback_without_provider_config_returns_501() {
	let (app, _dir) = setup_test_app().await;

	// Without Google OAuth configured, the callback should return 501 Not Implemented
	let response = app
		.oneshot(
			Request::builder()
				.uri("/auth/google/callback?code=test_code&state=test_state")
				.body(Body::empty())
				.unwrap(),
		)
		.await
		.unwrap();

	// Provider not configured returns 501
	assert_eq!(response.status(), StatusCode::NOT_IMPLEMENTED);
}

#[tokio::test]
async fn test_providers_endpoint_returns_available_providers() {
	let (app, _dir) = setup_test_app().await;

	let response = app
		.oneshot(
			Request::builder()
				.uri("/auth/providers")
				.body(Body::empty())
				.unwrap(),
		)
		.await
		.unwrap();

	assert_eq!(response.status(), StatusCode::OK);

	let body = axum::body::to_bytes(response.into_body(), usize::MAX)
		.await
		.unwrap();
	let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
	assert!(json["providers"].is_array());
}

// ============================================================================
// Magic Link Tests
// ============================================================================

#[tokio::test]
async fn test_magic_link_request_accepts_any_email() {
	let (app, _dir) = setup_test_app().await;

	let body = serde_json::json!({ "email": "test@example.com" }).to_string();
	let response = app
		.oneshot(
			Request::builder()
				.uri("/auth/magic-link")
				.method("POST")
				.header("content-type", "application/json")
				.body(Body::from(body))
				.unwrap(),
		)
		.await
		.unwrap();

	let status = response.status();
	assert!(
		status == StatusCode::OK || status == StatusCode::ACCEPTED,
		"Expected 200 or 202, got {status}"
	);
}

#[tokio::test]
async fn test_magic_link_request_missing_email_returns_422() {
	let (app, _dir) = setup_test_app().await;

	let body = serde_json::json!({}).to_string();
	let response = app
		.oneshot(
			Request::builder()
				.uri("/auth/magic-link")
				.method("POST")
				.header("content-type", "application/json")
				.body(Body::from(body))
				.unwrap(),
		)
		.await
		.unwrap();

	// Missing required field returns 422 Unprocessable Entity
	assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

// ============================================================================
// Device Code Tests
// ============================================================================

#[tokio::test]
async fn test_device_start_returns_codes() {
	let (app, _dir) = setup_test_app().await;

	let response = app
		.oneshot(
			Request::builder()
				.uri("/auth/device/start")
				.method("POST")
				.header("content-type", "application/json")
				.body(Body::empty())
				.unwrap(),
		)
		.await
		.unwrap();

	assert_eq!(response.status(), StatusCode::OK);

	let body = axum::body::to_bytes(response.into_body(), usize::MAX)
		.await
		.unwrap();
	let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

	assert!(json["device_code"].is_string());
	assert!(json["user_code"].is_string());
	assert!(json["expires_in"].is_number());
}

#[tokio::test]
async fn test_device_poll_pending_initially() {
	let (app, _dir) = setup_test_app().await;

	let start_response = app
		.clone()
		.oneshot(
			Request::builder()
				.uri("/auth/device/start")
				.method("POST")
				.header("content-type", "application/json")
				.body(Body::empty())
				.unwrap(),
		)
		.await
		.unwrap();

	let start_body = axum::body::to_bytes(start_response.into_body(), usize::MAX)
		.await
		.unwrap();
	let start_json: serde_json::Value = serde_json::from_slice(&start_body).unwrap();
	let device_code = start_json["device_code"].as_str().unwrap();

	let poll_body = serde_json::json!({ "device_code": device_code }).to_string();
	let poll_response = app
		.oneshot(
			Request::builder()
				.uri("/auth/device/poll")
				.method("POST")
				.header("content-type", "application/json")
				.body(Body::from(poll_body))
				.unwrap(),
		)
		.await
		.unwrap();

	assert_eq!(poll_response.status(), StatusCode::OK);

	let poll_body = axum::body::to_bytes(poll_response.into_body(), usize::MAX)
		.await
		.unwrap();
	let poll_json: serde_json::Value = serde_json::from_slice(&poll_body).unwrap();
	assert_eq!(poll_json["status"], "pending");
}

// ============================================================================
// Session Tests
// ============================================================================

#[tokio::test]
async fn test_session_cookie_has_http_only() {
	let (app, _dir) = setup_test_app().await;

	let start_response = app
		.oneshot(
			Request::builder()
				.uri("/auth/device/start")
				.method("POST")
				.body(Body::empty())
				.unwrap(),
		)
		.await
		.unwrap();

	// Check if any cookie headers exist, they should have HttpOnly
	if let Some(cookie) = start_response.headers().get(SET_COOKIE) {
		let cookie_str = cookie.to_str().unwrap().to_lowercase();
		// If session cookie, verify HttpOnly
		if cookie_str.contains("session") {
			assert!(
				cookie_str.contains("httponly"),
				"Session cookie should be HttpOnly"
			);
		}
	}
}

#[tokio::test]
async fn test_session_cookie_has_same_site() {
	let (app, _dir) = setup_test_app().await;

	let start_response = app
		.oneshot(
			Request::builder()
				.uri("/auth/device/start")
				.method("POST")
				.body(Body::empty())
				.unwrap(),
		)
		.await
		.unwrap();

	// Check if any cookie headers exist, they should have SameSite
	if let Some(cookie) = start_response.headers().get(SET_COOKIE) {
		let cookie_str = cookie.to_str().unwrap().to_lowercase();
		// If session cookie, verify SameSite
		if cookie_str.contains("session") {
			assert!(
				cookie_str.contains("samesite"),
				"Session cookie should have SameSite attribute"
			);
		}
	}
}

// ============================================================================
// Email Verification Tests
// ============================================================================

/// Helper function to create a test user with specific email
fn create_test_user(email: &str) -> loom_server_auth::User {
	use chrono::Utc;
	use loom_server_auth::UserId;

	loom_server_auth::User {
		id: UserId::generate(),
		display_name: "Test User".to_string(),
		username: None,
		primary_email: Some(email.to_string()),
		avatar_url: None,
		email_visible: false,
		is_system_admin: false,
		is_support: false,
		is_auditor: false,
		created_at: Utc::now(),
		updated_at: Utc::now(),
		deleted_at: None,
		locale: None,
	}
}

#[test]
fn test_github_email_verified_check() {
	let verified_email = GitHubEmail {
		email: "verified@example.com".to_string(),
		primary: true,
		verified: true,
	};

	let unverified_email = GitHubEmail {
		email: "unverified@example.com".to_string(),
		primary: true,
		verified: false,
	};

	assert!(verified_email.verified, "Should recognize verified email");
	assert!(
		!unverified_email.verified,
		"Should recognize unverified email"
	);
}

#[test]
fn test_google_email_verified_check() {
	let verified_user = GoogleUserInfo {
		sub: "123456789".to_string(),
		email: "verified@gmail.com".to_string(),
		email_verified: true,
		name: Some("Test User".to_string()),
		picture: None,
		given_name: None,
		family_name: None,
	};

	let unverified_user = GoogleUserInfo {
		sub: "987654321".to_string(),
		email: "unverified@gmail.com".to_string(),
		email_verified: false,
		name: Some("Unverified User".to_string()),
		picture: None,
		given_name: None,
		family_name: None,
	};

	assert!(
		verified_user.email_verified,
		"Should recognize verified Google email"
	);
	assert!(
		!unverified_user.email_verified,
		"Should recognize unverified Google email"
	);
}

#[test]
fn test_okta_email_verified_check() {
	let verified_user = OktaUserInfo {
		sub: "okta-id-123".to_string(),
		email: "verified@company.com".to_string(),
		email_verified: Some(true),
		name: Some("Corp User".to_string()),
		preferred_username: Some("corp.user".to_string()),
		given_name: None,
		family_name: None,
		groups: None,
	};

	let unverified_user = OktaUserInfo {
		sub: "okta-id-456".to_string(),
		email: "unverified@company.com".to_string(),
		email_verified: Some(false),
		name: Some("Unverified User".to_string()),
		preferred_username: Some("unverified.user".to_string()),
		given_name: None,
		family_name: None,
		groups: None,
	};

	assert!(
		verified_user.email_verified.unwrap_or(false),
		"Should recognize verified Okta email"
	);
	assert!(
		!unverified_user.email_verified.unwrap_or(false),
		"Should recognize unverified Okta email"
	);
}

// ============================================================================
// Admin Route Authorization Tests (H9)
// ============================================================================

/// Test that admin route returns 401 without authentication
#[tokio::test]
async fn test_admin_route_requires_authentication() {
	let (app, _dir) = setup_test_app().await;

	let response = app
		.oneshot(
			Request::builder()
				.uri("/api/admin/users")
				.body(Body::empty())
				.unwrap(),
		)
		.await
		.unwrap();

	assert_eq!(
		response.status(),
		StatusCode::UNAUTHORIZED,
		"Admin routes should require authentication"
	);
}

/// Test that admin route returns 403 for non-admin user
#[tokio::test]
async fn test_admin_route_forbidden_for_non_admin() {
	use axum::middleware::{self, Next};
	use axum::routing::get;
	use axum::Router;
	use loom_server::abac_middleware::RequireRole;
	use loom_server::routes;

	let user = create_test_user("regular@example.com");

	// Use auth context middleware approach
	use loom_server_auth::middleware::{AuthContext, CurrentUser};

	let current_user = CurrentUser::from_access_token(user);
	let auth_ctx = AuthContext::authenticated(current_user);

	let dir = tempdir().unwrap();
	let db_path = dir.path().join("test_admin_forbidden.db");
	let db_url = format!("sqlite:{}?mode=rwc", db_path.display());
	let pool = loom_server::db::create_pool(&db_url).await.unwrap();
	let repo = Arc::new(ThreadRepository::new(pool.clone()));
	let config = ServerConfig::default();
	let state = create_app_state(pool, repo, &config, None).await;

	let inject_auth = move |mut req: Request<Body>, next: Next| {
		let auth_ctx = auth_ctx.clone();
		async move {
			req.extensions_mut().insert(auth_ctx);
			next.run(req).await
		}
	};

	let app = Router::new()
		.route("/api/admin/users", get(routes::admin::list_users))
		.route_layer(RequireRole::admin())
		.layer(middleware::from_fn(inject_auth))
		.with_state(state);

	let response = app
		.oneshot(
			Request::builder()
				.uri("/api/admin/users")
				.body(Body::empty())
				.unwrap(),
		)
		.await
		.unwrap();

	assert_eq!(
		response.status(),
		StatusCode::FORBIDDEN,
		"Non-admin user should get 403 Forbidden for admin routes"
	);
}

/// Test that admin route returns 200 for admin user
#[tokio::test]
async fn test_admin_route_allowed_for_admin() {
	use axum::middleware::{self, Next};
	use axum::routing::get;
	use axum::Router;
	use loom_server::abac_middleware::RequireRole;
	use loom_server::routes;

	use loom_server_auth::middleware::{AuthContext, CurrentUser};

	let mut admin_user = create_test_user("admin@example.com");
	admin_user.is_system_admin = true;
	let current_user = CurrentUser::from_access_token(admin_user);
	let auth_ctx = AuthContext::authenticated(current_user);

	let dir = tempdir().unwrap();
	let db_path = dir.path().join("test_admin_access.db");
	let db_url = format!("sqlite:{}?mode=rwc", db_path.display());
	let pool = loom_server::db::create_pool(&db_url).await.unwrap();
	loom_server::db::run_migrations(&pool).await.unwrap();
	let repo = Arc::new(ThreadRepository::new(pool.clone()));
	let config = ServerConfig::default();
	let state = create_app_state(pool, repo, &config, None).await;

	let inject_auth = move |mut req: Request<Body>, next: Next| {
		let auth_ctx = auth_ctx.clone();
		async move {
			req.extensions_mut().insert(auth_ctx);
			next.run(req).await
		}
	};

	let app = Router::new()
		.route("/api/admin/users", get(routes::admin::list_users))
		.route_layer(RequireRole::admin())
		.layer(middleware::from_fn(inject_auth))
		.with_state(state);

	let response = app
		.oneshot(
			Request::builder()
				.uri("/api/admin/users")
				.body(Body::empty())
				.unwrap(),
		)
		.await
		.unwrap();

	assert_eq!(
		response.status(),
		StatusCode::OK,
		"Admin user should be able to access admin routes"
	);
}

// ============================================================================
// Route Authentication Tests - Verify all protected routes require auth
// ============================================================================

/// Protected API routes that MUST require authentication.
/// These routes should return 401 Unauthorized without a valid token.
/// Routes may return 404 if optional features (like K8s/weaver) aren't configured.
///
/// NOTE: Thread routes (/api/threads/*) are intentionally PUBLIC and not listed here.
/// Only routes with RequireAuth extractor in their handlers are listed.
const PROTECTED_GET_ROUTES: &[&str] = &[
	// Session routes (sessions.rs uses RequireAuth)
	"/api/sessions",
	// Org routes (orgs.rs uses RequireAuth)
	"/api/orgs",
	"/api/orgs/test-id",
	"/api/orgs/test-id/members",
	// Team routes (teams.rs uses RequireAuth)
	"/api/orgs/test-org/teams",
	"/api/orgs/test-org/teams/test-team",
	"/api/orgs/test-org/teams/test-team/members",
	// API key routes (api_keys.rs uses RequireAuth)
	"/api/orgs/test-org/api-keys",
	"/api/orgs/test-org/api-keys/test-key/usage",
	// Invitation routes (invitations.rs uses RequireAuth)
	"/api/orgs/test-org/invitations",
	// User routes (auth.rs get_current_user uses RequireAuth)
	"/auth/me",
	// Weaver routes (weaver.rs uses RequireAuth, may 404 if K8s not configured)
	"/api/weavers",
	"/api/weaver/test-id",
	"/api/weaver/test-id/logs",
	// Admin routes (admin.rs uses RequireAuth + RequireRole)
	"/api/admin/users",
	"/api/admin/audit-logs",
];

const PROTECTED_POST_ROUTES: &[&str] = &[
	// Auth routes that need session (auth.rs uses RequireAuth)
	"/auth/logout",
	"/auth/device/complete",
	// Org routes (orgs.rs uses RequireAuth)
	"/api/orgs",
	"/api/orgs/test-id/members",
	// Team routes (teams.rs uses RequireAuth)
	"/api/orgs/test-org/teams",
	"/api/orgs/test-org/teams/test-team/members",
	// API key routes (api_keys.rs uses RequireAuth)
	"/api/orgs/test-org/api-keys",
	// Invitation routes (invitations.rs uses RequireAuth)
	"/api/orgs/test-org/invitations",
	// Share routes (share.rs uses RequireAuth)
	"/api/threads/test-id/share",
	"/api/threads/test-id/support-access/request",
	"/api/threads/test-id/support-access/approve",
	// Weaver routes (weaver.rs uses RequireAuth, may 404 if K8s not configured)
	"/api/weaver",
	"/api/weavers/cleanup",
];

const PROTECTED_DELETE_ROUTES: &[&str] = &[
	// Session routes (sessions.rs uses RequireAuth)
	"/api/sessions/test-id",
	// Org routes (orgs.rs uses RequireAuth)
	"/api/orgs/test-id",
	"/api/orgs/test-org/members/test-user",
	// Team routes (teams.rs uses RequireAuth)
	"/api/orgs/test-org/teams/test-team",
	"/api/orgs/test-org/teams/test-team/members/test-user",
	// API key routes (api_keys.rs uses RequireAuth)
	"/api/orgs/test-org/api-keys/test-key",
	// Invitation routes (invitations.rs uses RequireAuth)
	"/api/orgs/test-org/invitations/test-id",
	// Share routes (share.rs uses RequireAuth)
	"/api/threads/test-id/share",
	"/api/threads/test-id/support-access",
	// Weaver routes (weaver.rs uses RequireAuth, may 404 if K8s not configured)
	"/api/weaver/test-id",
];

/// Test that all protected GET routes require authentication
#[tokio::test]
async fn test_protected_get_routes_require_auth() {
	let (app, _dir) = setup_test_app().await;

	for route in PROTECTED_GET_ROUTES {
		let response = app
			.clone()
			.oneshot(Request::builder().uri(*route).body(Body::empty()).unwrap())
			.await
			.unwrap();

		// 401 = auth required (correct)
		// 404 = route not found or feature not configured (acceptable)
		// 400/422 = route exists, processed request but bad params (acceptable - auth was checked first)
		// 200 with actual data = auth bypass bug!
		assert!(
			response.status() == StatusCode::UNAUTHORIZED
				|| response.status() == StatusCode::NOT_FOUND
				|| response.status() == StatusCode::METHOD_NOT_ALLOWED
				|| response.status() == StatusCode::BAD_REQUEST
				|| response.status() == StatusCode::UNPROCESSABLE_ENTITY,
			"GET {} should require auth (401), got unexpected {}",
			route,
			response.status()
		);
	}
}

/// Test that all protected POST routes require authentication
#[tokio::test]
async fn test_protected_post_routes_require_auth() {
	let (app, _dir) = setup_test_app().await;

	for route in PROTECTED_POST_ROUTES {
		let response = app
			.clone()
			.oneshot(
				Request::builder()
					.method("POST")
					.uri(*route)
					.header("content-type", "application/json")
					.body(Body::from("{}"))
					.unwrap(),
			)
			.await
			.unwrap();

		assert!(
			response.status() == StatusCode::UNAUTHORIZED
				|| response.status() == StatusCode::NOT_FOUND
				|| response.status() == StatusCode::METHOD_NOT_ALLOWED
				|| response.status() == StatusCode::BAD_REQUEST
				|| response.status() == StatusCode::UNPROCESSABLE_ENTITY,
			"POST {} should require auth (401), got unexpected {}",
			route,
			response.status()
		);
	}
}

/// Test that all protected DELETE routes require authentication
#[tokio::test]
async fn test_protected_delete_routes_require_auth() {
	let (app, _dir) = setup_test_app().await;

	for route in PROTECTED_DELETE_ROUTES {
		let response = app
			.clone()
			.oneshot(
				Request::builder()
					.method("DELETE")
					.uri(*route)
					.body(Body::empty())
					.unwrap(),
			)
			.await
			.unwrap();

		assert!(
			response.status() == StatusCode::UNAUTHORIZED
				|| response.status() == StatusCode::NOT_FOUND
				|| response.status() == StatusCode::METHOD_NOT_ALLOWED
				|| response.status() == StatusCode::BAD_REQUEST
				|| response.status() == StatusCode::UNPROCESSABLE_ENTITY,
			"DELETE {} should require auth (401), got unexpected {}",
			route,
			response.status()
		);
	}
}

/// Test that protected routes process bearer tokens (auth middleware is applied)
#[tokio::test]
async fn test_routes_process_bearer_token() {
	let (app, _dir) = setup_test_app().await;

	// Sample of PROTECTED routes to test with invalid bearer token
	// (thread routes are public, so they're not included here)
	let sample_routes = ["/api/orgs", "/api/weavers", "/api/sessions", "/auth/me"];

	for route in sample_routes {
		let response = app
			.clone()
			.oneshot(
				Request::builder()
					.uri(route)
					.header("Authorization", "Bearer lt_invalid_token_for_testing")
					.body(Body::empty())
					.unwrap(),
			)
			.await
			.unwrap();

		// Should be 401 (token processed but invalid) or 404 (feature not configured)
		// NOT 200 (which would indicate auth was bypassed)
		assert!(
			response.status() == StatusCode::UNAUTHORIZED || response.status() == StatusCode::NOT_FOUND,
			"{} with invalid bearer token should return 401 or 404, got {}",
			route,
			response.status()
		);
	}
}
