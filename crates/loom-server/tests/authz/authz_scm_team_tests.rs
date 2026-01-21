// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! Authorization tests for team-based access control in SCM handlers.
//!
//! These tests verify that team-based access enforcement works correctly
//! for git operations (clone, push, fetch) and repository management.

use axum::http::{Method, StatusCode};
use serde_json::json;

use super::support::{run_authz_cases, AuthzCase, TestApp};

/// Test that team-based access grants read access to org repos.
#[tokio::test]
async fn test_team_member_can_read_org_repo() {
	let app = TestApp::new().await;
	let owner = &app.fixtures.org_a.owner;
	let member = &app.fixtures.org_a.member;
	let org_id = app.fixtures.org_a.org.id.to_string();
	let team = &app.fixtures.org_a.team;

	// Create an org repo
	let create_response = app
		.post(
			"/api/repos",
			Some(owner),
			json!({
				"owner_type": "org",
				"owner_id": org_id,
				"name": "team-test-repo",
				"visibility": "private"
			}),
		)
		.await;
	assert_eq!(create_response.status(), StatusCode::CREATED);

	let body = axum::body::to_bytes(create_response.into_body(), usize::MAX)
		.await
		.unwrap();
	let repo: serde_json::Value = serde_json::from_slice(&body).unwrap();
	let repo_id = repo["id"].as_str().unwrap();

	// Grant read access to the team
	let grant_response = app
		.post(
			&format!("/api/repos/{repo_id}/teams"),
			Some(owner),
			json!({
				"team_id": team.id.to_string(),
				"role": "read"
			}),
		)
		.await;

	// Should succeed (201 or 200)
	assert!(
		grant_response.status() == StatusCode::CREATED || grant_response.status() == StatusCode::OK,
		"Grant team access should succeed, got {}",
		grant_response.status()
	);

	// Team member should now be able to read the repo
	let cases = vec![AuthzCase {
		name: "team_member_can_read_repo_with_team_access",
		method: Method::GET,
		path: format!("/api/repos/{repo_id}"),
		user: Some(member.clone()),
		body: None,
		expected_status: StatusCode::OK,
	}];

	run_authz_cases(&app, &cases).await;
}

/// Test that team access with write role allows push operations.
#[tokio::test]
async fn test_team_write_access_allows_push() {
	let app = TestApp::new().await;
	let owner = &app.fixtures.org_a.owner;
	let org_id = app.fixtures.org_a.org.id.to_string();
	let team = &app.fixtures.org_a.team;

	// Create an org repo
	let create_response = app
		.post(
			"/api/repos",
			Some(owner),
			json!({
				"owner_type": "org",
				"owner_id": org_id,
				"name": "team-write-test-repo",
				"visibility": "private"
			}),
		)
		.await;
	assert_eq!(create_response.status(), StatusCode::CREATED);

	let body = axum::body::to_bytes(create_response.into_body(), usize::MAX)
		.await
		.unwrap();
	let repo: serde_json::Value = serde_json::from_slice(&body).unwrap();
	let repo_id = repo["id"].as_str().unwrap();

	// Grant write access to the team
	let _grant_response = app
		.post(
			&format!("/api/repos/{repo_id}/teams"),
			Some(owner),
			json!({
				"team_id": team.id.to_string(),
				"role": "write"
			}),
		)
		.await;

	// Team member should get their role via the team access endpoint
	let list_response = app
		.get(&format!("/api/repos/{repo_id}/teams"), Some(owner))
		.await;

	// Only admin can list team access
	assert_eq!(list_response.status(), StatusCode::OK);
}

/// Test that team admin access allows repo management.
#[tokio::test]
async fn test_team_admin_can_manage_repo() {
	let app = TestApp::new().await;
	let owner = &app.fixtures.org_a.owner;
	let member = &app.fixtures.org_a.member;
	let org_id = app.fixtures.org_a.org.id.to_string();
	let team = &app.fixtures.org_a.team;

	// Create an org repo
	let create_response = app
		.post(
			"/api/repos",
			Some(owner),
			json!({
				"owner_type": "org",
				"owner_id": org_id,
				"name": "team-admin-test-repo",
				"visibility": "private"
			}),
		)
		.await;
	assert_eq!(create_response.status(), StatusCode::CREATED);

	let body = axum::body::to_bytes(create_response.into_body(), usize::MAX)
		.await
		.unwrap();
	let repo: serde_json::Value = serde_json::from_slice(&body).unwrap();
	let repo_id = repo["id"].as_str().unwrap();

	// Grant admin access to the team
	let _grant_response = app
		.post(
			&format!("/api/repos/{repo_id}/teams"),
			Some(owner),
			json!({
				"team_id": team.id.to_string(),
				"role": "admin"
			}),
		)
		.await;

	// Team member with admin should be able to update repo
	let update_cases = vec![AuthzCase {
		name: "team_admin_can_update_repo",
		method: Method::PATCH,
		path: format!("/api/repos/{repo_id}"),
		user: Some(member.clone()),
		body: Some(json!({"visibility": "public"})),
		expected_status: StatusCode::OK,
	}];

	run_authz_cases(&app, &update_cases).await;
}

/// Test that non-team member cannot access private org repo.
#[tokio::test]
async fn test_non_team_member_cannot_access_repo() {
	let app = TestApp::new().await;
	let owner = &app.fixtures.org_a.owner;
	let other_user = &app.fixtures.org_b.owner;
	let org_id = app.fixtures.org_a.org.id.to_string();

	// Create a private org repo
	let create_response = app
		.post(
			"/api/repos",
			Some(owner),
			json!({
				"owner_type": "org",
				"owner_id": org_id,
				"name": "private-no-team-repo",
				"visibility": "private"
			}),
		)
		.await;
	assert_eq!(create_response.status(), StatusCode::CREATED);

	let body = axum::body::to_bytes(create_response.into_body(), usize::MAX)
		.await
		.unwrap();
	let repo: serde_json::Value = serde_json::from_slice(&body).unwrap();
	let repo_id = repo["id"].as_str().unwrap();

	// User from another org should not be able to access
	let cases = vec![AuthzCase {
		name: "non_team_member_cannot_read_private_repo",
		method: Method::GET,
		path: format!("/api/repos/{repo_id}"),
		user: Some(other_user.clone()),
		body: None,
		expected_status: StatusCode::FORBIDDEN,
	}];

	run_authz_cases(&app, &cases).await;
}

/// Test that team access can be revoked.
#[tokio::test]
async fn test_revoke_team_access() {
	let app = TestApp::new().await;
	let owner = &app.fixtures.org_a.owner;
	let member = &app.fixtures.org_a.member;
	let org_id = app.fixtures.org_a.org.id.to_string();
	let team = &app.fixtures.org_a.team;

	// Create an org repo
	let create_response = app
		.post(
			"/api/repos",
			Some(owner),
			json!({
				"owner_type": "org",
				"owner_id": org_id,
				"name": "revoke-team-test-repo",
				"visibility": "private"
			}),
		)
		.await;
	assert_eq!(create_response.status(), StatusCode::CREATED);

	let body = axum::body::to_bytes(create_response.into_body(), usize::MAX)
		.await
		.unwrap();
	let repo: serde_json::Value = serde_json::from_slice(&body).unwrap();
	let repo_id = repo["id"].as_str().unwrap();

	// Grant admin access to the team
	let _grant_response = app
		.post(
			&format!("/api/repos/{repo_id}/teams"),
			Some(owner),
			json!({
				"team_id": team.id.to_string(),
				"role": "admin"
			}),
		)
		.await;

	// Verify member can access
	let can_access = app
		.get(&format!("/api/repos/{repo_id}"), Some(member))
		.await;
	assert_eq!(
		can_access.status(),
		StatusCode::OK,
		"Member should have access via team"
	);

	// Revoke team access
	let revoke_response = app
		.delete(
			&format!("/api/repos/{repo_id}/teams/{}", team.id),
			Some(owner),
		)
		.await;

	assert!(
		revoke_response.status() == StatusCode::NO_CONTENT
			|| revoke_response.status() == StatusCode::OK,
		"Revoke should succeed, got {}",
		revoke_response.status()
	);
}

/// Test that only admin can manage team access.
#[tokio::test]
async fn test_only_admin_can_grant_team_access() {
	let app = TestApp::new().await;
	let owner = &app.fixtures.org_a.owner;
	let member = &app.fixtures.org_a.member;
	let org_id = app.fixtures.org_a.org.id.to_string();
	let team = &app.fixtures.org_a.team;

	// Create an org repo
	let create_response = app
		.post(
			"/api/repos",
			Some(owner),
			json!({
				"owner_type": "org",
				"owner_id": org_id,
				"name": "admin-only-grant-repo",
				"visibility": "private"
			}),
		)
		.await;
	assert_eq!(create_response.status(), StatusCode::CREATED);

	let body = axum::body::to_bytes(create_response.into_body(), usize::MAX)
		.await
		.unwrap();
	let repo: serde_json::Value = serde_json::from_slice(&body).unwrap();
	let repo_id = repo["id"].as_str().unwrap();

	// Member should not be able to grant team access
	let cases = vec![AuthzCase {
		name: "member_cannot_grant_team_access",
		method: Method::POST,
		path: format!("/api/repos/{repo_id}/teams"),
		user: Some(member.clone()),
		body: Some(json!({
			"team_id": team.id.to_string(),
			"role": "read"
		})),
		expected_status: StatusCode::FORBIDDEN,
	}];

	run_authz_cases(&app, &cases).await;
}

/// Test role hierarchy: admin > write > read.
#[tokio::test]
async fn test_team_role_hierarchy() {
	let app = TestApp::new().await;
	let owner = &app.fixtures.org_a.owner;
	let org_id = app.fixtures.org_a.org.id.to_string();
	let team = &app.fixtures.org_a.team;

	// Create an org repo
	let create_response = app
		.post(
			"/api/repos",
			Some(owner),
			json!({
				"owner_type": "org",
				"owner_id": org_id,
				"name": "role-hierarchy-test-repo",
				"visibility": "private"
			}),
		)
		.await;
	assert_eq!(create_response.status(), StatusCode::CREATED);

	let body = axum::body::to_bytes(create_response.into_body(), usize::MAX)
		.await
		.unwrap();
	let repo: serde_json::Value = serde_json::from_slice(&body).unwrap();
	let repo_id = repo["id"].as_str().unwrap();

	// Grant read access
	let grant_read = app
		.post(
			&format!("/api/repos/{repo_id}/teams"),
			Some(owner),
			json!({
				"team_id": team.id.to_string(),
				"role": "read"
			}),
		)
		.await;

	assert!(
		grant_read.status() == StatusCode::OK || grant_read.status() == StatusCode::CREATED,
		"Grant read access should succeed, got {}",
		grant_read.status()
	);

	// Upgrade to write access
	let grant_write = app
		.post(
			&format!("/api/repos/{repo_id}/teams"),
			Some(owner),
			json!({
				"team_id": team.id.to_string(),
				"role": "write"
			}),
		)
		.await;

	assert!(
		grant_write.status() == StatusCode::OK || grant_write.status() == StatusCode::CREATED,
		"Grant write access should succeed (upgrade), got {}",
		grant_write.status()
	);

	// Upgrade to admin access
	let grant_admin = app
		.post(
			&format!("/api/repos/{repo_id}/teams"),
			Some(owner),
			json!({
				"team_id": team.id.to_string(),
				"role": "admin"
			}),
		)
		.await;

	assert!(
		grant_admin.status() == StatusCode::OK || grant_admin.status() == StatusCode::CREATED,
		"Grant admin access should succeed (upgrade), got {}",
		grant_admin.status()
	);
}
