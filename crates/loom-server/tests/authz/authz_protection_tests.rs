// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! Authorization tests for Branch Protection endpoints.
//!
//! Tests cover:
//! - CRUD operations on protection rules
//! - Authorization (only repo admins can manage protection)
//! - Duplicate pattern rejection
//! - Non-existent repo handling

use axum::http::{Method, StatusCode};
use serde_json::json;

use super::support::{run_authz_cases, AuthzCase, TestApp};

/// Helper to create a test repo and return its ID
async fn create_test_repo(app: &TestApp, name: &str) -> String {
	let owner = &app.fixtures.org_a.owner;
	let user_id = owner.user.id.to_string();

	let response = app
		.post(
			"/api/repos",
			Some(owner),
			json!({
				"owner_type": "user",
				"owner_id": user_id,
				"name": name,
				"visibility": "private"
			}),
		)
		.await;

	assert_eq!(response.status(), StatusCode::CREATED);

	let body = axum::body::to_bytes(response.into_body(), usize::MAX)
		.await
		.unwrap();
	let repo: serde_json::Value = serde_json::from_slice(&body).unwrap();
	repo["id"].as_str().unwrap().to_string()
}

/// **Test: Create branch protection rule**
///
/// Repo admin can create a protection rule.
#[tokio::test]
async fn test_protection_create_success() {
	let app = TestApp::new().await;
	let repo_id = create_test_repo(&app, "protection-create-test").await;
	let owner = &app.fixtures.org_a.owner;

	let response = app
		.post(
			&format!("/api/repos/{repo_id}/protection"),
			Some(owner),
			json!({
				"pattern": "cannon",
				"block_direct_push": true,
				"block_force_push": true,
				"block_deletion": true
			}),
		)
		.await;

	assert_eq!(
		response.status(),
		StatusCode::CREATED,
		"Admin should be able to create protection rule"
	);

	// Verify response contains expected fields
	let body = axum::body::to_bytes(response.into_body(), usize::MAX)
		.await
		.unwrap();
	let rule: serde_json::Value = serde_json::from_slice(&body).unwrap();

	assert!(rule.get("id").is_some(), "Response should have 'id'");
	assert_eq!(rule["pattern"].as_str().unwrap(), "cannon");
	assert!(rule["block_direct_push"].as_bool().unwrap());
	assert!(rule["block_force_push"].as_bool().unwrap());
	assert!(rule["block_deletion"].as_bool().unwrap());
}

/// **Test: Create wildcard protection rule**
///
/// Wildcard patterns like 'release/*' should be accepted.
#[tokio::test]
async fn test_protection_create_wildcard_pattern() {
	let app = TestApp::new().await;
	let repo_id = create_test_repo(&app, "protection-wildcard-test").await;
	let owner = &app.fixtures.org_a.owner;

	let response = app
		.post(
			&format!("/api/repos/{repo_id}/protection"),
			Some(owner),
			json!({
				"pattern": "release/*",
				"block_direct_push": true,
				"block_force_push": true,
				"block_deletion": true
			}),
		)
		.await;

	assert_eq!(
		response.status(),
		StatusCode::CREATED,
		"Wildcard pattern should be accepted"
	);
}

/// **Test: List protection rules**
///
/// Admin can list all protection rules for a repo.
#[tokio::test]
async fn test_protection_list() {
	let app = TestApp::new().await;
	let repo_id = create_test_repo(&app, "protection-list-test").await;
	let owner = &app.fixtures.org_a.owner;

	// Initially empty
	let response = app
		.get(&format!("/api/repos/{repo_id}/protection"), Some(owner))
		.await;
	assert_eq!(response.status(), StatusCode::OK);

	let body = axum::body::to_bytes(response.into_body(), usize::MAX)
		.await
		.unwrap();
	let result: serde_json::Value = serde_json::from_slice(&body).unwrap();
	assert!(result["rules"].as_array().unwrap().is_empty());

	// Create a rule
	app
		.post(
			&format!("/api/repos/{repo_id}/protection"),
			Some(owner),
			json!({
				"pattern": "main",
				"block_direct_push": true,
				"block_force_push": false,
				"block_deletion": true
			}),
		)
		.await;

	// List should now have one rule
	let response = app
		.get(&format!("/api/repos/{repo_id}/protection"), Some(owner))
		.await;
	assert_eq!(response.status(), StatusCode::OK);

	let body = axum::body::to_bytes(response.into_body(), usize::MAX)
		.await
		.unwrap();
	let result: serde_json::Value = serde_json::from_slice(&body).unwrap();
	assert_eq!(result["rules"].as_array().unwrap().len(), 1);
	assert_eq!(result["rules"][0]["pattern"].as_str().unwrap(), "main");
}

/// **Test: Delete protection rule**
///
/// Admin can delete a protection rule.
#[tokio::test]
async fn test_protection_delete() {
	let app = TestApp::new().await;
	let repo_id = create_test_repo(&app, "protection-delete-test").await;
	let owner = &app.fixtures.org_a.owner;

	// Create a rule
	let response = app
		.post(
			&format!("/api/repos/{repo_id}/protection"),
			Some(owner),
			json!({
				"pattern": "cannon",
				"block_direct_push": true,
				"block_force_push": true,
				"block_deletion": true
			}),
		)
		.await;

	let body = axum::body::to_bytes(response.into_body(), usize::MAX)
		.await
		.unwrap();
	let rule: serde_json::Value = serde_json::from_slice(&body).unwrap();
	let rule_id = rule["id"].as_str().unwrap();

	// Delete the rule
	let response = app
		.delete(
			&format!("/api/repos/{repo_id}/protection/{rule_id}"),
			Some(owner),
		)
		.await;
	assert_eq!(response.status(), StatusCode::NO_CONTENT);

	// Verify it's gone
	let response = app
		.get(&format!("/api/repos/{repo_id}/protection"), Some(owner))
		.await;
	let body = axum::body::to_bytes(response.into_body(), usize::MAX)
		.await
		.unwrap();
	let result: serde_json::Value = serde_json::from_slice(&body).unwrap();
	assert!(result["rules"].as_array().unwrap().is_empty());
}

/// **Test: Duplicate pattern returns conflict**
///
/// Creating a rule with an existing pattern should return 409 Conflict.
#[tokio::test]
async fn test_protection_duplicate_pattern_conflict() {
	let app = TestApp::new().await;
	let repo_id = create_test_repo(&app, "protection-dup-test").await;
	let owner = &app.fixtures.org_a.owner;

	// Create first rule
	let response = app
		.post(
			&format!("/api/repos/{repo_id}/protection"),
			Some(owner),
			json!({
				"pattern": "cannon",
				"block_direct_push": true,
				"block_force_push": true,
				"block_deletion": true
			}),
		)
		.await;
	assert_eq!(response.status(), StatusCode::CREATED);

	// Try to create duplicate
	let response = app
		.post(
			&format!("/api/repos/{repo_id}/protection"),
			Some(owner),
			json!({
				"pattern": "cannon",
				"block_direct_push": false,
				"block_force_push": false,
				"block_deletion": false
			}),
		)
		.await;
	assert_eq!(
		response.status(),
		StatusCode::CONFLICT,
		"Duplicate pattern should return 409 Conflict"
	);
}

/// **Test: Non-admin cannot create protection rule**
///
/// Users without admin access should get 403 Forbidden.
#[tokio::test]
async fn test_protection_non_admin_cannot_create() {
	let app = TestApp::new().await;
	let repo_id = create_test_repo(&app, "protection-auth-test").await;
	let other_user = &app.fixtures.org_b.owner;

	let cases = vec![
		// Other user (not repo owner) cannot create
		AuthzCase {
			name: "other_user_cannot_create_protection",
			method: Method::POST,
			path: format!("/api/repos/{repo_id}/protection"),
			user: Some(other_user.clone()),
			body: Some(json!({
				"pattern": "main",
				"block_direct_push": true,
				"block_force_push": true,
				"block_deletion": true
			})),
			expected_status: StatusCode::FORBIDDEN,
		},
		// Unauthenticated user cannot create
		AuthzCase {
			name: "unauthenticated_cannot_create_protection",
			method: Method::POST,
			path: format!("/api/repos/{repo_id}/protection"),
			user: None,
			body: Some(json!({
				"pattern": "main",
				"block_direct_push": true,
				"block_force_push": true,
				"block_deletion": true
			})),
			expected_status: StatusCode::UNAUTHORIZED,
		},
	];

	run_authz_cases(&app, &cases).await;
}

/// **Test: Non-admin cannot delete protection rule**
///
/// Users without admin access should get 403 Forbidden.
#[tokio::test]
async fn test_protection_non_admin_cannot_delete() {
	let app = TestApp::new().await;
	let repo_id = create_test_repo(&app, "protection-del-auth-test").await;
	let owner = &app.fixtures.org_a.owner;
	let other_user = &app.fixtures.org_b.owner;

	// Create a rule first
	let response = app
		.post(
			&format!("/api/repos/{repo_id}/protection"),
			Some(owner),
			json!({
				"pattern": "cannon",
				"block_direct_push": true,
				"block_force_push": true,
				"block_deletion": true
			}),
		)
		.await;
	let body = axum::body::to_bytes(response.into_body(), usize::MAX)
		.await
		.unwrap();
	let rule: serde_json::Value = serde_json::from_slice(&body).unwrap();
	let rule_id = rule["id"].as_str().unwrap();

	let cases = vec![
		// Other user cannot delete
		AuthzCase {
			name: "other_user_cannot_delete_protection",
			method: Method::DELETE,
			path: format!("/api/repos/{repo_id}/protection/{rule_id}"),
			user: Some(other_user.clone()),
			body: None,
			expected_status: StatusCode::FORBIDDEN,
		},
		// Unauthenticated user cannot delete
		AuthzCase {
			name: "unauthenticated_cannot_delete_protection",
			method: Method::DELETE,
			path: format!("/api/repos/{repo_id}/protection/{rule_id}"),
			user: None,
			body: None,
			expected_status: StatusCode::UNAUTHORIZED,
		},
	];

	run_authz_cases(&app, &cases).await;
}

/// **Test: Protection on non-existent repo returns 404**
#[tokio::test]
async fn test_protection_nonexistent_repo() {
	let app = TestApp::new().await;
	let owner = &app.fixtures.org_a.owner;
	let fake_repo_id = "00000000-0000-0000-0000-000000000000";

	let cases = vec![
		AuthzCase {
			name: "list_protection_nonexistent_repo",
			method: Method::GET,
			path: format!("/api/repos/{fake_repo_id}/protection"),
			user: Some(owner.clone()),
			body: None,
			expected_status: StatusCode::NOT_FOUND,
		},
		AuthzCase {
			name: "create_protection_nonexistent_repo",
			method: Method::POST,
			path: format!("/api/repos/{fake_repo_id}/protection"),
			user: Some(owner.clone()),
			body: Some(json!({
				"pattern": "main",
				"block_direct_push": true,
				"block_force_push": true,
				"block_deletion": true
			})),
			expected_status: StatusCode::NOT_FOUND,
		},
	];

	run_authz_cases(&app, &cases).await;
}

/// **Test: Delete non-existent rule returns 404**
#[tokio::test]
async fn test_protection_delete_nonexistent_rule() {
	let app = TestApp::new().await;
	let repo_id = create_test_repo(&app, "protection-del-404-test").await;
	let owner = &app.fixtures.org_a.owner;
	let fake_rule_id = "00000000-0000-0000-0000-000000000000";

	let response = app
		.delete(
			&format!("/api/repos/{repo_id}/protection/{fake_rule_id}"),
			Some(owner),
		)
		.await;
	assert_eq!(
		response.status(),
		StatusCode::NOT_FOUND,
		"Deleting non-existent rule should return 404"
	);
}

/// **Test: Org admin can manage protection on org repo**
#[tokio::test]
async fn test_protection_org_admin_can_manage() {
	let app = TestApp::new().await;
	let owner = &app.fixtures.org_a.owner;
	let org_id = app.fixtures.org_a.org.id.to_string();

	// Create org repo
	let response = app
		.post(
			"/api/repos",
			Some(owner),
			json!({
				"owner_type": "org",
				"owner_id": org_id,
				"name": "org-protection-test",
				"visibility": "private"
			}),
		)
		.await;
	assert_eq!(response.status(), StatusCode::CREATED);

	let body = axum::body::to_bytes(response.into_body(), usize::MAX)
		.await
		.unwrap();
	let repo: serde_json::Value = serde_json::from_slice(&body).unwrap();
	let repo_id = repo["id"].as_str().unwrap();

	// Org owner can create protection
	let response = app
		.post(
			&format!("/api/repos/{repo_id}/protection"),
			Some(owner),
			json!({
				"pattern": "cannon",
				"block_direct_push": true,
				"block_force_push": true,
				"block_deletion": true
			}),
		)
		.await;
	assert_eq!(
		response.status(),
		StatusCode::CREATED,
		"Org owner should be able to create protection on org repo"
	);
}

/// **Test: Org member cannot manage protection on org repo**
#[tokio::test]
async fn test_protection_org_member_cannot_manage() {
	let app = TestApp::new().await;
	let owner = &app.fixtures.org_a.owner;
	let member = &app.fixtures.org_a.member;
	let org_id = app.fixtures.org_a.org.id.to_string();

	// Create org repo
	let response = app
		.post(
			"/api/repos",
			Some(owner),
			json!({
				"owner_type": "org",
				"owner_id": org_id,
				"name": "org-protection-member-test",
				"visibility": "private"
			}),
		)
		.await;
	let body = axum::body::to_bytes(response.into_body(), usize::MAX)
		.await
		.unwrap();
	let repo: serde_json::Value = serde_json::from_slice(&body).unwrap();
	let repo_id = repo["id"].as_str().unwrap();

	// Member cannot create protection (only has Read access per get_direct_role)
	let response = app
		.post(
			&format!("/api/repos/{repo_id}/protection"),
			Some(member),
			json!({
				"pattern": "main",
				"block_direct_push": true,
				"block_force_push": true,
				"block_deletion": true
			}),
		)
		.await;
	assert_eq!(
		response.status(),
		StatusCode::FORBIDDEN,
		"Org member should not be able to create protection on org repo"
	);
}
