// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use axum::http::StatusCode;
use serde_json::json;

use super::support::TestApp;

#[tokio::test]
async fn test_weaver_create_org_member_can_create() {
	let app = TestApp::with_provisioner().await;
	let org_a_id = app.fixtures.org_a.org.id.to_string();

	let response = app
		.post(
			"/api/weaver",
			Some(&app.fixtures.org_a.member),
			json!({
				"image": "ubuntu:latest",
				"org_id": org_a_id
			}),
		)
		.await;

	assert_eq!(
		response.status(),
		StatusCode::CREATED,
		"Org member should be able to create weaver for their org"
	);
}

#[tokio::test]
async fn test_weaver_create_org_owner_can_create() {
	let app = TestApp::with_provisioner().await;
	let org_a_id = app.fixtures.org_a.org.id.to_string();

	let response = app
		.post(
			"/api/weaver",
			Some(&app.fixtures.org_a.owner),
			json!({
				"image": "ubuntu:latest",
				"org_id": org_a_id
			}),
		)
		.await;

	assert_eq!(
		response.status(),
		StatusCode::CREATED,
		"Org owner should be able to create weaver for their org"
	);
}

#[tokio::test]
async fn test_weaver_create_non_member_forbidden() {
	let app = TestApp::with_provisioner().await;
	let org_a_id = app.fixtures.org_a.org.id.to_string();

	let response = app
		.post(
			"/api/weaver",
			Some(&app.fixtures.org_b.member),
			json!({
				"image": "ubuntu:latest",
				"org_id": org_a_id
			}),
		)
		.await;

	assert_eq!(
		response.status(),
		StatusCode::FORBIDDEN,
		"Non-member should NOT be able to create weaver for org they don't belong to"
	);
}

#[tokio::test]
async fn test_weaver_create_other_org_owner_forbidden() {
	let app = TestApp::with_provisioner().await;
	let org_a_id = app.fixtures.org_a.org.id.to_string();

	let response = app
		.post(
			"/api/weaver",
			Some(&app.fixtures.org_b.owner),
			json!({
				"image": "ubuntu:latest",
				"org_id": org_a_id
			}),
		)
		.await;

	assert_eq!(
		response.status(),
		StatusCode::FORBIDDEN,
		"Owner of different org should NOT be able to create weaver for another org"
	);
}

#[tokio::test]
async fn test_weaver_create_system_admin_can_create_for_any_org() {
	let app = TestApp::with_provisioner().await;
	let org_a_id = app.fixtures.org_a.org.id.to_string();
	let org_b_id = app.fixtures.org_b.org.id.to_string();

	let response_a = app
		.post(
			"/api/weaver",
			Some(&app.fixtures.admin),
			json!({
				"image": "ubuntu:latest",
				"org_id": org_a_id
			}),
		)
		.await;

	assert_eq!(
		response_a.status(),
		StatusCode::CREATED,
		"System admin should be able to create weaver for any org (org_a)"
	);

	let response_b = app
		.post(
			"/api/weaver",
			Some(&app.fixtures.admin),
			json!({
				"image": "ubuntu:latest",
				"org_id": org_b_id
			}),
		)
		.await;

	assert_eq!(
		response_b.status(),
		StatusCode::CREATED,
		"System admin should be able to create weaver for any org (org_b)"
	);
}

#[tokio::test]
async fn test_weaver_create_invalid_org_id_bad_request() {
	let app = TestApp::with_provisioner().await;

	let response = app
		.post(
			"/api/weaver",
			Some(&app.fixtures.org_a.member),
			json!({
				"image": "ubuntu:latest",
				"org_id": "not-a-valid-uuid"
			}),
		)
		.await;

	assert_eq!(
		response.status(),
		StatusCode::BAD_REQUEST,
		"Invalid org_id format should return 400 Bad Request"
	);
}

#[tokio::test]
async fn test_weaver_create_unauthenticated_unauthorized() {
	let app = TestApp::with_provisioner().await;
	let org_a_id = app.fixtures.org_a.org.id.to_string();

	let response = app
		.post(
			"/api/weaver",
			None,
			json!({
				"image": "ubuntu:latest",
				"org_id": org_a_id
			}),
		)
		.await;

	assert_eq!(
		response.status(),
		StatusCode::UNAUTHORIZED,
		"Unauthenticated request should return 401 Unauthorized"
	);
}

#[tokio::test]
async fn test_weaver_create_nonexistent_org_forbidden() {
	let app = TestApp::with_provisioner().await;
	let fake_org_id = uuid::Uuid::new_v4().to_string();

	let response = app
		.post(
			"/api/weaver",
			Some(&app.fixtures.org_a.member),
			json!({
				"image": "ubuntu:latest",
				"org_id": fake_org_id
			}),
		)
		.await;

	assert_eq!(
		response.status(),
		StatusCode::FORBIDDEN,
		"User cannot create weaver for non-existent org (not a member)"
	);
}
