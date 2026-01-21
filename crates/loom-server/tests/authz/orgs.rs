// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use axum::http::{Method, StatusCode};
use serde_json::json;

use super::support::{run_authz_cases, AuthzCase, TestApp};

#[tokio::test]
async fn test_org_authorization() {
	let app = TestApp::new().await;
	let org_a_id = app.fixtures.org_a.org.id.to_string();
	let org_b_id = app.fixtures.org_b.org.id.to_string();

	let cases = vec![
		// GET /api/orgs - user_can_list_own_orgs
		AuthzCase {
			name: "user_can_list_own_orgs",
			method: Method::GET,
			path: "/api/orgs".to_string(),
			user: Some(app.fixtures.org_a.owner.clone()),
			body: None,
			expected_status: StatusCode::OK,
		},
		// GET /api/orgs/{id} - member_can_get_own_org
		AuthzCase {
			name: "member_can_get_own_org",
			method: Method::GET,
			path: format!("/api/orgs/{org_a_id}"),
			user: Some(app.fixtures.org_a.member.clone()),
			body: None,
			expected_status: StatusCode::OK,
		},
		// GET /api/orgs/{id} - other_org_cannot_get_org
		AuthzCase {
			name: "other_org_cannot_get_org",
			method: Method::GET,
			path: format!("/api/orgs/{org_a_id}"),
			user: Some(app.fixtures.org_b.member.clone()),
			body: None,
			expected_status: StatusCode::FORBIDDEN,
		},
		// PATCH /api/orgs/{id} - owner_can_update_org
		AuthzCase {
			name: "owner_can_update_org",
			method: Method::PATCH,
			path: format!("/api/orgs/{org_a_id}"),
			user: Some(app.fixtures.org_a.owner.clone()),
			body: Some(json!({"name": "Updated Organization A"})),
			expected_status: StatusCode::OK,
		},
		// PATCH /api/orgs/{id} - member_cannot_update_org
		AuthzCase {
			name: "member_cannot_update_org",
			method: Method::PATCH,
			path: format!("/api/orgs/{org_a_id}"),
			user: Some(app.fixtures.org_a.member.clone()),
			body: Some(json!({"name": "Should Not Update"})),
			expected_status: StatusCode::FORBIDDEN,
		},
		// PATCH /api/orgs/{id} - other_org_cannot_update_org
		AuthzCase {
			name: "other_org_cannot_update_org",
			method: Method::PATCH,
			path: format!("/api/orgs/{org_a_id}"),
			user: Some(app.fixtures.org_b.owner.clone()),
			body: Some(json!({"name": "Should Not Update"})),
			expected_status: StatusCode::FORBIDDEN,
		},
		// DELETE /api/orgs/{id} - member_cannot_delete_org
		AuthzCase {
			name: "member_cannot_delete_org",
			method: Method::DELETE,
			path: format!("/api/orgs/{org_b_id}"),
			user: Some(app.fixtures.org_b.member.clone()),
			body: None,
			expected_status: StatusCode::FORBIDDEN,
		},
		// GET /api/orgs/{id}/members - member_can_list_members
		AuthzCase {
			name: "member_can_list_members",
			method: Method::GET,
			path: format!("/api/orgs/{org_a_id}/members"),
			user: Some(app.fixtures.org_a.member.clone()),
			body: None,
			expected_status: StatusCode::OK,
		},
		// GET /api/orgs/{id}/members - other_org_cannot_list_members
		AuthzCase {
			name: "other_org_cannot_list_members",
			method: Method::GET,
			path: format!("/api/orgs/{org_a_id}/members"),
			user: Some(app.fixtures.org_b.member.clone()),
			body: None,
			expected_status: StatusCode::FORBIDDEN,
		},
	];

	run_authz_cases(&app, &cases).await;
}

#[tokio::test]
async fn test_org_create_authorization() {
	let app = TestApp::new().await;

	let cases = vec![
		// POST /api/orgs - user_can_create_org
		AuthzCase {
			name: "user_can_create_org",
			method: Method::POST,
			path: "/api/orgs".to_string(),
			user: Some(app.fixtures.org_a.owner.clone()),
			body: Some(json!({
				"name": "New Organization",
				"slug": "new-org"
			})),
			expected_status: StatusCode::CREATED,
		},
	];

	run_authz_cases(&app, &cases).await;
}

#[tokio::test]
async fn test_org_delete_authorization() {
	let app = TestApp::new().await;

	let create_response = app
		.post(
			"/api/orgs",
			Some(&app.fixtures.org_a.owner),
			json!({
				"name": "Org To Delete",
				"slug": "org-to-delete"
			}),
		)
		.await;
	assert_eq!(create_response.status(), StatusCode::CREATED);

	let body = axum::body::to_bytes(create_response.into_body(), usize::MAX)
		.await
		.unwrap();
	let created_org: serde_json::Value = serde_json::from_slice(&body).unwrap();
	let new_org_id = created_org["id"].as_str().unwrap();

	let cases = vec![
		// DELETE /api/orgs/{id} - owner_can_delete_org
		AuthzCase {
			name: "owner_can_delete_org",
			method: Method::DELETE,
			path: format!("/api/orgs/{new_org_id}"),
			user: Some(app.fixtures.org_a.owner.clone()),
			body: None,
			expected_status: StatusCode::OK,
		},
	];

	run_authz_cases(&app, &cases).await;
}

#[tokio::test]
async fn test_team_authorization() {
	let app = TestApp::new().await;
	let org_a_id = app.fixtures.org_a.org.id.to_string();
	let org_b_id = app.fixtures.org_b.org.id.to_string();
	let team_a_id = app.fixtures.org_a.team.id.to_string();

	let cases = vec![
		// GET /api/orgs/{org_id}/teams - member_can_list_teams
		AuthzCase {
			name: "member_can_list_teams",
			method: Method::GET,
			path: format!("/api/orgs/{org_a_id}/teams"),
			user: Some(app.fixtures.org_a.member.clone()),
			body: None,
			expected_status: StatusCode::OK,
		},
		// GET /api/orgs/{org_id}/teams - other_org_cannot_list_teams
		AuthzCase {
			name: "other_org_cannot_list_teams",
			method: Method::GET,
			path: format!("/api/orgs/{org_a_id}/teams"),
			user: Some(app.fixtures.org_b.member.clone()),
			body: None,
			expected_status: StatusCode::FORBIDDEN,
		},
		// GET /api/orgs/{org_id}/teams/{team_id} - member_can_get_team
		AuthzCase {
			name: "member_can_get_team",
			method: Method::GET,
			path: format!("/api/orgs/{org_a_id}/teams/{team_a_id}"),
			user: Some(app.fixtures.org_a.member.clone()),
			body: None,
			expected_status: StatusCode::OK,
		},
		// GET /api/orgs/{org_id}/teams/{team_id} - other_org_cannot_get_team
		AuthzCase {
			name: "other_org_cannot_get_team",
			method: Method::GET,
			path: format!("/api/orgs/{org_a_id}/teams/{team_a_id}"),
			user: Some(app.fixtures.org_b.member.clone()),
			body: None,
			expected_status: StatusCode::FORBIDDEN,
		},
		// POST /api/orgs/{org_id}/teams - owner_can_create_team
		AuthzCase {
			name: "owner_can_create_team",
			method: Method::POST,
			path: format!("/api/orgs/{org_a_id}/teams"),
			user: Some(app.fixtures.org_a.owner.clone()),
			body: Some(json!({
				"name": "New Team",
				"slug": "new-team"
			})),
			expected_status: StatusCode::CREATED,
		},
		// POST /api/orgs/{org_id}/teams - other_org_cannot_create_team
		AuthzCase {
			name: "other_org_cannot_create_team",
			method: Method::POST,
			path: format!("/api/orgs/{org_a_id}/teams"),
			user: Some(app.fixtures.org_b.owner.clone()),
			body: Some(json!({
				"name": "Unauthorized Team",
				"slug": "unauthorized-team"
			})),
			expected_status: StatusCode::FORBIDDEN,
		},
	];

	run_authz_cases(&app, &cases).await;

	// Verify team creation failed for org_b owner by checking team doesn't exist
	let list_response = app
		.get(
			&format!("/api/orgs/{org_b_id}/teams"),
			Some(&app.fixtures.org_b.owner),
		)
		.await;
	let body = axum::body::to_bytes(list_response.into_body(), usize::MAX)
		.await
		.unwrap();
	let teams: serde_json::Value = serde_json::from_slice(&body).unwrap();
	let team_slugs: Vec<&str> = teams["teams"]
		.as_array()
		.unwrap()
		.iter()
		.filter_map(|t| t["slug"].as_str())
		.collect();
	assert!(
		!team_slugs.contains(&"unauthorized-team"),
		"Team should not have been created in org_b"
	);
}
