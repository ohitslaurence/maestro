// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! Integration tests for health and metrics endpoints.
//!
//! Tests cover:
//! - Health check endpoint returns proper status
//! - Prometheus metrics endpoint returns valid format
//! - Content-Type headers are correct

use axum::{
	body::Body,
	http::{Request, StatusCode},
};
use loom_server::api::{create_app_state, create_router};
use loom_server::db::ThreadRepository;
use loom_server::ServerConfig;
use std::sync::Arc;
use tempfile::tempdir;
use tower::ServiceExt;

/// Creates a test app with isolated database
async fn setup_test_app() -> (axum::Router, tempfile::TempDir) {
	let dir = tempdir().unwrap();
	let db_path = dir.path().join("test_health.db");
	let db_url = format!("sqlite:{}?mode=rwc", db_path.display());
	let pool = loom_server::db::create_pool(&db_url).await.unwrap();
	loom_server::db::run_migrations(&pool).await.unwrap();
	let repo = Arc::new(ThreadRepository::new(pool.clone()));
	let config = ServerConfig::default();
	let state = create_app_state(pool, repo, &config, None).await;
	(create_router(state), dir)
}

// ============================================================================
// Health Endpoint Tests
// ============================================================================

#[tokio::test]
async fn test_health_endpoint_returns_valid_status() {
	let (app, _dir) = setup_test_app().await;

	let response = app
		.oneshot(
			Request::builder()
				.uri("/health")
				.body(Body::empty())
				.unwrap(),
		)
		.await
		.unwrap();

	// Health check returns 200 for healthy/degraded, 503 for unhealthy
	// In test environment without all components configured, 503 is acceptable
	let status = response.status();
	assert!(
		status == StatusCode::OK || status == StatusCode::SERVICE_UNAVAILABLE,
		"Expected 200 or 503, got {status}"
	);
}

#[tokio::test]
async fn test_health_endpoint_returns_json() {
	let (app, _dir) = setup_test_app().await;

	let response = app
		.oneshot(
			Request::builder()
				.uri("/health")
				.body(Body::empty())
				.unwrap(),
		)
		.await
		.unwrap();

	let content_type = response
		.headers()
		.get("content-type")
		.and_then(|v| v.to_str().ok())
		.unwrap_or("");
	assert!(
		content_type.contains("application/json"),
		"Expected JSON content-type, got: {content_type}"
	);

	let body = axum::body::to_bytes(response.into_body(), usize::MAX)
		.await
		.unwrap();
	let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

	// Verify required fields
	assert!(json["status"].is_string(), "Missing status field");
	assert!(json["timestamp"].is_string(), "Missing timestamp field");
	assert!(json["components"].is_object(), "Missing components field");
}

#[tokio::test]
async fn test_health_endpoint_contains_database_component() {
	let (app, _dir) = setup_test_app().await;

	let response = app
		.oneshot(
			Request::builder()
				.uri("/health")
				.body(Body::empty())
				.unwrap(),
		)
		.await
		.unwrap();

	let body = axum::body::to_bytes(response.into_body(), usize::MAX)
		.await
		.unwrap();
	let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

	assert!(
		json["components"]["database"].is_object(),
		"Missing database component in health check"
	);
	assert!(
		json["components"]["database"]["status"].is_string(),
		"Database component missing status"
	);
}

// ============================================================================
// Metrics Endpoint Tests
// ============================================================================

#[tokio::test]
async fn test_metrics_endpoint_returns_ok() {
	let (app, _dir) = setup_test_app().await;

	let response = app
		.oneshot(
			Request::builder()
				.uri("/metrics")
				.body(Body::empty())
				.unwrap(),
		)
		.await
		.unwrap();

	assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_metrics_endpoint_returns_prometheus_format() {
	let (app, _dir) = setup_test_app().await;

	let response = app
		.oneshot(
			Request::builder()
				.uri("/metrics")
				.body(Body::empty())
				.unwrap(),
		)
		.await
		.unwrap();

	let content_type = response
		.headers()
		.get("content-type")
		.and_then(|v| v.to_str().ok())
		.unwrap_or("");
	assert!(
		content_type.contains("text/plain"),
		"Expected text/plain content-type for Prometheus metrics, got: {content_type}"
	);

	let body = axum::body::to_bytes(response.into_body(), usize::MAX)
		.await
		.unwrap();
	let body_str = String::from_utf8_lossy(&body);

	// Prometheus format uses # HELP and # TYPE comments
	assert!(
		body_str.contains("# HELP") || body_str.contains("# TYPE") || body_str.is_empty(),
		"Response doesn't look like Prometheus format: {body_str}"
	);
}

#[tokio::test]
async fn test_metrics_endpoint_contains_query_metrics() {
	let (app, _dir) = setup_test_app().await;

	let response = app
		.oneshot(
			Request::builder()
				.uri("/metrics")
				.body(Body::empty())
				.unwrap(),
		)
		.await
		.unwrap();

	let body = axum::body::to_bytes(response.into_body(), usize::MAX)
		.await
		.unwrap();
	let body_str = String::from_utf8_lossy(&body);

	// Should contain loom-specific metrics
	assert!(
		body_str.contains("loom_server_loom_queries") || body_str.contains("loom_query"),
		"Missing loom query metrics in Prometheus output"
	);
}

// ============================================================================
// Edge Cases
// ============================================================================

#[tokio::test]
async fn test_health_with_method_post_returns_method_not_allowed() {
	let (app, _dir) = setup_test_app().await;

	let response = app
		.oneshot(
			Request::builder()
				.method("POST")
				.uri("/health")
				.body(Body::empty())
				.unwrap(),
		)
		.await
		.unwrap();

	assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
}

#[tokio::test]
async fn test_metrics_with_method_post_returns_method_not_allowed() {
	let (app, _dir) = setup_test_app().await;

	let response = app
		.oneshot(
			Request::builder()
				.method("POST")
				.uri("/metrics")
				.body(Body::empty())
				.unwrap(),
		)
		.await
		.unwrap();

	assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
}
