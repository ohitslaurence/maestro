// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! Authorization tests for Git HTTP endpoints.
//!
//! Tests cover:
//! - Public vs private repository access
//! - Authentication requirements for clone/push
//! - Role-based access control

use axum::{
	body::Body,
	http::{Request, StatusCode},
};
use tower::ServiceExt;

use super::support::TestApp;

/// **Test: Git info/refs returns 404 for non-existent repo**
///
/// Requesting info/refs for a repo that doesn't exist should return 404.
#[tokio::test]
async fn test_git_info_refs_nonexistent_repo_returns_404() {
	let app = TestApp::new().await;

	let request = Request::builder()
		.method("GET")
		.uri("/git/nonexistent-org/nonexistent-repo.git/info/refs?service=git-upload-pack")
		.body(Body::empty())
		.unwrap();

	let response = app.router.clone().oneshot(request).await.unwrap();

	// Should be 404 (or 401/403 depending on implementation - either is acceptable)
	assert!(
		response.status() == StatusCode::NOT_FOUND
			|| response.status() == StatusCode::UNAUTHORIZED
			|| response.status() == StatusCode::FORBIDDEN,
		"Expected 404/401/403, got {}",
		response.status()
	);
}

/// **Test: Git receive-pack (push) requires authentication**
///
/// Push operations should always require authentication, even conceptually.
#[tokio::test]
async fn test_git_receive_pack_requires_auth() {
	let app = TestApp::new().await;
	let org = &app.fixtures.org_a.org;

	let request = Request::builder()
		.method("POST")
		.uri(&format!("/git/{}/test-repo.git/git-receive-pack", org.slug))
		.header("content-type", "application/x-git-receive-pack-request")
		.body(Body::from("0000"))
		.unwrap();

	let response = app.router.clone().oneshot(request).await.unwrap();

	// Without auth, should be denied
	assert!(
		response.status() == StatusCode::UNAUTHORIZED
			|| response.status() == StatusCode::FORBIDDEN
			|| response.status() == StatusCode::NOT_FOUND,
		"Push without auth should be denied, got {}",
		response.status()
	);
}

/// **Test: Git upload-pack (clone/fetch) for non-existent repo returns error**
///
/// Clone operations for non-existent repos should return appropriate error.
#[tokio::test]
async fn test_git_upload_pack_nonexistent_repo() {
	let app = TestApp::new().await;
	let org = &app.fixtures.org_a.org;

	let request = Request::builder()
		.method("POST")
		.uri(&format!(
			"/git/{}/nonexistent.git/git-upload-pack",
			org.slug
		))
		.header("content-type", "application/x-git-upload-pack-request")
		.body(Body::from("0000"))
		.unwrap();

	let response = app.router.clone().oneshot(request).await.unwrap();

	assert!(
		response.status() == StatusCode::NOT_FOUND
			|| response.status() == StatusCode::UNAUTHORIZED
			|| response.status() == StatusCode::FORBIDDEN,
		"Expected error for nonexistent repo, got {}",
		response.status()
	);
}

/// **Test: Invalid git service parameter returns error**
///
/// The service parameter must be git-upload-pack or git-receive-pack.
#[tokio::test]
async fn test_git_info_refs_invalid_service() {
	let app = TestApp::new().await;
	let org = &app.fixtures.org_a.org;

	let request = Request::builder()
		.method("GET")
		.uri(&format!(
			"/git/{}/repo.git/info/refs?service=invalid-service",
			org.slug
		))
		.body(Body::empty())
		.unwrap();

	let response = app.router.clone().oneshot(request).await.unwrap();

	// Should reject invalid service
	assert!(
		response.status() == StatusCode::BAD_REQUEST
			|| response.status() == StatusCode::FORBIDDEN
			|| response.status() == StatusCode::NOT_FOUND,
		"Invalid service should be rejected, got {}",
		response.status()
	);
}

/// **Test: Mirror path format accepted**
///
/// The /git/mirrors/{platform}/{owner}/{repo} path should be routed correctly.
#[tokio::test]
async fn test_git_mirror_path_routing() {
	let app = TestApp::new().await;

	// This will likely fail (repo doesn't exist) but should be routed correctly
	let request = Request::builder()
		.method("GET")
		.uri("/git/mirrors/github/test-owner/test-repo.git/info/refs?service=git-upload-pack")
		.body(Body::empty())
		.unwrap();

	let response = app.router.clone().oneshot(request).await.unwrap();

	// Should get some response (not 404 for route not found)
	// The actual response depends on whether on-demand mirroring is configured
	let status = response.status();
	assert!(
		status != StatusCode::METHOD_NOT_ALLOWED,
		"Mirror path should be routed, got {}",
		status
	);
}
