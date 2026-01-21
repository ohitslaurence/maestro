// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! Authorization tests for feature flags routes.
//!
//! Key invariants:
//! - All org-level flag routes require org membership
//! - Non-members cannot access any flag resources
//! - Unauthenticated users cannot access any flag resources

use axum::http::{Method, StatusCode};
use serde_json::json;

use super::support::{run_authz_cases, AuthzCase, TestApp};

// ============================================================================
// Environment Routes
// ============================================================================

#[tokio::test]
async fn org_member_can_list_environments() {
	let app = TestApp::new().await;
	let org_id = app.fixtures.org_a.org.id.to_string();

	let cases = vec![AuthzCase {
		name: "org_member_can_list_environments",
		method: Method::GET,
		path: format!("/api/orgs/{}/flags/environments", org_id),
		user: Some(app.fixtures.org_a.member.clone()),
		body: None,
		expected_status: StatusCode::OK,
	}];

	run_authz_cases(&app, &cases).await;
}

#[tokio::test]
async fn non_member_cannot_list_environments() {
	let app = TestApp::new().await;
	let org_id = app.fixtures.org_a.org.id.to_string();

	let cases = vec![AuthzCase {
		name: "non_member_cannot_list_environments",
		method: Method::GET,
		path: format!("/api/orgs/{}/flags/environments", org_id),
		user: Some(app.fixtures.org_b.member.clone()),
		body: None,
		expected_status: StatusCode::NOT_FOUND,
	}];

	run_authz_cases(&app, &cases).await;
}

#[tokio::test]
async fn unauthenticated_cannot_list_environments() {
	let app = TestApp::new().await;
	let org_id = app.fixtures.org_a.org.id.to_string();

	let cases = vec![AuthzCase {
		name: "unauthenticated_cannot_list_environments",
		method: Method::GET,
		path: format!("/api/orgs/{}/flags/environments", org_id),
		user: None,
		body: None,
		expected_status: StatusCode::UNAUTHORIZED,
	}];

	run_authz_cases(&app, &cases).await;
}

#[tokio::test]
async fn org_member_can_create_environment() {
	let app = TestApp::new().await;
	let org_id = app.fixtures.org_a.org.id.to_string();

	let cases = vec![AuthzCase {
		name: "org_member_can_create_environment",
		method: Method::POST,
		path: format!("/api/orgs/{}/flags/environments", org_id),
		user: Some(app.fixtures.org_a.member.clone()),
		body: Some(json!({
			"name": "staging",
			"color": "#FFA500"
		})),
		expected_status: StatusCode::CREATED,
	}];

	run_authz_cases(&app, &cases).await;
}

#[tokio::test]
async fn non_member_cannot_create_environment() {
	let app = TestApp::new().await;
	let org_id = app.fixtures.org_a.org.id.to_string();

	let cases = vec![AuthzCase {
		name: "non_member_cannot_create_environment",
		method: Method::POST,
		path: format!("/api/orgs/{}/flags/environments", org_id),
		user: Some(app.fixtures.org_b.member.clone()),
		body: Some(json!({
			"name": "staging",
			"color": "#FFA500"
		})),
		expected_status: StatusCode::NOT_FOUND,
	}];

	run_authz_cases(&app, &cases).await;
}

// ============================================================================
// Flag Routes
// ============================================================================

#[tokio::test]
async fn org_member_can_list_flags() {
	let app = TestApp::new().await;
	let org_id = app.fixtures.org_a.org.id.to_string();

	let cases = vec![AuthzCase {
		name: "org_member_can_list_flags",
		method: Method::GET,
		path: format!("/api/orgs/{}/flags", org_id),
		user: Some(app.fixtures.org_a.member.clone()),
		body: None,
		expected_status: StatusCode::OK,
	}];

	run_authz_cases(&app, &cases).await;
}

#[tokio::test]
async fn non_member_cannot_list_flags() {
	let app = TestApp::new().await;
	let org_id = app.fixtures.org_a.org.id.to_string();

	let cases = vec![AuthzCase {
		name: "non_member_cannot_list_flags",
		method: Method::GET,
		path: format!("/api/orgs/{}/flags", org_id),
		user: Some(app.fixtures.org_b.member.clone()),
		body: None,
		expected_status: StatusCode::NOT_FOUND,
	}];

	run_authz_cases(&app, &cases).await;
}

#[tokio::test]
async fn unauthenticated_cannot_list_flags() {
	let app = TestApp::new().await;
	let org_id = app.fixtures.org_a.org.id.to_string();

	let cases = vec![AuthzCase {
		name: "unauthenticated_cannot_list_flags",
		method: Method::GET,
		path: format!("/api/orgs/{}/flags", org_id),
		user: None,
		body: None,
		expected_status: StatusCode::UNAUTHORIZED,
	}];

	run_authz_cases(&app, &cases).await;
}

#[tokio::test]
async fn org_member_can_create_flag() {
	let app = TestApp::new().await;
	let org_id = app.fixtures.org_a.org.id.to_string();

	let cases = vec![AuthzCase {
		name: "org_member_can_create_flag",
		method: Method::POST,
		path: format!("/api/orgs/{}/flags", org_id),
		user: Some(app.fixtures.org_a.member.clone()),
		body: Some(json!({
			"key": "feature.new_checkout",
			"name": "New Checkout Flow",
			"description": "Enable the new checkout flow",
			"tags": ["checkout", "experiment"],
			"variants": [
				{ "name": "off", "value": { "type": "Boolean", "value": false }, "weight": 100 },
				{ "name": "on", "value": { "type": "Boolean", "value": true }, "weight": 0 }
			],
			"default_variant": "off"
		})),
		expected_status: StatusCode::CREATED,
	}];

	run_authz_cases(&app, &cases).await;
}

#[tokio::test]
async fn non_member_cannot_create_flag() {
	let app = TestApp::new().await;
	let org_id = app.fixtures.org_a.org.id.to_string();

	let cases = vec![AuthzCase {
		name: "non_member_cannot_create_flag",
		method: Method::POST,
		path: format!("/api/orgs/{}/flags", org_id),
		user: Some(app.fixtures.org_b.member.clone()),
		body: Some(json!({
			"key": "feature.forbidden",
			"name": "Forbidden Flag",
			"variants": [
				{ "name": "off", "value": { "type": "Boolean", "value": false }, "weight": 100 }
			],
			"default_variant": "off"
		})),
		expected_status: StatusCode::NOT_FOUND,
	}];

	run_authz_cases(&app, &cases).await;
}

// ============================================================================
// Strategy Routes
// ============================================================================

#[tokio::test]
async fn org_member_can_list_strategies() {
	let app = TestApp::new().await;
	let org_id = app.fixtures.org_a.org.id.to_string();

	let cases = vec![AuthzCase {
		name: "org_member_can_list_strategies",
		method: Method::GET,
		path: format!("/api/orgs/{}/flags/strategies", org_id),
		user: Some(app.fixtures.org_a.member.clone()),
		body: None,
		expected_status: StatusCode::OK,
	}];

	run_authz_cases(&app, &cases).await;
}

#[tokio::test]
async fn non_member_cannot_list_strategies() {
	let app = TestApp::new().await;
	let org_id = app.fixtures.org_a.org.id.to_string();

	let cases = vec![AuthzCase {
		name: "non_member_cannot_list_strategies",
		method: Method::GET,
		path: format!("/api/orgs/{}/flags/strategies", org_id),
		user: Some(app.fixtures.org_b.member.clone()),
		body: None,
		expected_status: StatusCode::NOT_FOUND,
	}];

	run_authz_cases(&app, &cases).await;
}

#[tokio::test]
async fn org_member_can_create_strategy() {
	let app = TestApp::new().await;
	let org_id = app.fixtures.org_a.org.id.to_string();

	let cases = vec![AuthzCase {
		name: "org_member_can_create_strategy",
		method: Method::POST,
		path: format!("/api/orgs/{}/flags/strategies", org_id),
		user: Some(app.fixtures.org_a.member.clone()),
		body: Some(json!({
			"name": "Enterprise Users",
			"description": "Target enterprise plan users",
			"conditions": [
				{
					"type": "Attribute",
					"attribute": "plan",
					"operator": "equals",
					"value": "enterprise"
				}
			],
			"percentage": 100,
			"percentage_key": "user_id"
		})),
		expected_status: StatusCode::CREATED,
	}];

	run_authz_cases(&app, &cases).await;
}

#[tokio::test]
async fn non_member_cannot_create_strategy() {
	let app = TestApp::new().await;
	let org_id = app.fixtures.org_a.org.id.to_string();

	let cases = vec![AuthzCase {
		name: "non_member_cannot_create_strategy",
		method: Method::POST,
		path: format!("/api/orgs/{}/flags/strategies", org_id),
		user: Some(app.fixtures.org_b.member.clone()),
		body: Some(json!({
			"name": "Forbidden Strategy",
			"conditions": [],
			"percentage": 100,
			"percentage_key": "user_id"
		})),
		expected_status: StatusCode::NOT_FOUND,
	}];

	run_authz_cases(&app, &cases).await;
}

// ============================================================================
// Kill Switch Routes
// ============================================================================

#[tokio::test]
async fn org_member_can_list_kill_switches() {
	let app = TestApp::new().await;
	let org_id = app.fixtures.org_a.org.id.to_string();

	let cases = vec![AuthzCase {
		name: "org_member_can_list_kill_switches",
		method: Method::GET,
		path: format!("/api/orgs/{}/flags/kill-switches", org_id),
		user: Some(app.fixtures.org_a.member.clone()),
		body: None,
		expected_status: StatusCode::OK,
	}];

	run_authz_cases(&app, &cases).await;
}

#[tokio::test]
async fn non_member_cannot_list_kill_switches() {
	let app = TestApp::new().await;
	let org_id = app.fixtures.org_a.org.id.to_string();

	let cases = vec![AuthzCase {
		name: "non_member_cannot_list_kill_switches",
		method: Method::GET,
		path: format!("/api/orgs/{}/flags/kill-switches", org_id),
		user: Some(app.fixtures.org_b.member.clone()),
		body: None,
		expected_status: StatusCode::NOT_FOUND,
	}];

	run_authz_cases(&app, &cases).await;
}

#[tokio::test]
async fn org_member_can_create_kill_switch() {
	let app = TestApp::new().await;
	let org_id = app.fixtures.org_a.org.id.to_string();

	let cases = vec![AuthzCase {
		name: "org_member_can_create_kill_switch",
		method: Method::POST,
		path: format!("/api/orgs/{}/flags/kill-switches", org_id),
		user: Some(app.fixtures.org_a.member.clone()),
		body: Some(json!({
			"key": "emergency_stop",
			"name": "Emergency Stop",
			"description": "Emergency kill switch for all features",
			"linked_flag_keys": []
		})),
		expected_status: StatusCode::CREATED,
	}];

	run_authz_cases(&app, &cases).await;
}

#[tokio::test]
async fn non_member_cannot_create_kill_switch() {
	let app = TestApp::new().await;
	let org_id = app.fixtures.org_a.org.id.to_string();

	let cases = vec![AuthzCase {
		name: "non_member_cannot_create_kill_switch",
		method: Method::POST,
		path: format!("/api/orgs/{}/flags/kill-switches", org_id),
		user: Some(app.fixtures.org_b.member.clone()),
		body: Some(json!({
			"key": "forbidden_ks",
			"name": "Forbidden Kill Switch",
			"linked_flag_keys": []
		})),
		expected_status: StatusCode::NOT_FOUND,
	}];

	run_authz_cases(&app, &cases).await;
}

// ============================================================================
// Evaluation Routes
// ============================================================================

#[tokio::test]
async fn org_member_can_evaluate_flags() {
	let app = TestApp::new().await;
	let org_id = app.fixtures.org_a.org.id.to_string();

	// First create an environment for the org since test fixtures don't auto-create them
	// via the route handler (which would trigger environment creation)
	let env_response = app
		.post(
			&format!("/api/orgs/{}/flags/environments", org_id),
			Some(&app.fixtures.org_a.member),
			json!({
				"name": "prod",
				"color": "#22C55E"
			}),
		)
		.await;
	assert_eq!(env_response.status(), StatusCode::CREATED);

	let cases = vec![AuthzCase {
		name: "org_member_can_evaluate_flags",
		method: Method::POST,
		path: format!("/api/orgs/{}/flags/evaluate", org_id),
		user: Some(app.fixtures.org_a.member.clone()),
		body: Some(json!({
			"context": {
				"environment": "prod",
				"user_id": "user_123",
				"attributes": {
					"plan": "enterprise"
				}
			}
		})),
		expected_status: StatusCode::OK,
	}];

	run_authz_cases(&app, &cases).await;
}

#[tokio::test]
async fn non_member_cannot_evaluate_flags() {
	let app = TestApp::new().await;
	let org_id = app.fixtures.org_a.org.id.to_string();

	let cases = vec![AuthzCase {
		name: "non_member_cannot_evaluate_flags",
		method: Method::POST,
		path: format!("/api/orgs/{}/flags/evaluate", org_id),
		user: Some(app.fixtures.org_b.member.clone()),
		body: Some(json!({
			"context": {
				"environment": "prod",
				"user_id": "user_123"
			}
		})),
		expected_status: StatusCode::NOT_FOUND,
	}];

	run_authz_cases(&app, &cases).await;
}

#[tokio::test]
async fn unauthenticated_cannot_evaluate_flags() {
	let app = TestApp::new().await;
	let org_id = app.fixtures.org_a.org.id.to_string();

	let cases = vec![AuthzCase {
		name: "unauthenticated_cannot_evaluate_flags",
		method: Method::POST,
		path: format!("/api/orgs/{}/flags/evaluate", org_id),
		user: None,
		body: Some(json!({
			"context": {
				"environment": "prod",
				"user_id": "user_123"
			}
		})),
		expected_status: StatusCode::UNAUTHORIZED,
	}];

	run_authz_cases(&app, &cases).await;
}

// ============================================================================
// Stale Flags Routes
// ============================================================================

#[tokio::test]
async fn org_member_can_list_stale_flags() {
	let app = TestApp::new().await;
	let org_id = app.fixtures.org_a.org.id.to_string();

	let cases = vec![AuthzCase {
		name: "org_member_can_list_stale_flags",
		method: Method::GET,
		path: format!("/api/orgs/{}/flags/stale", org_id),
		user: Some(app.fixtures.org_a.member.clone()),
		body: None,
		expected_status: StatusCode::OK,
	}];

	run_authz_cases(&app, &cases).await;
}

#[tokio::test]
async fn non_member_cannot_list_stale_flags() {
	let app = TestApp::new().await;
	let org_id = app.fixtures.org_a.org.id.to_string();

	let cases = vec![AuthzCase {
		name: "non_member_cannot_list_stale_flags",
		method: Method::GET,
		path: format!("/api/orgs/{}/flags/stale", org_id),
		user: Some(app.fixtures.org_b.member.clone()),
		body: None,
		expected_status: StatusCode::NOT_FOUND,
	}];

	run_authz_cases(&app, &cases).await;
}

// ============================================================================
// SSE Stream Route (SDK Key Auth)
// ============================================================================

#[tokio::test]
async fn stream_requires_sdk_key_authentication() {
	let app = TestApp::new().await;

	// SSE stream endpoint requires SDK key, not user session
	// Must provide environment query parameter
	let cases = vec![AuthzCase {
		name: "stream_requires_sdk_key_authentication",
		method: Method::GET,
		path: "/api/flags/stream?environment=prod".to_string(),
		user: None,
		body: None,
		// Without SDK key header, should return 401
		expected_status: StatusCode::UNAUTHORIZED,
	}];

	run_authz_cases(&app, &cases).await;
}

// ============================================================================
// Stream Stats Route (Admin Only)
// ============================================================================

#[tokio::test]
async fn admin_can_view_stream_stats() {
	let app = TestApp::new().await;

	let cases = vec![AuthzCase {
		name: "admin_can_view_stream_stats",
		method: Method::GET,
		path: "/api/flags/stream/stats".to_string(),
		user: Some(app.fixtures.admin.clone()),
		body: None,
		expected_status: StatusCode::OK,
	}];

	run_authz_cases(&app, &cases).await;
}

#[tokio::test]
async fn non_admin_cannot_view_stream_stats() {
	let app = TestApp::new().await;

	let cases = vec![AuthzCase {
		name: "non_admin_cannot_view_stream_stats",
		method: Method::GET,
		path: "/api/flags/stream/stats".to_string(),
		user: Some(app.fixtures.org_a.owner.clone()),
		body: None,
		expected_status: StatusCode::FORBIDDEN,
	}];

	run_authz_cases(&app, &cases).await;
}

// ============================================================================
// Cross-Org Access Tests
// ============================================================================

#[tokio::test]
async fn org_a_member_cannot_access_org_b_flags() {
	let app = TestApp::new().await;
	let org_b_id = app.fixtures.org_b.org.id.to_string();

	let cases = vec![
		AuthzCase {
			name: "org_a_member_cannot_list_org_b_flags",
			method: Method::GET,
			path: format!("/api/orgs/{}/flags", org_b_id),
			user: Some(app.fixtures.org_a.member.clone()),
			body: None,
			expected_status: StatusCode::NOT_FOUND,
		},
		AuthzCase {
			name: "org_a_member_cannot_list_org_b_environments",
			method: Method::GET,
			path: format!("/api/orgs/{}/flags/environments", org_b_id),
			user: Some(app.fixtures.org_a.member.clone()),
			body: None,
			expected_status: StatusCode::NOT_FOUND,
		},
		AuthzCase {
			name: "org_a_member_cannot_list_org_b_strategies",
			method: Method::GET,
			path: format!("/api/orgs/{}/flags/strategies", org_b_id),
			user: Some(app.fixtures.org_a.member.clone()),
			body: None,
			expected_status: StatusCode::NOT_FOUND,
		},
		AuthzCase {
			name: "org_a_member_cannot_list_org_b_kill_switches",
			method: Method::GET,
			path: format!("/api/orgs/{}/flags/kill-switches", org_b_id),
			user: Some(app.fixtures.org_a.member.clone()),
			body: None,
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
			name: "invalid_org_id_for_flags",
			method: Method::GET,
			path: "/api/orgs/not-a-uuid/flags".to_string(),
			user: Some(app.fixtures.org_a.member.clone()),
			body: None,
			expected_status: StatusCode::BAD_REQUEST,
		},
		AuthzCase {
			name: "invalid_org_id_for_environments",
			method: Method::GET,
			path: "/api/orgs/not-a-uuid/flags/environments".to_string(),
			user: Some(app.fixtures.org_a.member.clone()),
			body: None,
			expected_status: StatusCode::BAD_REQUEST,
		},
	];

	run_authz_cases(&app, &cases).await;
}
