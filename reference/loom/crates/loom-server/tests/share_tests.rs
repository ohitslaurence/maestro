// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! Integration tests for share link ownership checks.
//!
//! Tests verify IDOR (Insecure Direct Object Reference) protection:
//! - Users cannot create share links for threads they don't own
//! - Users cannot revoke share links for threads they don't own
//! - Thread owners can manage their own share links

use axum::{
	body::Body,
	http::{Request, StatusCode},
};
use chrono::Utc;
use loom_common_thread::Thread;
use loom_server::api::{create_app_state, create_router};
use loom_server::db::ThreadRepository;
use loom_server::ServerConfig;
use loom_server_auth::{generate_session_token, Session, SessionType, User, UserId};
use sha2::{Digest, Sha256};
use std::sync::Arc;
use tempfile::tempdir;
use tower::ServiceExt;

/// Hash a token using SHA-256 (same as auth_middleware)
fn hash_token(token: &str) -> String {
	let mut hasher = Sha256::new();
	hasher.update(token.as_bytes());
	hex::encode(hasher.finalize())
}

/// Creates a test app with isolated database and returns handles for direct DB access.
async fn setup_test_app() -> (
	axum::Router,
	Arc<ThreadRepository>,
	loom_server::db::UserRepository,
	loom_server::db::SessionRepository,
	tempfile::TempDir,
) {
	let dir = tempdir().unwrap();
	let db_path = dir.path().join("test_share.db");
	let db_url = format!("sqlite:{}?mode=rwc", db_path.display());
	let pool = loom_server::db::create_pool(&db_url).await.unwrap();
	loom_server::db::run_migrations(&pool).await.unwrap();
	let repo = Arc::new(ThreadRepository::new(pool.clone()));
	let config = ServerConfig::default();
	let state = create_app_state(pool.clone(), repo.clone(), &config, None).await;

	let user_repo = loom_server::db::UserRepository::new(pool.clone());
	let session_repo = loom_server::db::SessionRepository::new(pool);

	(create_router(state), repo, user_repo, session_repo, dir)
}

/// Creates a test user and session, returning the session token.
async fn create_test_user_with_session(
	user_repo: &loom_server::db::UserRepository,
	session_repo: &loom_server::db::SessionRepository,
	email: &str,
) -> (User, String) {
	let user = User {
		id: UserId::generate(),
		display_name: email.to_string(),
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
	};

	user_repo.create_user(&user).await.unwrap();

	let session = Session::new(user.id, SessionType::Web);
	let token = generate_session_token();
	let token_hash = hash_token(&token);

	session_repo
		.create_session(&session, &token_hash)
		.await
		.unwrap();

	(user, token)
}

/// Creates a test thread owned by the specified user.
async fn create_test_thread_with_owner(repo: &ThreadRepository, owner_user_id: &UserId) -> Thread {
	let thread = Thread::new();
	repo.upsert(&thread, None).await.unwrap();
	repo
		.set_owner_user_id(thread.id.as_str(), &owner_user_id.to_string())
		.await
		.unwrap();
	thread
}

// ============================================================================
// IDOR Prevention Tests - C7
// ============================================================================

#[tokio::test]
async fn test_cannot_create_share_link_for_others_thread() {
	let (app, repo, user_repo, session_repo, _dir) = setup_test_app().await;

	// Create User A (thread owner)
	let (user_a, _token_a) =
		create_test_user_with_session(&user_repo, &session_repo, "user-a@example.com").await;

	// Create User B (attacker)
	let (_user_b, token_b) =
		create_test_user_with_session(&user_repo, &session_repo, "user-b@example.com").await;

	// Create a thread owned by User A
	let thread = create_test_thread_with_owner(&repo, &user_a.id).await;

	// User B tries to create a share link for User A's thread
	let body = serde_json::json!({}).to_string();
	let response = app
		.oneshot(
			Request::builder()
				.uri(format!("/api/threads/{}/share", thread.id.as_str()))
				.method("POST")
				.header("content-type", "application/json")
				.header("cookie", format!("loom_session={token_b}"))
				.body(Body::from(body))
				.unwrap(),
		)
		.await
		.unwrap();

	assert_eq!(
		response.status(),
		StatusCode::FORBIDDEN,
		"Non-owner should get 403 Forbidden when creating share link for another user's thread"
	);

	let body = axum::body::to_bytes(response.into_body(), usize::MAX)
		.await
		.unwrap();
	let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
	assert_eq!(json["code"], "not_owner");
}

#[tokio::test]
async fn test_cannot_revoke_share_link_for_others_thread() {
	let (app, repo, user_repo, session_repo, _dir) = setup_test_app().await;

	// Create User A (thread owner)
	let (user_a, token_a) =
		create_test_user_with_session(&user_repo, &session_repo, "user-a@example.com").await;

	// Create User B (attacker)
	let (_user_b, token_b) =
		create_test_user_with_session(&user_repo, &session_repo, "user-b@example.com").await;

	// Create a thread owned by User A
	let thread = create_test_thread_with_owner(&repo, &user_a.id).await;

	// User A creates a share link (to have something to revoke)
	let body = serde_json::json!({}).to_string();
	let create_response = app
		.clone()
		.oneshot(
			Request::builder()
				.uri(format!("/api/threads/{}/share", thread.id.as_str()))
				.method("POST")
				.header("content-type", "application/json")
				.header("cookie", format!("loom_session={token_a}"))
				.body(Body::from(body))
				.unwrap(),
		)
		.await
		.unwrap();

	assert_eq!(
		create_response.status(),
		StatusCode::CREATED,
		"Owner should be able to create share link"
	);

	// User B tries to revoke the share link
	let response = app
		.oneshot(
			Request::builder()
				.uri(format!("/api/threads/{}/share", thread.id.as_str()))
				.method("DELETE")
				.header("cookie", format!("loom_session={token_b}"))
				.body(Body::empty())
				.unwrap(),
		)
		.await
		.unwrap();

	assert_eq!(
		response.status(),
		StatusCode::FORBIDDEN,
		"Non-owner should get 403 Forbidden when revoking share link for another user's thread"
	);

	let body = axum::body::to_bytes(response.into_body(), usize::MAX)
		.await
		.unwrap();
	let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
	assert_eq!(json["code"], "not_owner");
}

#[tokio::test]
async fn test_owner_can_manage_share_links() {
	let (app, repo, user_repo, session_repo, _dir) = setup_test_app().await;

	// Create User A (thread owner)
	let (user_a, token_a) =
		create_test_user_with_session(&user_repo, &session_repo, "user-a@example.com").await;

	// Create a thread owned by User A
	let thread = create_test_thread_with_owner(&repo, &user_a.id).await;

	// User A creates a share link
	let body = serde_json::json!({}).to_string();
	let create_response = app
		.clone()
		.oneshot(
			Request::builder()
				.uri(format!("/api/threads/{}/share", thread.id.as_str()))
				.method("POST")
				.header("content-type", "application/json")
				.header("cookie", format!("loom_session={token_a}"))
				.body(Body::from(body))
				.unwrap(),
		)
		.await
		.unwrap();

	assert_eq!(
		create_response.status(),
		StatusCode::CREATED,
		"Owner should be able to create share link"
	);

	let body = axum::body::to_bytes(create_response.into_body(), usize::MAX)
		.await
		.unwrap();
	let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
	assert!(json["url"].is_string(), "Response should contain share URL");

	// User A revokes the share link
	let revoke_response = app
		.oneshot(
			Request::builder()
				.uri(format!("/api/threads/{}/share", thread.id.as_str()))
				.method("DELETE")
				.header("cookie", format!("loom_session={token_a}"))
				.body(Body::empty())
				.unwrap(),
		)
		.await
		.unwrap();

	assert_eq!(
		revoke_response.status(),
		StatusCode::OK,
		"Owner should be able to revoke share link"
	);
}

#[tokio::test]
async fn test_unauthenticated_user_cannot_create_share_link() {
	let (app, repo, user_repo, session_repo, _dir) = setup_test_app().await;

	// Create a user and thread
	let (user_a, _token_a) =
		create_test_user_with_session(&user_repo, &session_repo, "user-a@example.com").await;
	let thread = create_test_thread_with_owner(&repo, &user_a.id).await;

	// Try to create share link without authentication
	let body = serde_json::json!({}).to_string();
	let response = app
		.oneshot(
			Request::builder()
				.uri(format!("/api/threads/{}/share", thread.id.as_str()))
				.method("POST")
				.header("content-type", "application/json")
				.body(Body::from(body))
				.unwrap(),
		)
		.await
		.unwrap();

	assert_eq!(
		response.status(),
		StatusCode::UNAUTHORIZED,
		"Unauthenticated user should get 401"
	);
}

#[tokio::test]
async fn test_invalid_token_cannot_create_share_link() {
	let (app, repo, user_repo, session_repo, _dir) = setup_test_app().await;

	// Create a user and thread
	let (user_a, _token_a) =
		create_test_user_with_session(&user_repo, &session_repo, "user-a@example.com").await;
	let thread = create_test_thread_with_owner(&repo, &user_a.id).await;

	// Try to create share link with invalid token
	let body = serde_json::json!({}).to_string();
	let response = app
		.oneshot(
			Request::builder()
				.uri(format!("/api/threads/{}/share", thread.id.as_str()))
				.method("POST")
				.header("content-type", "application/json")
				.header("cookie", "loom_session=invalid-token-12345")
				.body(Body::from(body))
				.unwrap(),
		)
		.await
		.unwrap();

	assert_eq!(
		response.status(),
		StatusCode::UNAUTHORIZED,
		"Invalid token should get 401"
	);
}
