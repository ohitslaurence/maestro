// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! Authorization tests for crons monitoring routes.
//!
//! Key invariants:
//! - Ping endpoints are public (use ping_key for auth, not user session)
//! - API endpoints require user auth and org membership
//! - Invalid ping keys return 404
//! - Cross-org isolation: users cannot see monitors from other orgs

use axum::http::{Method, StatusCode};
use serde_json::json;

use super::support::{run_authz_cases, AuthzCase, TestApp};

// ============================================================================
// Helper: Create a monitor and return its details
// ============================================================================

async fn create_test_monitor(app: &TestApp, org_id: &str, slug: &str) -> (String, String) {
	// Returns (slug, ping_key)
	let response = app
		.post(
			"/api/crons/monitors",
			Some(&app.fixtures.org_a.member),
			json!({
				"org_id": org_id,
				"slug": slug,
				"name": format!("Test Monitor {}", slug),
				"schedule": {
					"type": "interval",
					"minutes": 60
				},
				"timezone": "UTC",
				"checkin_margin_minutes": 5
			}),
		)
		.await;

	assert_eq!(
		response.status(),
		StatusCode::CREATED,
		"Failed to create test monitor"
	);

	let (_, body) = response.into_parts();
	let body_bytes = axum::body::to_bytes(body, usize::MAX).await.unwrap();
	let result: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();

	let monitor = &result["monitor"];
	let ping_key = monitor["ping_key"].as_str().unwrap().to_string();
	let slug = monitor["slug"].as_str().unwrap().to_string();

	(slug, ping_key)
}

// ============================================================================
// Ping Endpoints (Public - No User Auth Required)
// ============================================================================

#[tokio::test]
async fn ping_success_with_valid_key() {
	let app = TestApp::new().await;
	let org_id = app.fixtures.org_a.org.id.to_string();

	let (_, ping_key) = create_test_monitor(&app, &org_id, "ping-test-1").await;

	// GET /ping/{key} should work without authentication
	let response = app.get(&format!("/ping/{}", ping_key), None).await;
	assert_eq!(
		response.status(),
		StatusCode::OK,
		"Valid ping key should succeed without auth"
	);
}

#[tokio::test]
async fn ping_start_with_valid_key() {
	let app = TestApp::new().await;
	let org_id = app.fixtures.org_a.org.id.to_string();

	let (_, ping_key) = create_test_monitor(&app, &org_id, "ping-start-test").await;

	// GET /ping/{key}/start should work without authentication
	let response = app.get(&format!("/ping/{}/start", ping_key), None).await;
	assert_eq!(
		response.status(),
		StatusCode::OK,
		"Valid ping key start should succeed without auth"
	);
}

#[tokio::test]
async fn ping_fail_with_valid_key() {
	let app = TestApp::new().await;
	let org_id = app.fixtures.org_a.org.id.to_string();

	let (_, ping_key) = create_test_monitor(&app, &org_id, "ping-fail-test").await;

	// GET /ping/{key}/fail should work without authentication
	let response = app.get(&format!("/ping/{}/fail", ping_key), None).await;
	assert_eq!(
		response.status(),
		StatusCode::OK,
		"Valid ping key fail should succeed without auth"
	);
}

#[tokio::test]
async fn ping_with_invalid_key_returns_not_found() {
	let app = TestApp::new().await;

	// Invalid ping key should return 404
	let response = app
		.get("/ping/00000000-0000-0000-0000-000000000000", None)
		.await;
	assert_eq!(
		response.status(),
		StatusCode::NOT_FOUND,
		"Invalid ping key should return 404"
	);
}

#[tokio::test]
async fn ping_start_with_invalid_key_returns_not_found() {
	let app = TestApp::new().await;

	let response = app
		.get("/ping/00000000-0000-0000-0000-000000000000/start", None)
		.await;
	assert_eq!(
		response.status(),
		StatusCode::NOT_FOUND,
		"Invalid ping key for start should return 404"
	);
}

#[tokio::test]
async fn ping_fail_with_invalid_key_returns_not_found() {
	let app = TestApp::new().await;

	let response = app
		.get("/ping/00000000-0000-0000-0000-000000000000/fail", None)
		.await;
	assert_eq!(
		response.status(),
		StatusCode::NOT_FOUND,
		"Invalid ping key for fail should return 404"
	);
}

// ============================================================================
// Monitor API Endpoints (Authenticated)
// ============================================================================

#[tokio::test]
async fn org_member_can_create_monitor() {
	let app = TestApp::new().await;
	let org_id = app.fixtures.org_a.org.id.to_string();

	let cases = vec![AuthzCase {
		name: "org_member_can_create_monitor",
		method: Method::POST,
		path: "/api/crons/monitors".to_string(),
		user: Some(app.fixtures.org_a.member.clone()),
		body: Some(json!({
			"org_id": org_id,
			"slug": "test-monitor-create",
			"name": "Test Monitor",
			"schedule": {
				"type": "cron",
				"expression": "0 * * * *"
			},
			"timezone": "UTC",
			"checkin_margin_minutes": 5
		})),
		expected_status: StatusCode::CREATED,
	}];

	run_authz_cases(&app, &cases).await;
}

#[tokio::test]
async fn unauthenticated_cannot_create_monitor() {
	let app = TestApp::new().await;
	let org_id = app.fixtures.org_a.org.id.to_string();

	let cases = vec![AuthzCase {
		name: "unauthenticated_cannot_create_monitor",
		method: Method::POST,
		path: "/api/crons/monitors".to_string(),
		user: None,
		body: Some(json!({
			"org_id": org_id,
			"slug": "test-monitor-unauth",
			"name": "Test Monitor",
			"schedule": {
				"type": "interval",
				"minutes": 60
			}
		})),
		expected_status: StatusCode::UNAUTHORIZED,
	}];

	run_authz_cases(&app, &cases).await;
}

#[tokio::test]
async fn org_member_can_list_monitors() {
	let app = TestApp::new().await;
	let org_id = app.fixtures.org_a.org.id.to_string();

	// Create a monitor first
	let _ = create_test_monitor(&app, &org_id, "list-test-monitor").await;

	let cases = vec![AuthzCase {
		name: "org_member_can_list_monitors",
		method: Method::GET,
		path: format!("/api/crons/monitors?org_id={}", org_id),
		user: Some(app.fixtures.org_a.member.clone()),
		body: None,
		expected_status: StatusCode::OK,
	}];

	run_authz_cases(&app, &cases).await;
}

#[tokio::test]
async fn unauthenticated_cannot_list_monitors() {
	let app = TestApp::new().await;
	let org_id = app.fixtures.org_a.org.id.to_string();

	let cases = vec![AuthzCase {
		name: "unauthenticated_cannot_list_monitors",
		method: Method::GET,
		path: format!("/api/crons/monitors?org_id={}", org_id),
		user: None,
		body: None,
		expected_status: StatusCode::UNAUTHORIZED,
	}];

	run_authz_cases(&app, &cases).await;
}

#[tokio::test]
async fn org_member_can_get_monitor() {
	let app = TestApp::new().await;
	let org_id = app.fixtures.org_a.org.id.to_string();

	let (slug, _) = create_test_monitor(&app, &org_id, "get-test-monitor").await;

	let cases = vec![AuthzCase {
		name: "org_member_can_get_monitor",
		method: Method::GET,
		path: format!("/api/crons/monitors/{}?org_id={}", slug, org_id),
		user: Some(app.fixtures.org_a.member.clone()),
		body: None,
		expected_status: StatusCode::OK,
	}];

	run_authz_cases(&app, &cases).await;
}

#[tokio::test]
async fn unauthenticated_cannot_get_monitor() {
	let app = TestApp::new().await;
	let org_id = app.fixtures.org_a.org.id.to_string();

	let (slug, _) = create_test_monitor(&app, &org_id, "get-unauth-test").await;

	let cases = vec![AuthzCase {
		name: "unauthenticated_cannot_get_monitor",
		method: Method::GET,
		path: format!("/api/crons/monitors/{}?org_id={}", slug, org_id),
		user: None,
		body: None,
		expected_status: StatusCode::UNAUTHORIZED,
	}];

	run_authz_cases(&app, &cases).await;
}

#[tokio::test]
async fn org_member_can_delete_monitor() {
	let app = TestApp::new().await;
	let org_id = app.fixtures.org_a.org.id.to_string();

	let (slug, _) = create_test_monitor(&app, &org_id, "delete-test-monitor").await;

	let cases = vec![AuthzCase {
		name: "org_member_can_delete_monitor",
		method: Method::DELETE,
		path: format!("/api/crons/monitors/{}?org_id={}", slug, org_id),
		user: Some(app.fixtures.org_a.member.clone()),
		body: None,
		expected_status: StatusCode::NO_CONTENT,
	}];

	run_authz_cases(&app, &cases).await;
}

#[tokio::test]
async fn unauthenticated_cannot_delete_monitor() {
	let app = TestApp::new().await;
	let org_id = app.fixtures.org_a.org.id.to_string();

	let (slug, _) = create_test_monitor(&app, &org_id, "delete-unauth-test").await;

	let cases = vec![AuthzCase {
		name: "unauthenticated_cannot_delete_monitor",
		method: Method::DELETE,
		path: format!("/api/crons/monitors/{}?org_id={}", slug, org_id),
		user: None,
		body: None,
		expected_status: StatusCode::UNAUTHORIZED,
	}];

	run_authz_cases(&app, &cases).await;
}

// ============================================================================
// Check-in API Endpoints (Authenticated)
// ============================================================================

#[tokio::test]
async fn org_member_can_list_checkins() {
	let app = TestApp::new().await;
	let org_id = app.fixtures.org_a.org.id.to_string();

	let (slug, ping_key) = create_test_monitor(&app, &org_id, "checkin-list-test").await;

	// Create a check-in via ping
	let _ = app.get(&format!("/ping/{}", ping_key), None).await;

	let cases = vec![AuthzCase {
		name: "org_member_can_list_checkins",
		method: Method::GET,
		path: format!("/api/crons/monitors/{}/checkins?org_id={}", slug, org_id),
		user: Some(app.fixtures.org_a.member.clone()),
		body: None,
		expected_status: StatusCode::OK,
	}];

	run_authz_cases(&app, &cases).await;
}

#[tokio::test]
async fn unauthenticated_cannot_list_checkins() {
	let app = TestApp::new().await;
	let org_id = app.fixtures.org_a.org.id.to_string();

	let (slug, _) = create_test_monitor(&app, &org_id, "checkin-unauth-list").await;

	let cases = vec![AuthzCase {
		name: "unauthenticated_cannot_list_checkins",
		method: Method::GET,
		path: format!("/api/crons/monitors/{}/checkins?org_id={}", slug, org_id),
		user: None,
		body: None,
		expected_status: StatusCode::UNAUTHORIZED,
	}];

	run_authz_cases(&app, &cases).await;
}

#[tokio::test]
async fn org_member_can_create_sdk_checkin() {
	let app = TestApp::new().await;
	let org_id = app.fixtures.org_a.org.id.to_string();

	let (slug, _) = create_test_monitor(&app, &org_id, "sdk-checkin-test").await;

	let cases = vec![AuthzCase {
		name: "org_member_can_create_sdk_checkin",
		method: Method::POST,
		path: format!("/api/crons/monitors/{}/checkins", slug),
		user: Some(app.fixtures.org_a.member.clone()),
		body: Some(json!({
			"org_id": org_id,
			"status": "ok"
		})),
		expected_status: StatusCode::CREATED,
	}];

	run_authz_cases(&app, &cases).await;
}

#[tokio::test]
async fn unauthenticated_cannot_create_sdk_checkin() {
	let app = TestApp::new().await;
	let org_id = app.fixtures.org_a.org.id.to_string();

	let (slug, _) = create_test_monitor(&app, &org_id, "sdk-checkin-unauth").await;

	let cases = vec![AuthzCase {
		name: "unauthenticated_cannot_create_sdk_checkin",
		method: Method::POST,
		path: format!("/api/crons/monitors/{}/checkins", slug),
		user: None,
		body: Some(json!({
			"org_id": org_id,
			"status": "ok"
		})),
		expected_status: StatusCode::UNAUTHORIZED,
	}];

	run_authz_cases(&app, &cases).await;
}

// ============================================================================
// Cross-Organization Isolation Tests
// ============================================================================

#[tokio::test]
async fn org_b_member_cannot_see_org_a_monitors() {
	let app = TestApp::new().await;
	let org_a_id = app.fixtures.org_a.org.id.to_string();

	let (slug, _) = create_test_monitor(&app, &org_a_id, "cross-org-test").await;

	// Org B member tries to get Org A's monitor - should return 403 (forbidden)
	let cases = vec![AuthzCase {
		name: "org_b_member_cannot_see_org_a_monitor",
		method: Method::GET,
		path: format!("/api/crons/monitors/{}?org_id={}", slug, org_a_id),
		user: Some(app.fixtures.org_b.member.clone()),
		body: None,
		// Returns 403 because org membership check fails
		expected_status: StatusCode::FORBIDDEN,
	}];

	run_authz_cases(&app, &cases).await;
}

#[tokio::test]
async fn org_b_member_cannot_delete_org_a_monitor() {
	let app = TestApp::new().await;
	let org_a_id = app.fixtures.org_a.org.id.to_string();

	let (slug, _) = create_test_monitor(&app, &org_a_id, "cross-org-delete").await;

	// Org B member tries to delete Org A's monitor - should return 403 (forbidden)
	let cases = vec![AuthzCase {
		name: "org_b_member_cannot_delete_org_a_monitor",
		method: Method::DELETE,
		path: format!("/api/crons/monitors/{}?org_id={}", slug, org_a_id),
		user: Some(app.fixtures.org_b.member.clone()),
		body: None,
		// Returns 403 because org membership check fails
		expected_status: StatusCode::FORBIDDEN,
	}];

	run_authz_cases(&app, &cases).await;
}

#[tokio::test]
async fn org_b_member_cannot_create_monitor_in_org_a() {
	let app = TestApp::new().await;
	let org_a_id = app.fixtures.org_a.org.id.to_string();

	// Org B member tries to create a monitor in Org A
	let cases = vec![AuthzCase {
		name: "org_b_member_cannot_create_monitor_in_org_a",
		method: Method::POST,
		path: "/api/crons/monitors".to_string(),
		user: Some(app.fixtures.org_b.member.clone()),
		body: Some(json!({
			"org_id": org_a_id,
			"slug": "sneaky-monitor",
			"name": "Sneaky Monitor",
			"schedule": {
				"type": "interval",
				"minutes": 60
			}
		})),
		// Should fail - org B member not allowed to create in org A
		expected_status: StatusCode::FORBIDDEN,
	}];

	run_authz_cases(&app, &cases).await;
}

// ============================================================================
// Invalid Slug Tests
// ============================================================================

#[tokio::test]
async fn invalid_slug_returns_bad_request() {
	let app = TestApp::new().await;
	let org_id = app.fixtures.org_a.org.id.to_string();

	let cases = vec![AuthzCase {
		name: "invalid_slug_returns_bad_request",
		method: Method::POST,
		path: "/api/crons/monitors".to_string(),
		user: Some(app.fixtures.org_a.member.clone()),
		body: Some(json!({
			"org_id": org_id,
			"slug": "invalid slug with spaces!",
			"name": "Test Monitor",
			"schedule": {
				"type": "interval",
				"minutes": 60
			}
		})),
		expected_status: StatusCode::BAD_REQUEST,
	}];

	run_authz_cases(&app, &cases).await;
}

#[tokio::test]
async fn duplicate_slug_returns_conflict() {
	let app = TestApp::new().await;
	let org_id = app.fixtures.org_a.org.id.to_string();

	// Create first monitor
	let _ = create_test_monitor(&app, &org_id, "duplicate-slug").await;

	// Try to create another with the same slug
	let cases = vec![AuthzCase {
		name: "duplicate_slug_returns_conflict",
		method: Method::POST,
		path: "/api/crons/monitors".to_string(),
		user: Some(app.fixtures.org_a.member.clone()),
		body: Some(json!({
			"org_id": org_id,
			"slug": "duplicate-slug",
			"name": "Duplicate Monitor",
			"schedule": {
				"type": "interval",
				"minutes": 60
			}
		})),
		expected_status: StatusCode::CONFLICT,
	}];

	run_authz_cases(&app, &cases).await;
}

#[tokio::test]
async fn nonexistent_monitor_returns_not_found() {
	let app = TestApp::new().await;
	let org_id = app.fixtures.org_a.org.id.to_string();

	let cases = vec![AuthzCase {
		name: "nonexistent_monitor_returns_not_found",
		method: Method::GET,
		path: format!("/api/crons/monitors/nonexistent-monitor?org_id={}", org_id),
		user: Some(app.fixtures.org_a.member.clone()),
		body: None,
		expected_status: StatusCode::NOT_FOUND,
	}];

	run_authz_cases(&app, &cases).await;
}

// ============================================================================
// Ping Endpoint with Exit Code Tests
// ============================================================================

#[tokio::test]
async fn ping_with_exit_code_zero_succeeds() {
	let app = TestApp::new().await;
	let org_id = app.fixtures.org_a.org.id.to_string();

	let (_, ping_key) = create_test_monitor(&app, &org_id, "exit-code-zero").await;

	let response = app
		.get(&format!("/ping/{}?exit_code=0", ping_key), None)
		.await;
	assert_eq!(
		response.status(),
		StatusCode::OK,
		"Ping with exit_code=0 should succeed"
	);
}

#[tokio::test]
async fn ping_with_nonzero_exit_code_records_failure() {
	let app = TestApp::new().await;
	let org_id = app.fixtures.org_a.org.id.to_string();

	let (_, ping_key) = create_test_monitor(&app, &org_id, "exit-code-nonzero").await;

	// Non-zero exit code should still return 200 (ping was received)
	// but internally marks the check-in as error status
	let response = app
		.get(&format!("/ping/{}?exit_code=1", ping_key), None)
		.await;
	assert_eq!(
		response.status(),
		StatusCode::OK,
		"Ping with exit_code=1 should succeed (records failure internally)"
	);
}

// ============================================================================
// SSE Stream Endpoint (Authenticated)
// ============================================================================

#[tokio::test]
async fn org_member_can_access_stream() {
	let app = TestApp::new().await;
	let org_id = app.fixtures.org_a.org.id.to_string();

	let cases = vec![AuthzCase {
		name: "org_member_can_access_stream",
		method: Method::GET,
		path: format!("/api/crons/stream?org_id={}", org_id),
		user: Some(app.fixtures.org_a.member.clone()),
		body: None,
		expected_status: StatusCode::OK,
	}];

	run_authz_cases(&app, &cases).await;
}

#[tokio::test]
async fn unauthenticated_cannot_access_stream() {
	let app = TestApp::new().await;
	let org_id = app.fixtures.org_a.org.id.to_string();

	let cases = vec![AuthzCase {
		name: "unauthenticated_cannot_access_stream",
		method: Method::GET,
		path: format!("/api/crons/stream?org_id={}", org_id),
		user: None,
		body: None,
		expected_status: StatusCode::UNAUTHORIZED,
	}];

	run_authz_cases(&app, &cases).await;
}

#[tokio::test]
async fn org_b_member_cannot_access_org_a_stream() {
	let app = TestApp::new().await;
	let org_a_id = app.fixtures.org_a.org.id.to_string();

	// Org B member tries to access Org A's stream
	let cases = vec![AuthzCase {
		name: "org_b_member_cannot_access_org_a_stream",
		method: Method::GET,
		path: format!("/api/crons/stream?org_id={}", org_a_id),
		user: Some(app.fixtures.org_b.member.clone()),
		body: None,
		// Should fail - org B member not allowed to access org A stream
		expected_status: StatusCode::FORBIDDEN,
	}];

	run_authz_cases(&app, &cases).await;
}
