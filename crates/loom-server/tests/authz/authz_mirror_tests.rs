// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! Authorization tests for Push Mirror endpoints.
//!
//! Tests cover:
//! - CRUD operations on push mirrors
//! - Authorization (write access required, admin for management)
//! - SSRF protection (blocking localhost, private IPs, cloud metadata)
//! - Sync trigger functionality
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

/// **Test: Create push mirror**
///
/// Repo admin can create a push mirror.
#[tokio::test]
async fn test_mirror_create_success() {
	let app = TestApp::new().await;
	let repo_id = create_test_repo(&app, "mirror-create-test").await;
	let owner = &app.fixtures.org_a.owner;

	let response = app
		.post(
			&format!("/api/repos/{repo_id}/mirrors"),
			Some(owner),
			json!({
				"remote_url": "https://github.com/org/mirror-target.git"
			}),
		)
		.await;

	assert_eq!(
		response.status(),
		StatusCode::CREATED,
		"Admin should be able to create push mirror"
	);

	// Verify response contains expected fields
	let body = axum::body::to_bytes(response.into_body(), usize::MAX)
		.await
		.unwrap();
	let mirror: serde_json::Value = serde_json::from_slice(&body).unwrap();

	assert!(mirror.get("id").is_some(), "Response should have 'id'");
	assert_eq!(mirror["repo_id"].as_str().unwrap(), repo_id);
	assert_eq!(
		mirror["remote_url"].as_str().unwrap(),
		"https://github.com/org/mirror-target.git"
	);
	assert!(mirror["enabled"].as_bool().unwrap());
	assert!(mirror.get("last_pushed_at").is_some());
	assert!(mirror.get("last_error").is_some());
}

/// **Test: List push mirrors**
///
/// Admin can list all push mirrors for a repo.
#[tokio::test]
async fn test_mirror_list() {
	let app = TestApp::new().await;
	let repo_id = create_test_repo(&app, "mirror-list-test").await;
	let owner = &app.fixtures.org_a.owner;

	// Initially empty
	let response = app
		.get(&format!("/api/repos/{repo_id}/mirrors"), Some(owner))
		.await;
	assert_eq!(response.status(), StatusCode::OK);

	let body = axum::body::to_bytes(response.into_body(), usize::MAX)
		.await
		.unwrap();
	let result: serde_json::Value = serde_json::from_slice(&body).unwrap();
	assert!(result["mirrors"].as_array().unwrap().is_empty());

	// Create a mirror
	app
		.post(
			&format!("/api/repos/{repo_id}/mirrors"),
			Some(owner),
			json!({
				"remote_url": "https://github.com/org/repo.git"
			}),
		)
		.await;

	// List should now have one mirror
	let response = app
		.get(&format!("/api/repos/{repo_id}/mirrors"), Some(owner))
		.await;
	assert_eq!(response.status(), StatusCode::OK);

	let body = axum::body::to_bytes(response.into_body(), usize::MAX)
		.await
		.unwrap();
	let result: serde_json::Value = serde_json::from_slice(&body).unwrap();
	assert_eq!(result["mirrors"].as_array().unwrap().len(), 1);
	assert_eq!(
		result["mirrors"][0]["remote_url"].as_str().unwrap(),
		"https://github.com/org/repo.git"
	);
}

/// **Test: Delete push mirror**
///
/// Admin can delete a push mirror.
#[tokio::test]
async fn test_mirror_delete() {
	let app = TestApp::new().await;
	let repo_id = create_test_repo(&app, "mirror-delete-test").await;
	let owner = &app.fixtures.org_a.owner;

	// Create a mirror
	let response = app
		.post(
			&format!("/api/repos/{repo_id}/mirrors"),
			Some(owner),
			json!({
				"remote_url": "https://github.com/org/to-delete.git"
			}),
		)
		.await;

	let body = axum::body::to_bytes(response.into_body(), usize::MAX)
		.await
		.unwrap();
	let mirror: serde_json::Value = serde_json::from_slice(&body).unwrap();
	let mirror_id = mirror["id"].as_str().unwrap();

	// Delete the mirror
	let response = app
		.delete(
			&format!("/api/repos/{repo_id}/mirrors/{mirror_id}"),
			Some(owner),
		)
		.await;
	assert_eq!(response.status(), StatusCode::NO_CONTENT);

	// Verify it's gone
	let response = app
		.get(&format!("/api/repos/{repo_id}/mirrors"), Some(owner))
		.await;
	let body = axum::body::to_bytes(response.into_body(), usize::MAX)
		.await
		.unwrap();
	let result: serde_json::Value = serde_json::from_slice(&body).unwrap();
	assert!(result["mirrors"].as_array().unwrap().is_empty());
}

/// **Test: Trigger sync**
///
/// Admin can trigger a sync for a push mirror.
#[tokio::test]
async fn test_mirror_trigger_sync() {
	let app = TestApp::new().await;
	let repo_id = create_test_repo(&app, "mirror-sync-test").await;
	let owner = &app.fixtures.org_a.owner;

	// Create a mirror
	let response = app
		.post(
			&format!("/api/repos/{repo_id}/mirrors"),
			Some(owner),
			json!({
				"remote_url": "https://github.com/org/sync-target.git"
			}),
		)
		.await;

	let body = axum::body::to_bytes(response.into_body(), usize::MAX)
		.await
		.unwrap();
	let mirror: serde_json::Value = serde_json::from_slice(&body).unwrap();
	let mirror_id = mirror["id"].as_str().unwrap();

	// Trigger sync
	let response = app
		.post(
			&format!("/api/repos/{repo_id}/mirrors/{mirror_id}/sync"),
			Some(owner),
			json!({}),
		)
		.await;

	// Accept 200, 202, or 204 as valid sync trigger responses
	assert!(
		response.status() == StatusCode::OK
			|| response.status() == StatusCode::ACCEPTED
			|| response.status() == StatusCode::NO_CONTENT,
		"Sync trigger should return success status, got {}",
		response.status()
	);
}

/// **Test: SSRF protection - localhost blocked**
#[tokio::test]
async fn test_mirror_ssrf_localhost_blocked() {
	let app = TestApp::new().await;
	let repo_id = create_test_repo(&app, "mirror-ssrf-localhost-test").await;
	let owner = &app.fixtures.org_a.owner;

	let response = app
		.post(
			&format!("/api/repos/{repo_id}/mirrors"),
			Some(owner),
			json!({
				"remote_url": "http://localhost:8080/repo.git"
			}),
		)
		.await;

	assert_eq!(
		response.status(),
		StatusCode::BAD_REQUEST,
		"Localhost URL should be rejected"
	);
}

/// **Test: SSRF protection - private IPs blocked**
#[tokio::test]
async fn test_mirror_ssrf_private_ip_blocked() {
	let app = TestApp::new().await;
	let repo_id = create_test_repo(&app, "mirror-ssrf-private-ip-test").await;
	let owner = &app.fixtures.org_a.owner;

	// 10.x.x.x range
	let response = app
		.post(
			&format!("/api/repos/{repo_id}/mirrors"),
			Some(owner),
			json!({
				"remote_url": "http://10.0.0.1/repo.git"
			}),
		)
		.await;
	assert_eq!(
		response.status(),
		StatusCode::BAD_REQUEST,
		"10.x.x.x should be blocked"
	);

	// 192.168.x.x range
	let response = app
		.post(
			&format!("/api/repos/{repo_id}/mirrors"),
			Some(owner),
			json!({
				"remote_url": "http://192.168.1.1/repo.git"
			}),
		)
		.await;
	assert_eq!(
		response.status(),
		StatusCode::BAD_REQUEST,
		"192.168.x.x should be blocked"
	);

	// 172.16-31.x.x range
	let response = app
		.post(
			&format!("/api/repos/{repo_id}/mirrors"),
			Some(owner),
			json!({
				"remote_url": "http://172.16.0.1/repo.git"
			}),
		)
		.await;
	assert_eq!(
		response.status(),
		StatusCode::BAD_REQUEST,
		"172.16.x.x should be blocked"
	);
}

/// **Test: SSRF protection - cloud metadata blocked**
#[tokio::test]
async fn test_mirror_ssrf_cloud_metadata_blocked() {
	let app = TestApp::new().await;
	let repo_id = create_test_repo(&app, "mirror-ssrf-metadata-test").await;
	let owner = &app.fixtures.org_a.owner;

	// AWS/GCP metadata endpoint
	let response = app
		.post(
			&format!("/api/repos/{repo_id}/mirrors"),
			Some(owner),
			json!({
				"remote_url": "http://169.254.169.254/latest/meta-data/"
			}),
		)
		.await;

	assert_eq!(
		response.status(),
		StatusCode::BAD_REQUEST,
		"Cloud metadata URL should be rejected"
	);
}

/// **Test: Non-admin cannot create mirror**
///
/// Users without write access should get 403 Forbidden.
#[tokio::test]
async fn test_mirror_non_admin_cannot_create() {
	let app = TestApp::new().await;
	let repo_id = create_test_repo(&app, "mirror-auth-test").await;
	let other_user = &app.fixtures.org_b.owner;

	let cases = vec![
		// Other user (not repo owner) cannot create
		AuthzCase {
			name: "other_user_cannot_create_mirror",
			method: Method::POST,
			path: format!("/api/repos/{repo_id}/mirrors"),
			user: Some(other_user.clone()),
			body: Some(json!({
				"remote_url": "https://github.com/org/repo.git"
			})),
			expected_status: StatusCode::FORBIDDEN,
		},
		// Unauthenticated user cannot create
		AuthzCase {
			name: "unauthenticated_cannot_create_mirror",
			method: Method::POST,
			path: format!("/api/repos/{repo_id}/mirrors"),
			user: None,
			body: Some(json!({
				"remote_url": "https://github.com/org/repo.git"
			})),
			expected_status: StatusCode::UNAUTHORIZED,
		},
	];

	run_authz_cases(&app, &cases).await;
}

/// **Test: Non-admin cannot delete mirror**
///
/// Users without admin access should get 403 Forbidden.
#[tokio::test]
async fn test_mirror_non_admin_cannot_delete() {
	let app = TestApp::new().await;
	let repo_id = create_test_repo(&app, "mirror-del-auth-test").await;
	let owner = &app.fixtures.org_a.owner;
	let other_user = &app.fixtures.org_b.owner;

	// Create a mirror first
	let response = app
		.post(
			&format!("/api/repos/{repo_id}/mirrors"),
			Some(owner),
			json!({
				"remote_url": "https://github.com/org/repo.git"
			}),
		)
		.await;
	let body = axum::body::to_bytes(response.into_body(), usize::MAX)
		.await
		.unwrap();
	let mirror: serde_json::Value = serde_json::from_slice(&body).unwrap();
	let mirror_id = mirror["id"].as_str().unwrap();

	let cases = vec![
		// Other user cannot delete
		AuthzCase {
			name: "other_user_cannot_delete_mirror",
			method: Method::DELETE,
			path: format!("/api/repos/{repo_id}/mirrors/{mirror_id}"),
			user: Some(other_user.clone()),
			body: None,
			expected_status: StatusCode::FORBIDDEN,
		},
		// Unauthenticated user cannot delete
		AuthzCase {
			name: "unauthenticated_cannot_delete_mirror",
			method: Method::DELETE,
			path: format!("/api/repos/{repo_id}/mirrors/{mirror_id}"),
			user: None,
			body: None,
			expected_status: StatusCode::UNAUTHORIZED,
		},
	];

	run_authz_cases(&app, &cases).await;
}

/// **Test: Mirror on non-existent repo returns 404**
#[tokio::test]
async fn test_mirror_nonexistent_repo() {
	let app = TestApp::new().await;
	let owner = &app.fixtures.org_a.owner;
	let fake_repo_id = "00000000-0000-0000-0000-000000000000";

	let cases = vec![
		AuthzCase {
			name: "list_mirrors_nonexistent_repo",
			method: Method::GET,
			path: format!("/api/repos/{fake_repo_id}/mirrors"),
			user: Some(owner.clone()),
			body: None,
			expected_status: StatusCode::NOT_FOUND,
		},
		AuthzCase {
			name: "create_mirror_nonexistent_repo",
			method: Method::POST,
			path: format!("/api/repos/{fake_repo_id}/mirrors"),
			user: Some(owner.clone()),
			body: Some(json!({
				"remote_url": "https://github.com/org/repo.git"
			})),
			expected_status: StatusCode::NOT_FOUND,
		},
	];

	run_authz_cases(&app, &cases).await;
}

/// **Test: Delete non-existent mirror returns 404**
#[tokio::test]
async fn test_mirror_delete_nonexistent() {
	let app = TestApp::new().await;
	let repo_id = create_test_repo(&app, "mirror-del-404-test").await;
	let owner = &app.fixtures.org_a.owner;
	let fake_mirror_id = "00000000-0000-0000-0000-000000000000";

	let response = app
		.delete(
			&format!("/api/repos/{repo_id}/mirrors/{fake_mirror_id}"),
			Some(owner),
		)
		.await;
	assert_eq!(
		response.status(),
		StatusCode::NOT_FOUND,
		"Deleting non-existent mirror should return 404"
	);
}

/// **Test: Org admin can manage mirrors on org repo**
#[tokio::test]
async fn test_mirror_org_admin_can_manage() {
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
				"name": "org-mirror-test",
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

	// Org owner can create mirror
	let response = app
		.post(
			&format!("/api/repos/{repo_id}/mirrors"),
			Some(owner),
			json!({
				"remote_url": "https://github.com/org/org-mirror.git"
			}),
		)
		.await;
	assert_eq!(
		response.status(),
		StatusCode::CREATED,
		"Org owner should be able to create mirror on org repo"
	);
}

/// **Test: Org member cannot manage mirrors on org repo**
#[tokio::test]
async fn test_mirror_org_member_cannot_manage() {
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
				"name": "org-mirror-member-test",
				"visibility": "private"
			}),
		)
		.await;
	let body = axum::body::to_bytes(response.into_body(), usize::MAX)
		.await
		.unwrap();
	let repo: serde_json::Value = serde_json::from_slice(&body).unwrap();
	let repo_id = repo["id"].as_str().unwrap();

	// Member cannot create mirror
	let response = app
		.post(
			&format!("/api/repos/{repo_id}/mirrors"),
			Some(member),
			json!({
				"remote_url": "https://github.com/org/repo.git"
			}),
		)
		.await;
	assert_eq!(
		response.status(),
		StatusCode::FORBIDDEN,
		"Org member should not be able to create mirror on org repo"
	);
}
