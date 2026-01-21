// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! Authorization tests for Webhook endpoints.
//!
//! Tests cover:
//! - CRUD operations on webhooks
//! - Authorization (only repo admins can manage webhooks)
//! - SSRF protection (blocking localhost, private IPs, cloud metadata)
//! - Event validation
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

/// **Test: Create webhook**
///
/// Repo admin can create a webhook.
#[tokio::test]
async fn test_webhook_create_success() {
	let app = TestApp::new().await;
	let repo_id = create_test_repo(&app, "webhook-create-test").await;
	let owner = &app.fixtures.org_a.owner;

	let response = app
		.post(
			&format!("/api/repos/{repo_id}/webhooks"),
			Some(owner),
			json!({
				"url": "https://example.com/webhook",
				"secret": "my-secret-key",
				"payload_format": "loom-v1",
				"events": ["push", "repo.created"]
			}),
		)
		.await;

	assert_eq!(
		response.status(),
		StatusCode::CREATED,
		"Admin should be able to create webhook"
	);

	// Verify response contains expected fields
	let body = axum::body::to_bytes(response.into_body(), usize::MAX)
		.await
		.unwrap();
	let webhook: serde_json::Value = serde_json::from_slice(&body).unwrap();

	assert!(webhook.get("id").is_some(), "Response should have 'id'");
	assert_eq!(
		webhook["url"].as_str().unwrap(),
		"https://example.com/webhook"
	);
	assert_eq!(webhook["payload_format"].as_str().unwrap(), "loom-v1");
	assert!(webhook["enabled"].as_bool().unwrap());
	// Secret should NOT be returned in response for security
	assert!(
		webhook.get("secret").is_none(),
		"Secret should not be in response"
	);
}

/// **Test: Create webhook with github-compat format**
///
/// The API accepts "github-compat" as the payload format (matches spec and database).
#[tokio::test]
async fn test_webhook_create_github_compat_format() {
	let app = TestApp::new().await;
	let repo_id = create_test_repo(&app, "webhook-github-format-test").await;
	let owner = &app.fixtures.org_a.owner;

	let response = app
		.post(
			&format!("/api/repos/{repo_id}/webhooks"),
			Some(owner),
			json!({
				"url": "https://example.com/github-webhook",
				"secret": "secret",
				"payload_format": "github-compat",
				"events": ["push"]
			}),
		)
		.await;

	assert_eq!(response.status(), StatusCode::CREATED);

	let body = axum::body::to_bytes(response.into_body(), usize::MAX)
		.await
		.unwrap();
	let webhook: serde_json::Value = serde_json::from_slice(&body).unwrap();
	assert_eq!(webhook["payload_format"].as_str().unwrap(), "github-compat");
}

/// **Test: List webhooks**
///
/// Admin can list all webhooks for a repo.
#[tokio::test]
async fn test_webhook_list() {
	let app = TestApp::new().await;
	let repo_id = create_test_repo(&app, "webhook-list-test").await;
	let owner = &app.fixtures.org_a.owner;

	// Initially empty
	let response = app
		.get(&format!("/api/repos/{repo_id}/webhooks"), Some(owner))
		.await;
	assert_eq!(response.status(), StatusCode::OK);

	let body = axum::body::to_bytes(response.into_body(), usize::MAX)
		.await
		.unwrap();
	let result: serde_json::Value = serde_json::from_slice(&body).unwrap();
	assert!(result["webhooks"].as_array().unwrap().is_empty());

	// Create a webhook
	app
		.post(
			&format!("/api/repos/{repo_id}/webhooks"),
			Some(owner),
			json!({
				"url": "https://example.com/hook1",
				"secret": "s1",
				"payload_format": "loom-v1",
				"events": ["push"]
			}),
		)
		.await;

	// List should now have one webhook
	let response = app
		.get(&format!("/api/repos/{repo_id}/webhooks"), Some(owner))
		.await;
	assert_eq!(response.status(), StatusCode::OK);

	let body = axum::body::to_bytes(response.into_body(), usize::MAX)
		.await
		.unwrap();
	let result: serde_json::Value = serde_json::from_slice(&body).unwrap();
	assert_eq!(result["webhooks"].as_array().unwrap().len(), 1);
	assert_eq!(
		result["webhooks"][0]["url"].as_str().unwrap(),
		"https://example.com/hook1"
	);
}

/// **Test: Delete webhook**
///
/// Admin can delete a webhook.
#[tokio::test]
async fn test_webhook_delete() {
	let app = TestApp::new().await;
	let repo_id = create_test_repo(&app, "webhook-delete-test").await;
	let owner = &app.fixtures.org_a.owner;

	// Create a webhook
	let response = app
		.post(
			&format!("/api/repos/{repo_id}/webhooks"),
			Some(owner),
			json!({
				"url": "https://example.com/to-delete",
				"secret": "s",
				"payload_format": "loom-v1",
				"events": ["push"]
			}),
		)
		.await;

	let body = axum::body::to_bytes(response.into_body(), usize::MAX)
		.await
		.unwrap();
	let webhook: serde_json::Value = serde_json::from_slice(&body).unwrap();
	let webhook_id = webhook["id"].as_str().unwrap();

	// Delete the webhook
	let response = app
		.delete(
			&format!("/api/repos/{repo_id}/webhooks/{webhook_id}"),
			Some(owner),
		)
		.await;
	assert_eq!(response.status(), StatusCode::NO_CONTENT);

	// Verify it's gone
	let response = app
		.get(&format!("/api/repos/{repo_id}/webhooks"), Some(owner))
		.await;
	let body = axum::body::to_bytes(response.into_body(), usize::MAX)
		.await
		.unwrap();
	let result: serde_json::Value = serde_json::from_slice(&body).unwrap();
	assert!(result["webhooks"].as_array().unwrap().is_empty());
}

/// **Test: Invalid events rejected**
///
/// Creating a webhook with invalid events should return 400.
#[tokio::test]
async fn test_webhook_invalid_events() {
	let app = TestApp::new().await;
	let repo_id = create_test_repo(&app, "webhook-invalid-events-test").await;
	let owner = &app.fixtures.org_a.owner;

	let response = app
		.post(
			&format!("/api/repos/{repo_id}/webhooks"),
			Some(owner),
			json!({
				"url": "https://example.com/hook",
				"secret": "s",
				"payload_format": "loom-v1",
				"events": ["invalid_event"]
			}),
		)
		.await;

	assert_eq!(
		response.status(),
		StatusCode::BAD_REQUEST,
		"Invalid events should be rejected"
	);
}

/// **Test: SSRF protection - localhost blocked**
#[tokio::test]
async fn test_webhook_ssrf_localhost_blocked() {
	let app = TestApp::new().await;
	let repo_id = create_test_repo(&app, "webhook-ssrf-localhost-test").await;
	let owner = &app.fixtures.org_a.owner;

	let response = app
		.post(
			&format!("/api/repos/{repo_id}/webhooks"),
			Some(owner),
			json!({
				"url": "http://localhost:8080/internal",
				"secret": "s",
				"payload_format": "loom-v1",
				"events": ["push"]
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
async fn test_webhook_ssrf_private_ip_blocked() {
	let app = TestApp::new().await;
	let repo_id = create_test_repo(&app, "webhook-ssrf-private-ip-test").await;
	let owner = &app.fixtures.org_a.owner;

	// 10.x.x.x range
	let response = app
		.post(
			&format!("/api/repos/{repo_id}/webhooks"),
			Some(owner),
			json!({
				"url": "http://10.0.0.1/webhook",
				"secret": "s",
				"payload_format": "loom-v1",
				"events": ["push"]
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
			&format!("/api/repos/{repo_id}/webhooks"),
			Some(owner),
			json!({
				"url": "http://192.168.1.1/webhook",
				"secret": "s",
				"payload_format": "loom-v1",
				"events": ["push"]
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
			&format!("/api/repos/{repo_id}/webhooks"),
			Some(owner),
			json!({
				"url": "http://172.16.0.1/webhook",
				"secret": "s",
				"payload_format": "loom-v1",
				"events": ["push"]
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
async fn test_webhook_ssrf_cloud_metadata_blocked() {
	let app = TestApp::new().await;
	let repo_id = create_test_repo(&app, "webhook-ssrf-metadata-test").await;
	let owner = &app.fixtures.org_a.owner;

	// AWS/GCP metadata endpoint
	let response = app
		.post(
			&format!("/api/repos/{repo_id}/webhooks"),
			Some(owner),
			json!({
				"url": "http://169.254.169.254/latest/meta-data/",
				"secret": "s",
				"payload_format": "loom-v1",
				"events": ["push"]
			}),
		)
		.await;

	assert_eq!(
		response.status(),
		StatusCode::BAD_REQUEST,
		"Cloud metadata URL should be rejected"
	);
}

/// **Test: Non-admin cannot create webhook**
///
/// Users without admin access should get 403 Forbidden.
#[tokio::test]
async fn test_webhook_non_admin_cannot_create() {
	let app = TestApp::new().await;
	let repo_id = create_test_repo(&app, "webhook-auth-test").await;
	let other_user = &app.fixtures.org_b.owner;

	let cases = vec![
		// Other user (not repo owner) cannot create
		AuthzCase {
			name: "other_user_cannot_create_webhook",
			method: Method::POST,
			path: format!("/api/repos/{repo_id}/webhooks"),
			user: Some(other_user.clone()),
			body: Some(json!({
				"url": "https://example.com/hook",
				"secret": "s",
				"payload_format": "loom-v1",
				"events": ["push"]
			})),
			expected_status: StatusCode::FORBIDDEN,
		},
		// Unauthenticated user cannot create
		AuthzCase {
			name: "unauthenticated_cannot_create_webhook",
			method: Method::POST,
			path: format!("/api/repos/{repo_id}/webhooks"),
			user: None,
			body: Some(json!({
				"url": "https://example.com/hook",
				"secret": "s",
				"payload_format": "loom-v1",
				"events": ["push"]
			})),
			expected_status: StatusCode::UNAUTHORIZED,
		},
	];

	run_authz_cases(&app, &cases).await;
}

/// **Test: Non-admin cannot delete webhook**
///
/// Users without admin access should get 403 Forbidden.
#[tokio::test]
async fn test_webhook_non_admin_cannot_delete() {
	let app = TestApp::new().await;
	let repo_id = create_test_repo(&app, "webhook-del-auth-test").await;
	let owner = &app.fixtures.org_a.owner;
	let other_user = &app.fixtures.org_b.owner;

	// Create a webhook first
	let response = app
		.post(
			&format!("/api/repos/{repo_id}/webhooks"),
			Some(owner),
			json!({
				"url": "https://example.com/hook",
				"secret": "s",
				"payload_format": "loom-v1",
				"events": ["push"]
			}),
		)
		.await;
	let body = axum::body::to_bytes(response.into_body(), usize::MAX)
		.await
		.unwrap();
	let webhook: serde_json::Value = serde_json::from_slice(&body).unwrap();
	let webhook_id = webhook["id"].as_str().unwrap();

	let cases = vec![
		// Other user cannot delete
		AuthzCase {
			name: "other_user_cannot_delete_webhook",
			method: Method::DELETE,
			path: format!("/api/repos/{repo_id}/webhooks/{webhook_id}"),
			user: Some(other_user.clone()),
			body: None,
			expected_status: StatusCode::FORBIDDEN,
		},
		// Unauthenticated user cannot delete
		AuthzCase {
			name: "unauthenticated_cannot_delete_webhook",
			method: Method::DELETE,
			path: format!("/api/repos/{repo_id}/webhooks/{webhook_id}"),
			user: None,
			body: None,
			expected_status: StatusCode::UNAUTHORIZED,
		},
	];

	run_authz_cases(&app, &cases).await;
}

/// **Test: Webhook on non-existent repo returns 404**
#[tokio::test]
async fn test_webhook_nonexistent_repo() {
	let app = TestApp::new().await;
	let owner = &app.fixtures.org_a.owner;
	let fake_repo_id = "00000000-0000-0000-0000-000000000000";

	let cases = vec![
		AuthzCase {
			name: "list_webhooks_nonexistent_repo",
			method: Method::GET,
			path: format!("/api/repos/{fake_repo_id}/webhooks"),
			user: Some(owner.clone()),
			body: None,
			expected_status: StatusCode::NOT_FOUND,
		},
		AuthzCase {
			name: "create_webhook_nonexistent_repo",
			method: Method::POST,
			path: format!("/api/repos/{fake_repo_id}/webhooks"),
			user: Some(owner.clone()),
			body: Some(json!({
				"url": "https://example.com/hook",
				"secret": "s",
				"payload_format": "loom-v1",
				"events": ["push"]
			})),
			expected_status: StatusCode::NOT_FOUND,
		},
	];

	run_authz_cases(&app, &cases).await;
}

/// **Test: Delete non-existent webhook returns 404**
#[tokio::test]
async fn test_webhook_delete_nonexistent() {
	let app = TestApp::new().await;
	let repo_id = create_test_repo(&app, "webhook-del-404-test").await;
	let owner = &app.fixtures.org_a.owner;
	let fake_webhook_id = "00000000-0000-0000-0000-000000000000";

	let response = app
		.delete(
			&format!("/api/repos/{repo_id}/webhooks/{fake_webhook_id}"),
			Some(owner),
		)
		.await;
	assert_eq!(
		response.status(),
		StatusCode::NOT_FOUND,
		"Deleting non-existent webhook should return 404"
	);
}

/// **Test: Org admin can manage webhooks on org repo**
#[tokio::test]
async fn test_webhook_org_admin_can_manage() {
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
				"name": "org-webhook-test",
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

	// Org owner can create webhook
	let response = app
		.post(
			&format!("/api/repos/{repo_id}/webhooks"),
			Some(owner),
			json!({
				"url": "https://example.com/org-hook",
				"secret": "s",
				"payload_format": "loom-v1",
				"events": ["push"]
			}),
		)
		.await;
	assert_eq!(
		response.status(),
		StatusCode::CREATED,
		"Org owner should be able to create webhook on org repo"
	);
}

/// **Test: Org member cannot manage webhooks on org repo**
#[tokio::test]
async fn test_webhook_org_member_cannot_manage() {
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
				"name": "org-webhook-member-test",
				"visibility": "private"
			}),
		)
		.await;
	let body = axum::body::to_bytes(response.into_body(), usize::MAX)
		.await
		.unwrap();
	let repo: serde_json::Value = serde_json::from_slice(&body).unwrap();
	let repo_id = repo["id"].as_str().unwrap();

	// Member cannot create webhook
	let response = app
		.post(
			&format!("/api/repos/{repo_id}/webhooks"),
			Some(member),
			json!({
				"url": "https://example.com/hook",
				"secret": "s",
				"payload_format": "loom-v1",
				"events": ["push"]
			}),
		)
		.await;
	assert_eq!(
		response.status(),
		StatusCode::FORBIDDEN,
		"Org member should not be able to create webhook on org repo"
	);
}
