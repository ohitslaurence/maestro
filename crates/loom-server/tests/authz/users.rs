// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! Authorization tests for user and session routes.

use axum::http::{Method, StatusCode};
use serde_json::json;

use super::support::{run_authz_cases, AuthzCase, TestApp};

// ============================================================================
// User Profile Tests
// ============================================================================

#[tokio::test]
async fn user_can_get_own_profile() {
	let app = TestApp::new().await;
	let owner = &app.fixtures.org_a.owner;

	let response = app
		.get(&format!("/api/users/{}", owner.user.id), Some(owner))
		.await;

	assert_eq!(
		response.status(),
		StatusCode::OK,
		"User should be able to get their own profile"
	);
}

#[tokio::test]
async fn user_can_get_other_profile() {
	let app = TestApp::new().await;
	let org_a_owner = &app.fixtures.org_a.owner;
	let org_b_owner = &app.fixtures.org_b.owner;

	let response = app
		.get(
			&format!("/api/users/{}", org_b_owner.user.id),
			Some(org_a_owner),
		)
		.await;

	assert_eq!(
		response.status(),
		StatusCode::OK,
		"User should be able to get another user's public profile"
	);
}

#[tokio::test]
async fn user_can_update_own_profile() {
	let app = TestApp::new().await;
	let owner = &app.fixtures.org_a.owner;

	let response = app
		.patch(
			"/api/users/me",
			Some(owner),
			json!({
				"display_name": "Updated Name"
			}),
		)
		.await;

	assert_eq!(
		response.status(),
		StatusCode::OK,
		"User should be able to update their own profile"
	);
}

// ============================================================================
// Locale Update Tests
// ============================================================================

#[tokio::test]
async fn user_can_update_locale() {
	let app = TestApp::new().await;
	let owner = &app.fixtures.org_a.owner;

	let response = app
		.patch(
			"/api/users/me",
			Some(owner),
			json!({
				"locale": "es"
			}),
		)
		.await;

	assert_eq!(
		response.status(),
		StatusCode::OK,
		"User should be able to update their locale"
	);

	let body = axum::body::to_bytes(response.into_body(), usize::MAX)
		.await
		.unwrap();
	let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

	assert_eq!(
		json["locale"], "es",
		"Response should contain the updated locale"
	);
}

#[tokio::test]
async fn user_cannot_set_invalid_locale() {
	let app = TestApp::new().await;
	let owner = &app.fixtures.org_a.owner;

	let response = app
		.patch(
			"/api/users/me",
			Some(owner),
			json!({
				"locale": "invalid_locale"
			}),
		)
		.await;

	assert_eq!(
		response.status(),
		StatusCode::BAD_REQUEST,
		"Setting an invalid locale should return 400"
	);
}

#[tokio::test]
async fn auth_me_returns_locale_after_update() {
	let app = TestApp::new().await;
	let owner = &app.fixtures.org_a.owner;

	// First update locale
	let update_response = app
		.patch(
			"/api/users/me",
			Some(owner),
			json!({
				"locale": "ja"
			}),
		)
		.await;
	assert_eq!(update_response.status(), StatusCode::OK);

	// Then verify /auth/me returns the updated locale
	let me_response = app.get("/auth/me", Some(owner)).await;
	assert_eq!(me_response.status(), StatusCode::OK);

	let body = axum::body::to_bytes(me_response.into_body(), usize::MAX)
		.await
		.unwrap();
	let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

	assert_eq!(
		json["locale"], "ja",
		"/auth/me should return the user's locale preference"
	);
}

// ============================================================================
// Account Deletion Tests
// ============================================================================

#[tokio::test]
async fn user_can_request_deletion() {
	let app = TestApp::new().await;
	let member = &app.fixtures.org_a.member;

	let response = app
		.post("/api/users/me/delete", Some(member), json!({}))
		.await;

	assert_eq!(
		response.status(),
		StatusCode::OK,
		"User should be able to request account deletion"
	);
}

#[tokio::test]
async fn user_can_restore_account() {
	let app = TestApp::new().await;
	let owner = &app.fixtures.org_a.owner;

	let restore_response = app
		.post("/api/users/me/restore", Some(owner), json!({}))
		.await;

	assert_eq!(
		restore_response.status(),
		StatusCode::NOT_FOUND,
		"User without pending deletion should get 404"
	);
}

#[tokio::test]
async fn restore_requires_reauth_after_deletion() {
	let app = TestApp::new().await;
	let member = &app.fixtures.org_a.member;

	let delete_response = app
		.post("/api/users/me/delete", Some(member), json!({}))
		.await;
	assert_eq!(delete_response.status(), StatusCode::OK);

	let restore_response = app
		.post("/api/users/me/restore", Some(member), json!({}))
		.await;

	assert_eq!(
		restore_response.status(),
		StatusCode::UNAUTHORIZED,
		"Session is invalidated after deletion request, requiring re-authentication to restore"
	);
}

// ============================================================================
// Session Tests
// ============================================================================

#[tokio::test]
async fn user_can_list_own_sessions() {
	let app = TestApp::new().await;
	let owner = &app.fixtures.org_a.owner;

	let response = app.get("/api/sessions", Some(owner)).await;

	assert_eq!(
		response.status(),
		StatusCode::OK,
		"User should be able to list their own sessions"
	);
}

#[tokio::test]
async fn sessions_scoped_to_user() {
	let app = TestApp::new().await;
	let org_a_owner = &app.fixtures.org_a.owner;
	let org_b_owner = &app.fixtures.org_b.owner;

	let response_a = app.get("/api/sessions", Some(org_a_owner)).await;
	let response_b = app.get("/api/sessions", Some(org_b_owner)).await;

	assert_eq!(response_a.status(), StatusCode::OK);
	assert_eq!(response_b.status(), StatusCode::OK);

	let body_a = axum::body::to_bytes(response_a.into_body(), usize::MAX)
		.await
		.unwrap();
	let body_b = axum::body::to_bytes(response_b.into_body(), usize::MAX)
		.await
		.unwrap();

	let json_a: serde_json::Value = serde_json::from_slice(&body_a).unwrap();
	let json_b: serde_json::Value = serde_json::from_slice(&body_b).unwrap();

	let sessions_a = json_a["sessions"].as_array().unwrap();
	let sessions_b = json_b["sessions"].as_array().unwrap();

	assert!(
		!sessions_a.is_empty(),
		"User A should have at least one session"
	);
	assert!(
		!sessions_b.is_empty(),
		"User B should have at least one session"
	);

	for session in sessions_a {
		for other_session in sessions_b {
			assert_ne!(
				session["id"], other_session["id"],
				"Sessions should be scoped to user - user A should not see user B's sessions"
			);
		}
	}
}

#[tokio::test]
async fn user_can_revoke_own_session() {
	let app = TestApp::new().await;
	let owner = &app.fixtures.org_a.owner;

	let list_response = app.get("/api/sessions", Some(owner)).await;
	assert_eq!(list_response.status(), StatusCode::OK);

	let body = axum::body::to_bytes(list_response.into_body(), usize::MAX)
		.await
		.unwrap();
	let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
	let sessions = json["sessions"].as_array().unwrap();

	assert!(
		!sessions.is_empty(),
		"User should have at least one session"
	);
	let session_id = sessions[0]["id"].as_str().unwrap();

	let revoke_response = app
		.delete(&format!("/api/sessions/{}", session_id), Some(owner))
		.await;

	assert_eq!(
		revoke_response.status(),
		StatusCode::OK,
		"User should be able to revoke their own session"
	);
}

#[tokio::test]
async fn user_cannot_revoke_other_session() {
	let app = TestApp::new().await;
	let org_a_owner = &app.fixtures.org_a.owner;
	let org_b_owner = &app.fixtures.org_b.owner;

	let list_response = app.get("/api/sessions", Some(org_b_owner)).await;
	assert_eq!(list_response.status(), StatusCode::OK);

	let body = axum::body::to_bytes(list_response.into_body(), usize::MAX)
		.await
		.unwrap();
	let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
	let sessions = json["sessions"].as_array().unwrap();

	assert!(
		!sessions.is_empty(),
		"User B should have at least one session"
	);
	let org_b_session_id = sessions[0]["id"].as_str().unwrap();

	let revoke_response = app
		.delete(
			&format!("/api/sessions/{}", org_b_session_id),
			Some(org_a_owner),
		)
		.await;

	assert!(
		revoke_response.status() == StatusCode::NOT_FOUND
			|| revoke_response.status() == StatusCode::FORBIDDEN,
		"User should not be able to revoke another user's session (expected 404 or 403, got {})",
		revoke_response.status()
	);
}

// ============================================================================
// Table-Driven Tests
// ============================================================================

#[tokio::test]
async fn user_routes_require_auth() {
	let app = TestApp::new().await;
	let user_id = app.fixtures.org_a.owner.user.id;

	let cases = vec![
		AuthzCase {
			name: "get_profile_requires_auth",
			method: Method::GET,
			path: format!("/api/users/{}", user_id),
			user: None,
			body: None,
			expected_status: StatusCode::UNAUTHORIZED,
		},
		AuthzCase {
			name: "list_sessions_requires_auth",
			method: Method::GET,
			path: "/api/sessions".to_string(),
			user: None,
			body: None,
			expected_status: StatusCode::UNAUTHORIZED,
		},
	];

	run_authz_cases(&app, &cases).await;
}
