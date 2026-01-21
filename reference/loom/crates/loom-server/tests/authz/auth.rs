// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! Authorization tests for auth routes including WS token endpoint.

use axum::http::StatusCode;

use super::support::TestApp;

// ============================================================================
// WS Token Tests
// ============================================================================

#[tokio::test]
async fn get_ws_token_requires_auth() {
	let app = TestApp::new().await;

	let response = app.get("/auth/ws-token", None).await;

	assert_eq!(
		response.status(),
		StatusCode::UNAUTHORIZED,
		"GET /auth/ws-token should require authentication"
	);
}

#[tokio::test]
async fn get_ws_token_returns_token_for_authenticated_user() {
	let app = TestApp::new().await;
	let owner = &app.fixtures.org_a.owner;

	let response = app.get("/auth/ws-token", Some(owner)).await;

	assert_eq!(
		response.status(),
		StatusCode::OK,
		"Authenticated user should be able to get WS token"
	);

	let body = axum::body::to_bytes(response.into_body(), usize::MAX)
		.await
		.unwrap();
	let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

	assert!(
		json.get("token").is_some(),
		"Response should contain token field"
	);
	assert!(
		json.get("expires_in").is_some(),
		"Response should contain expires_in field"
	);

	let token = json["token"].as_str().unwrap();
	assert!(
		token.starts_with("ws_"),
		"Token should have ws_ prefix, got: {}",
		token
	);

	let expires_in = json["expires_in"].as_i64().unwrap();
	assert_eq!(expires_in, 30, "Token should expire in 30 seconds");
}

#[tokio::test]
async fn ws_token_is_single_use() {
	let app = TestApp::new().await;
	let owner = &app.fixtures.org_a.owner;

	let response = app.get("/auth/ws-token", Some(owner)).await;
	assert_eq!(response.status(), StatusCode::OK);

	let body = axum::body::to_bytes(response.into_body(), usize::MAX)
		.await
		.unwrap();
	let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
	let token = json["token"].as_str().unwrap();

	let token_hash = {
		use sha2::{Digest, Sha256};
		let mut hasher = Sha256::new();
		hasher.update(token.as_bytes());
		hex::encode(hasher.finalize())
	};

	let first_result = app
		.state
		.session_repo
		.validate_and_consume_ws_token(&token_hash)
		.await
		.unwrap();
	assert!(
		first_result.is_some(),
		"First use of WS token should succeed"
	);

	let second_result = app
		.state
		.session_repo
		.validate_and_consume_ws_token(&token_hash)
		.await
		.unwrap();
	assert!(
		second_result.is_none(),
		"Second use of WS token should fail (single-use)"
	);
}

#[tokio::test]
async fn different_users_get_different_tokens() {
	let app = TestApp::new().await;
	let owner = &app.fixtures.org_a.owner;
	let member = &app.fixtures.org_a.member;

	let response1 = app.get("/auth/ws-token", Some(owner)).await;
	let response2 = app.get("/auth/ws-token", Some(member)).await;

	assert_eq!(response1.status(), StatusCode::OK);
	assert_eq!(response2.status(), StatusCode::OK);

	let body1 = axum::body::to_bytes(response1.into_body(), usize::MAX)
		.await
		.unwrap();
	let body2 = axum::body::to_bytes(response2.into_body(), usize::MAX)
		.await
		.unwrap();

	let json1: serde_json::Value = serde_json::from_slice(&body1).unwrap();
	let json2: serde_json::Value = serde_json::from_slice(&body2).unwrap();

	let token1 = json1["token"].as_str().unwrap();
	let token2 = json2["token"].as_str().unwrap();

	assert_ne!(
		token1, token2,
		"Different users should get different tokens"
	);
}

#[tokio::test]
async fn same_user_can_get_multiple_tokens() {
	let app = TestApp::new().await;
	let owner = &app.fixtures.org_a.owner;

	let response1 = app.get("/auth/ws-token", Some(owner)).await;
	let response2 = app.get("/auth/ws-token", Some(owner)).await;

	assert_eq!(response1.status(), StatusCode::OK);
	assert_eq!(response2.status(), StatusCode::OK);

	let body1 = axum::body::to_bytes(response1.into_body(), usize::MAX)
		.await
		.unwrap();
	let body2 = axum::body::to_bytes(response2.into_body(), usize::MAX)
		.await
		.unwrap();

	let json1: serde_json::Value = serde_json::from_slice(&body1).unwrap();
	let json2: serde_json::Value = serde_json::from_slice(&body2).unwrap();

	let token1 = json1["token"].as_str().unwrap();
	let token2 = json2["token"].as_str().unwrap();

	assert_ne!(
		token1, token2,
		"Same user should get different tokens on each request"
	);
}
