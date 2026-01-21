// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! Authorization tests for analytics routes.
//!
//! Key invariants:
//! - API key management routes require user auth and org membership
//! - SDK capture routes (capture, batch, identify, alias, set) accept Write or ReadWrite keys
//! - SDK query routes (persons, events) require ReadWrite keys
//! - Write keys cannot access query endpoints (403 Forbidden)
//! - Unauthenticated access to any endpoint returns 401

use axum::{
	body::Body,
	http::{header::HeaderName, header::HeaderValue, Method, Request, StatusCode},
};
use serde_json::json;
use tower::ServiceExt;

use super::support::{run_authz_cases, AuthzCase, TestApp};

// ============================================================================
// Helper: Create an API key and return it
// ============================================================================

async fn create_write_api_key(app: &TestApp, org_id: &str) -> String {
	let response = app
		.post(
			&format!("/api/orgs/{}/analytics/api-keys", org_id),
			Some(&app.fixtures.org_a.member),
			json!({
				"name": "test-write-key",
				"key_type": "write"
			}),
		)
		.await;

	assert_eq!(
		response.status(),
		StatusCode::CREATED,
		"Failed to create write API key"
	);

	let (_, body) = response.into_parts();
	let body_bytes = axum::body::to_bytes(body, usize::MAX).await.unwrap();
	let response: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
	response["key"].as_str().unwrap().to_string()
}

async fn create_read_write_api_key(app: &TestApp, org_id: &str) -> String {
	let response = app
		.post(
			&format!("/api/orgs/{}/analytics/api-keys", org_id),
			Some(&app.fixtures.org_a.member),
			json!({
				"name": "test-rw-key",
				"key_type": "read_write"
			}),
		)
		.await;

	assert_eq!(
		response.status(),
		StatusCode::CREATED,
		"Failed to create read_write API key"
	);

	let (_, body) = response.into_parts();
	let body_bytes = axum::body::to_bytes(body, usize::MAX).await.unwrap();
	let response: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
	response["key"].as_str().unwrap().to_string()
}

async fn request_with_api_key(
	app: &TestApp,
	method: Method,
	path: &str,
	api_key: &str,
	body: Option<serde_json::Value>,
) -> axum::response::Response<Body> {
	let mut builder = Request::builder().method(method).uri(path).header(
		HeaderName::from_static("authorization"),
		HeaderValue::from_str(&format!("Bearer {}", api_key)).unwrap(),
	);

	let request_body = match body {
		Some(b) => {
			builder = builder.header("content-type", "application/json");
			Body::from(serde_json::to_string(&b).unwrap())
		}
		None => Body::empty(),
	};

	let request = builder.body(request_body).unwrap();
	app.router.clone().oneshot(request).await.unwrap()
}

// ============================================================================
// API Key Management Routes (User Auth Required)
// ============================================================================

#[tokio::test]
async fn org_member_can_list_api_keys() {
	let app = TestApp::new().await;
	let org_id = app.fixtures.org_a.org.id.to_string();

	let cases = vec![AuthzCase {
		name: "org_member_can_list_api_keys",
		method: Method::GET,
		path: format!("/api/orgs/{}/analytics/api-keys", org_id),
		user: Some(app.fixtures.org_a.member.clone()),
		body: None,
		expected_status: StatusCode::OK,
	}];

	run_authz_cases(&app, &cases).await;
}

#[tokio::test]
async fn non_member_cannot_list_api_keys() {
	let app = TestApp::new().await;
	let org_id = app.fixtures.org_a.org.id.to_string();

	let cases = vec![AuthzCase {
		name: "non_member_cannot_list_api_keys",
		method: Method::GET,
		path: format!("/api/orgs/{}/analytics/api-keys", org_id),
		user: Some(app.fixtures.org_b.member.clone()),
		body: None,
		expected_status: StatusCode::NOT_FOUND,
	}];

	run_authz_cases(&app, &cases).await;
}

#[tokio::test]
async fn unauthenticated_cannot_list_api_keys() {
	let app = TestApp::new().await;
	let org_id = app.fixtures.org_a.org.id.to_string();

	let cases = vec![AuthzCase {
		name: "unauthenticated_cannot_list_api_keys",
		method: Method::GET,
		path: format!("/api/orgs/{}/analytics/api-keys", org_id),
		user: None,
		body: None,
		expected_status: StatusCode::UNAUTHORIZED,
	}];

	run_authz_cases(&app, &cases).await;
}

#[tokio::test]
async fn org_member_can_create_api_key() {
	let app = TestApp::new().await;
	let org_id = app.fixtures.org_a.org.id.to_string();

	let cases = vec![AuthzCase {
		name: "org_member_can_create_api_key",
		method: Method::POST,
		path: format!("/api/orgs/{}/analytics/api-keys", org_id),
		user: Some(app.fixtures.org_a.member.clone()),
		body: Some(json!({
			"name": "test-key",
			"key_type": "write"
		})),
		expected_status: StatusCode::CREATED,
	}];

	run_authz_cases(&app, &cases).await;
}

#[tokio::test]
async fn non_member_cannot_create_api_key() {
	let app = TestApp::new().await;
	let org_id = app.fixtures.org_a.org.id.to_string();

	let cases = vec![AuthzCase {
		name: "non_member_cannot_create_api_key",
		method: Method::POST,
		path: format!("/api/orgs/{}/analytics/api-keys", org_id),
		user: Some(app.fixtures.org_b.member.clone()),
		body: Some(json!({
			"name": "test-key",
			"key_type": "write"
		})),
		expected_status: StatusCode::NOT_FOUND,
	}];

	run_authz_cases(&app, &cases).await;
}

#[tokio::test]
async fn unauthenticated_cannot_create_api_key() {
	let app = TestApp::new().await;
	let org_id = app.fixtures.org_a.org.id.to_string();

	let cases = vec![AuthzCase {
		name: "unauthenticated_cannot_create_api_key",
		method: Method::POST,
		path: format!("/api/orgs/{}/analytics/api-keys", org_id),
		user: None,
		body: Some(json!({
			"name": "test-key",
			"key_type": "write"
		})),
		expected_status: StatusCode::UNAUTHORIZED,
	}];

	run_authz_cases(&app, &cases).await;
}

#[tokio::test]
async fn org_member_can_revoke_api_key() {
	let app = TestApp::new().await;
	let org_id = app.fixtures.org_a.org.id.to_string();

	// First create a key to revoke
	let response = app
		.post(
			&format!("/api/orgs/{}/analytics/api-keys", org_id),
			Some(&app.fixtures.org_a.member),
			json!({
				"name": "key-to-revoke",
				"key_type": "write"
			}),
		)
		.await;

	assert_eq!(response.status(), StatusCode::CREATED);
	let (_, body) = response.into_parts();
	let body_bytes = axum::body::to_bytes(body, usize::MAX).await.unwrap();
	let create_response: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
	let key_id = create_response["id"].as_str().unwrap();

	let cases = vec![AuthzCase {
		name: "org_member_can_revoke_api_key",
		method: Method::DELETE,
		path: format!("/api/orgs/{}/analytics/api-keys/{}", org_id, key_id),
		user: Some(app.fixtures.org_a.member.clone()),
		body: None,
		expected_status: StatusCode::OK,
	}];

	run_authz_cases(&app, &cases).await;
}

#[tokio::test]
async fn non_member_cannot_revoke_api_key() {
	let app = TestApp::new().await;
	let org_id = app.fixtures.org_a.org.id.to_string();

	// Create a key in org_a
	let response = app
		.post(
			&format!("/api/orgs/{}/analytics/api-keys", org_id),
			Some(&app.fixtures.org_a.member),
			json!({
				"name": "protected-key",
				"key_type": "write"
			}),
		)
		.await;

	assert_eq!(response.status(), StatusCode::CREATED);
	let (_, body) = response.into_parts();
	let body_bytes = axum::body::to_bytes(body, usize::MAX).await.unwrap();
	let create_response: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
	let key_id = create_response["id"].as_str().unwrap();

	// Try to revoke from org_b user
	let cases = vec![AuthzCase {
		name: "non_member_cannot_revoke_api_key",
		method: Method::DELETE,
		path: format!("/api/orgs/{}/analytics/api-keys/{}", org_id, key_id),
		user: Some(app.fixtures.org_b.member.clone()),
		body: None,
		expected_status: StatusCode::NOT_FOUND,
	}];

	run_authz_cases(&app, &cases).await;
}

// ============================================================================
// SDK Capture Routes (Write Key)
// ============================================================================

#[tokio::test]
async fn write_key_can_capture_event() {
	let app = TestApp::new().await;
	let org_id = app.fixtures.org_a.org.id.to_string();
	let api_key = create_write_api_key(&app, &org_id).await;

	let response = request_with_api_key(
		&app,
		Method::POST,
		"/api/analytics/capture",
		&api_key,
		Some(json!({
			"distinct_id": "user_123",
			"event": "test_event",
			"properties": {"foo": "bar"}
		})),
	)
	.await;

	assert_eq!(
		response.status(),
		StatusCode::OK,
		"Write key should be able to capture events"
	);
}

#[tokio::test]
async fn write_key_can_batch_capture() {
	let app = TestApp::new().await;
	let org_id = app.fixtures.org_a.org.id.to_string();
	let api_key = create_write_api_key(&app, &org_id).await;

	let response = request_with_api_key(
		&app,
		Method::POST,
		"/api/analytics/batch",
		&api_key,
		Some(json!({
			"batch": [
				{"distinct_id": "user_1", "event": "event_1", "properties": {}},
				{"distinct_id": "user_2", "event": "event_2", "properties": {}}
			]
		})),
	)
	.await;

	assert_eq!(
		response.status(),
		StatusCode::OK,
		"Write key should be able to batch capture events"
	);
}

#[tokio::test]
async fn write_key_can_identify() {
	let app = TestApp::new().await;
	let org_id = app.fixtures.org_a.org.id.to_string();
	let api_key = create_write_api_key(&app, &org_id).await;

	let response = request_with_api_key(
		&app,
		Method::POST,
		"/api/analytics/identify",
		&api_key,
		Some(json!({
			"distinct_id": "anon_123",
			"user_id": "user_456",
			"properties": {"name": "Test User"}
		})),
	)
	.await;

	assert_eq!(
		response.status(),
		StatusCode::OK,
		"Write key should be able to identify users"
	);
}

#[tokio::test]
async fn write_key_can_alias() {
	let app = TestApp::new().await;
	let org_id = app.fixtures.org_a.org.id.to_string();
	let api_key = create_write_api_key(&app, &org_id).await;

	let response = request_with_api_key(
		&app,
		Method::POST,
		"/api/analytics/alias",
		&api_key,
		Some(json!({
			"distinct_id": "user_123",
			"alias": "user_alias"
		})),
	)
	.await;

	assert_eq!(
		response.status(),
		StatusCode::OK,
		"Write key should be able to create aliases"
	);
}

#[tokio::test]
async fn write_key_can_set_properties() {
	let app = TestApp::new().await;
	let org_id = app.fixtures.org_a.org.id.to_string();
	let api_key = create_write_api_key(&app, &org_id).await;

	let response = request_with_api_key(
		&app,
		Method::POST,
		"/api/analytics/set",
		&api_key,
		Some(json!({
			"distinct_id": "user_123",
			"properties": {"plan": "enterprise"}
		})),
	)
	.await;

	assert_eq!(
		response.status(),
		StatusCode::OK,
		"Write key should be able to set person properties"
	);
}

// ============================================================================
// SDK Query Routes (Write Key CANNOT access)
// ============================================================================

#[tokio::test]
async fn write_key_cannot_list_persons() {
	let app = TestApp::new().await;
	let org_id = app.fixtures.org_a.org.id.to_string();
	let api_key = create_write_api_key(&app, &org_id).await;

	let response =
		request_with_api_key(&app, Method::GET, "/api/analytics/persons", &api_key, None).await;

	assert_eq!(
		response.status(),
		StatusCode::FORBIDDEN,
		"Write key should NOT be able to list persons"
	);
}

#[tokio::test]
async fn write_key_cannot_get_person() {
	let app = TestApp::new().await;
	let org_id = app.fixtures.org_a.org.id.to_string();
	let api_key = create_write_api_key(&app, &org_id).await;

	let response = request_with_api_key(
		&app,
		Method::GET,
		"/api/analytics/persons/some-person-id",
		&api_key,
		None,
	)
	.await;

	assert_eq!(
		response.status(),
		StatusCode::FORBIDDEN,
		"Write key should NOT be able to get person by ID"
	);
}

#[tokio::test]
async fn write_key_cannot_get_person_by_distinct_id() {
	let app = TestApp::new().await;
	let org_id = app.fixtures.org_a.org.id.to_string();
	let api_key = create_write_api_key(&app, &org_id).await;

	let response = request_with_api_key(
		&app,
		Method::GET,
		"/api/analytics/persons/by-distinct-id/user_123",
		&api_key,
		None,
	)
	.await;

	assert_eq!(
		response.status(),
		StatusCode::FORBIDDEN,
		"Write key should NOT be able to get person by distinct_id"
	);
}

#[tokio::test]
async fn write_key_cannot_list_events() {
	let app = TestApp::new().await;
	let org_id = app.fixtures.org_a.org.id.to_string();
	let api_key = create_write_api_key(&app, &org_id).await;

	let response =
		request_with_api_key(&app, Method::GET, "/api/analytics/events", &api_key, None).await;

	assert_eq!(
		response.status(),
		StatusCode::FORBIDDEN,
		"Write key should NOT be able to list events"
	);
}

#[tokio::test]
async fn write_key_cannot_count_events() {
	let app = TestApp::new().await;
	let org_id = app.fixtures.org_a.org.id.to_string();
	let api_key = create_write_api_key(&app, &org_id).await;

	let response = request_with_api_key(
		&app,
		Method::GET,
		"/api/analytics/events/count",
		&api_key,
		None,
	)
	.await;

	assert_eq!(
		response.status(),
		StatusCode::FORBIDDEN,
		"Write key should NOT be able to count events"
	);
}

#[tokio::test]
async fn write_key_cannot_export_events() {
	let app = TestApp::new().await;
	let org_id = app.fixtures.org_a.org.id.to_string();
	let api_key = create_write_api_key(&app, &org_id).await;

	let response = request_with_api_key(
		&app,
		Method::POST,
		"/api/analytics/events/export",
		&api_key,
		Some(json!({
			"limit": 100
		})),
	)
	.await;

	assert_eq!(
		response.status(),
		StatusCode::FORBIDDEN,
		"Write key should NOT be able to export events"
	);
}

// ============================================================================
// SDK Routes (ReadWrite Key)
// ============================================================================

#[tokio::test]
async fn read_write_key_can_capture_event() {
	let app = TestApp::new().await;
	let org_id = app.fixtures.org_a.org.id.to_string();
	let api_key = create_read_write_api_key(&app, &org_id).await;

	let response = request_with_api_key(
		&app,
		Method::POST,
		"/api/analytics/capture",
		&api_key,
		Some(json!({
			"distinct_id": "user_rw_123",
			"event": "test_event_rw",
			"properties": {"source": "read_write_key"}
		})),
	)
	.await;

	assert_eq!(
		response.status(),
		StatusCode::OK,
		"ReadWrite key should be able to capture events"
	);
}

#[tokio::test]
async fn read_write_key_can_list_persons() {
	let app = TestApp::new().await;
	let org_id = app.fixtures.org_a.org.id.to_string();
	let api_key = create_read_write_api_key(&app, &org_id).await;

	let response =
		request_with_api_key(&app, Method::GET, "/api/analytics/persons", &api_key, None).await;

	assert_eq!(
		response.status(),
		StatusCode::OK,
		"ReadWrite key should be able to list persons"
	);
}

#[tokio::test]
async fn read_write_key_can_list_events() {
	let app = TestApp::new().await;
	let org_id = app.fixtures.org_a.org.id.to_string();
	let api_key = create_read_write_api_key(&app, &org_id).await;

	let response =
		request_with_api_key(&app, Method::GET, "/api/analytics/events", &api_key, None).await;

	assert_eq!(
		response.status(),
		StatusCode::OK,
		"ReadWrite key should be able to list events"
	);
}

#[tokio::test]
async fn read_write_key_can_count_events() {
	let app = TestApp::new().await;
	let org_id = app.fixtures.org_a.org.id.to_string();
	let api_key = create_read_write_api_key(&app, &org_id).await;

	let response = request_with_api_key(
		&app,
		Method::GET,
		"/api/analytics/events/count",
		&api_key,
		None,
	)
	.await;

	assert_eq!(
		response.status(),
		StatusCode::OK,
		"ReadWrite key should be able to count events"
	);
}

#[tokio::test]
async fn read_write_key_can_export_events() {
	let app = TestApp::new().await;
	let org_id = app.fixtures.org_a.org.id.to_string();
	let api_key = create_read_write_api_key(&app, &org_id).await;

	let response = request_with_api_key(
		&app,
		Method::POST,
		"/api/analytics/events/export",
		&api_key,
		Some(json!({
			"limit": 100
		})),
	)
	.await;

	assert_eq!(
		response.status(),
		StatusCode::OK,
		"ReadWrite key should be able to export events"
	);
}

// ============================================================================
// Unauthenticated Access (SDK Routes)
// ============================================================================

#[tokio::test]
async fn unauthenticated_cannot_capture_event() {
	let app = TestApp::new().await;

	let response = request_with_api_key(
		&app,
		Method::POST,
		"/api/analytics/capture",
		"invalid_key",
		Some(json!({
			"distinct_id": "user_123",
			"event": "test_event",
			"properties": {}
		})),
	)
	.await;

	assert_eq!(
		response.status(),
		StatusCode::UNAUTHORIZED,
		"Invalid API key should return 401"
	);
}

#[tokio::test]
async fn missing_authorization_header_returns_unauthorized() {
	let app = TestApp::new().await;

	let request = Request::builder()
		.method(Method::POST)
		.uri("/api/analytics/capture")
		.header("content-type", "application/json")
		.body(Body::from(
			serde_json::to_string(&json!({
				"distinct_id": "user_123",
				"event": "test_event",
				"properties": {}
			}))
			.unwrap(),
		))
		.unwrap();

	let response = app.router.clone().oneshot(request).await.unwrap();

	assert_eq!(
		response.status(),
		StatusCode::UNAUTHORIZED,
		"Missing Authorization header should return 401"
	);
}

#[tokio::test]
async fn revoked_api_key_returns_unauthorized() {
	let app = TestApp::new().await;
	let org_id = app.fixtures.org_a.org.id.to_string();

	// Create and then revoke an API key
	let response = app
		.post(
			&format!("/api/orgs/{}/analytics/api-keys", org_id),
			Some(&app.fixtures.org_a.member),
			json!({
				"name": "key-to-revoke",
				"key_type": "write"
			}),
		)
		.await;

	assert_eq!(response.status(), StatusCode::CREATED);
	let (_, body) = response.into_parts();
	let body_bytes = axum::body::to_bytes(body, usize::MAX).await.unwrap();
	let create_response: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
	let key_id = create_response["id"].as_str().unwrap();
	let api_key = create_response["key"].as_str().unwrap();

	// Revoke the key
	let response = app
		.delete(
			&format!("/api/orgs/{}/analytics/api-keys/{}", org_id, key_id),
			Some(&app.fixtures.org_a.member),
		)
		.await;
	assert_eq!(response.status(), StatusCode::OK);

	// Try to use the revoked key
	let response = request_with_api_key(
		&app,
		Method::POST,
		"/api/analytics/capture",
		api_key,
		Some(json!({
			"distinct_id": "user_123",
			"event": "test_event",
			"properties": {}
		})),
	)
	.await;

	assert_eq!(
		response.status(),
		StatusCode::UNAUTHORIZED,
		"Revoked API key should return 401"
	);
}

// ============================================================================
// Cross-Org Access Tests
// ============================================================================

#[tokio::test]
async fn org_a_member_cannot_access_org_b_api_keys() {
	let app = TestApp::new().await;
	let org_b_id = app.fixtures.org_b.org.id.to_string();

	let cases = vec![
		AuthzCase {
			name: "org_a_member_cannot_list_org_b_api_keys",
			method: Method::GET,
			path: format!("/api/orgs/{}/analytics/api-keys", org_b_id),
			user: Some(app.fixtures.org_a.member.clone()),
			body: None,
			expected_status: StatusCode::NOT_FOUND,
		},
		AuthzCase {
			name: "org_a_member_cannot_create_org_b_api_key",
			method: Method::POST,
			path: format!("/api/orgs/{}/analytics/api-keys", org_b_id),
			user: Some(app.fixtures.org_a.member.clone()),
			body: Some(json!({
				"name": "sneaky-key",
				"key_type": "write"
			})),
			expected_status: StatusCode::NOT_FOUND,
		},
	];

	run_authz_cases(&app, &cases).await;
}

// ============================================================================
// Invalid Org ID Tests
// ============================================================================

#[tokio::test]
async fn invalid_org_id_returns_bad_request() {
	let app = TestApp::new().await;

	let cases = vec![
		AuthzCase {
			name: "invalid_org_id_for_list_api_keys",
			method: Method::GET,
			path: "/api/orgs/not-a-uuid/analytics/api-keys".to_string(),
			user: Some(app.fixtures.org_a.member.clone()),
			body: None,
			expected_status: StatusCode::BAD_REQUEST,
		},
		AuthzCase {
			name: "invalid_org_id_for_create_api_key",
			method: Method::POST,
			path: "/api/orgs/not-a-uuid/analytics/api-keys".to_string(),
			user: Some(app.fixtures.org_a.member.clone()),
			body: Some(json!({
				"name": "test-key",
				"key_type": "write"
			})),
			expected_status: StatusCode::BAD_REQUEST,
		},
	];

	run_authz_cases(&app, &cases).await;
}

// ============================================================================
// Cross-Organization Data Isolation Tests
// ============================================================================

/// Helper to create a read_write API key for a specific org with a specific user
async fn create_read_write_api_key_for_org(
	app: &TestApp,
	org_id: &str,
	user: &super::support::TestUser,
) -> String {
	let response = app
		.post(
			&format!("/api/orgs/{}/analytics/api-keys", org_id),
			Some(user),
			json!({
				"name": "test-rw-key",
				"key_type": "read_write"
			}),
		)
		.await;

	assert_eq!(
		response.status(),
		StatusCode::CREATED,
		"Failed to create read_write API key for org {}",
		org_id
	);

	let (_, body) = response.into_parts();
	let body_bytes = axum::body::to_bytes(body, usize::MAX).await.unwrap();
	let response: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
	response["key"].as_str().unwrap().to_string()
}

/// Tests that events captured by Org A are not visible to Org B's API key.
/// This is a critical security test ensuring data isolation between organizations.
#[tokio::test]
async fn org_b_api_key_cannot_see_org_a_events() {
	let app = TestApp::new().await;
	let org_a_id = app.fixtures.org_a.org.id.to_string();
	let org_b_id = app.fixtures.org_b.org.id.to_string();

	// Create API keys for both orgs
	let org_a_key =
		create_read_write_api_key_for_org(&app, &org_a_id, &app.fixtures.org_a.member).await;
	let org_b_key =
		create_read_write_api_key_for_org(&app, &org_b_id, &app.fixtures.org_b.member).await;

	// Capture an event with Org A's key using a unique event name
	// Use simple uuid format without hyphens since event names only allow alphanumeric, _, $, .
	let unique_event = format!("cross_org_isolation_test_{}", uuid::Uuid::new_v4().simple());
	let capture_response = request_with_api_key(
		&app,
		Method::POST,
		"/api/analytics/capture",
		&org_a_key,
		Some(json!({
			"distinct_id": "org_a_user",
			"event": unique_event,
			"properties": {"org": "a"}
		})),
	)
	.await;
	assert_eq!(
		capture_response.status(),
		StatusCode::OK,
		"Should be able to capture event"
	);

	// Query events with Org A's key - should see the event
	let org_a_response = request_with_api_key(
		&app,
		Method::GET,
		&format!("/api/analytics/events?event_name={}", unique_event),
		&org_a_key,
		None,
	)
	.await;
	assert_eq!(org_a_response.status(), StatusCode::OK);
	let (_, body) = org_a_response.into_parts();
	let body_bytes = axum::body::to_bytes(body, usize::MAX).await.unwrap();
	let response: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
	let org_a_events = response["events"].as_array().unwrap();
	assert_eq!(org_a_events.len(), 1, "Org A should see its own event");

	// Query events with Org B's key - should NOT see the event
	let org_b_response = request_with_api_key(
		&app,
		Method::GET,
		&format!("/api/analytics/events?event_name={}", unique_event),
		&org_b_key,
		None,
	)
	.await;
	assert_eq!(org_b_response.status(), StatusCode::OK);
	let (_, body) = org_b_response.into_parts();
	let body_bytes = axum::body::to_bytes(body, usize::MAX).await.unwrap();
	let response: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
	let org_b_events = response["events"].as_array().unwrap();
	assert_eq!(
		org_b_events.len(),
		0,
		"SECURITY: Org B must NOT see Org A's events"
	);
}

/// Tests that persons created by Org A are not accessible to Org B's API key.
#[tokio::test]
async fn org_b_api_key_cannot_see_org_a_persons() {
	let app = TestApp::new().await;
	let org_a_id = app.fixtures.org_a.org.id.to_string();
	let org_b_id = app.fixtures.org_b.org.id.to_string();

	// Create API keys for both orgs
	let org_a_key =
		create_read_write_api_key_for_org(&app, &org_a_id, &app.fixtures.org_a.member).await;
	let org_b_key =
		create_read_write_api_key_for_org(&app, &org_b_id, &app.fixtures.org_b.member).await;

	// Create a person in Org A via identify
	let unique_distinct_id = format!("org_a_user_{}", uuid::Uuid::new_v4());
	let identify_response = request_with_api_key(
		&app,
		Method::POST,
		"/api/analytics/identify",
		&org_a_key,
		Some(json!({
			"distinct_id": unique_distinct_id,
			"user_id": "test@example.com",
			"properties": {"org": "a"}
		})),
	)
	.await;
	assert_eq!(
		identify_response.status(),
		StatusCode::OK,
		"Should be able to identify"
	);
	let (_, body) = identify_response.into_parts();
	let body_bytes = axum::body::to_bytes(body, usize::MAX).await.unwrap();
	let response: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
	let person_id = response["person_id"].as_str().unwrap();

	// Get person by ID with Org A's key - should succeed
	let org_a_response = request_with_api_key(
		&app,
		Method::GET,
		&format!("/api/analytics/persons/{}", person_id),
		&org_a_key,
		None,
	)
	.await;
	assert_eq!(
		org_a_response.status(),
		StatusCode::OK,
		"Org A should access its own person"
	);

	// Get person by ID with Org B's key - should fail with 404
	let org_b_response = request_with_api_key(
		&app,
		Method::GET,
		&format!("/api/analytics/persons/{}", person_id),
		&org_b_key,
		None,
	)
	.await;
	assert_eq!(
		org_b_response.status(),
		StatusCode::NOT_FOUND,
		"SECURITY: Org B must NOT access Org A's person by ID"
	);

	// Get person by distinct_id with Org B's key - should fail with 404
	let org_b_response = request_with_api_key(
		&app,
		Method::GET,
		&format!(
			"/api/analytics/persons/by-distinct-id/{}",
			unique_distinct_id
		),
		&org_b_key,
		None,
	)
	.await;
	assert_eq!(
		org_b_response.status(),
		StatusCode::NOT_FOUND,
		"SECURITY: Org B must NOT access Org A's person by distinct_id"
	);
}
