// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights
// reserved. SPDX-License-Identifier: Proprietary

//! Server error types and HTTP response conversions.

use axum::{
	http::StatusCode,
	response::{IntoResponse, Response},
	Json,
};
use serde::Serialize;
use utoipa::ToSchema;

/// Server error types for thread operations.
#[derive(Debug, thiserror::Error)]
pub enum ServerError {
	/// Database operation failed.
	#[error("Database error: {0}")]
	Db(#[from] sqlx::Error),

	/// Database error from loom-db.
	#[error("Database error: {0}")]
	DbError(#[from] crate::db::DbError),

	/// Thread not found.
	#[error("Thread not found: {0}")]
	NotFound(String),

	/// Version conflict during update.
	#[error("Version conflict: expected {expected}, got {actual}")]
	Conflict { expected: u64, actual: u64 },

	/// Invalid request payload.
	#[error("Invalid request: {0}")]
	BadRequest(String),

	/// Internal server error.
	#[error("Internal error: {0}")]
	Internal(String),

	/// Serialization error.
	#[error("Serialization error: {0}")]
	Serialization(#[from] serde_json::Error),

	/// Upstream service returned an error.
	#[error("Upstream error: {0}")]
	UpstreamError(String),

	/// Upstream service timed out.
	#[error("Upstream timeout: {0}")]
	UpstreamTimeout(String),

	/// Service temporarily unavailable (e.g., rate limited).
	#[error("Service unavailable: {0}")]
	ServiceUnavailable(String),

	/// Unauthorized (authentication failed).
	#[error("Unauthorized: {0}")]
	Unauthorized(String),

	/// Forbidden (insufficient permissions).
	#[error("Forbidden: {0}")]
	Forbidden(String),

	/// Not implemented (placeholder for future functionality).
	#[error("Not implemented: {0}")]
	NotImplemented(String),

	/// Weaver provisioner error.
	#[error("Provisioner error: {0}")]
	Provisioner(#[from] loom_server_weaver::ProvisionerError),
}

/// Error response body.
#[derive(Debug, Serialize, ToSchema)]
pub struct ErrorResponse {
	pub error: String,
	pub message: String,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub server_version: Option<u64>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub client_version: Option<u64>,
}

impl IntoResponse for ServerError {
	fn into_response(self) -> Response {
		let (status, error_response) = match &self {
			ServerError::Db(e) => {
				tracing::error!(error = %e, "database error");
				(
					StatusCode::INTERNAL_SERVER_ERROR,
					ErrorResponse {
						error: "database_error".to_string(),
						message: "A database error occurred".to_string(),
						server_version: None,
						client_version: None,
					},
				)
			}
			ServerError::DbError(e) => {
				tracing::error!(error = %e, "database error");
				(
					StatusCode::INTERNAL_SERVER_ERROR,
					ErrorResponse {
						error: "database_error".to_string(),
						message: "A database error occurred".to_string(),
						server_version: None,
						client_version: None,
					},
				)
			}
			ServerError::NotFound(id) => (
				StatusCode::NOT_FOUND,
				ErrorResponse {
					error: "not_found".to_string(),
					message: format!("Thread not found: {id}"),
					server_version: None,
					client_version: None,
				},
			),
			ServerError::Conflict { expected, actual } => (
				StatusCode::CONFLICT,
				ErrorResponse {
					error: "conflict".to_string(),
					message: format!("Version conflict: expected {expected}, got {actual}"),
					server_version: Some(*expected),
					client_version: Some(*actual),
				},
			),
			ServerError::BadRequest(msg) => (
				StatusCode::BAD_REQUEST,
				ErrorResponse {
					error: "bad_request".to_string(),
					message: msg.clone(),
					server_version: None,
					client_version: None,
				},
			),
			ServerError::Internal(msg) => {
				tracing::error!(error = %msg, "internal error");
				(
					StatusCode::INTERNAL_SERVER_ERROR,
					ErrorResponse {
						error: "internal_error".to_string(),
						message: "An internal error occurred".to_string(),
						server_version: None,
						client_version: None,
					},
				)
			}
			ServerError::Serialization(e) => (
				StatusCode::BAD_REQUEST,
				ErrorResponse {
					error: "serialization_error".to_string(),
					message: format!("Invalid JSON: {e}"),
					server_version: None,
					client_version: None,
				},
			),
			ServerError::UpstreamError(msg) => {
				tracing::warn!(error = %msg, "upstream error");
				(
					StatusCode::BAD_GATEWAY,
					ErrorResponse {
						error: "upstream_error".to_string(),
						message: msg.clone(),
						server_version: None,
						client_version: None,
					},
				)
			}
			ServerError::UpstreamTimeout(msg) => {
				tracing::warn!(error = %msg, "upstream timeout");
				(
					StatusCode::GATEWAY_TIMEOUT,
					ErrorResponse {
						error: "upstream_timeout".to_string(),
						message: msg.clone(),
						server_version: None,
						client_version: None,
					},
				)
			}
			ServerError::ServiceUnavailable(msg) => {
				tracing::warn!(error = %msg, "service unavailable");
				(
					StatusCode::SERVICE_UNAVAILABLE,
					ErrorResponse {
						error: "service_unavailable".to_string(),
						message: msg.clone(),
						server_version: None,
						client_version: None,
					},
				)
			}
			ServerError::Unauthorized(msg) => {
				tracing::warn!(error = %msg, "unauthorized");
				(
					StatusCode::UNAUTHORIZED,
					ErrorResponse {
						error: "unauthorized".to_string(),
						message: msg.clone(),
						server_version: None,
						client_version: None,
					},
				)
			}
			ServerError::Forbidden(msg) => {
				tracing::warn!(error = %msg, "forbidden");
				(
					StatusCode::FORBIDDEN,
					ErrorResponse {
						error: "forbidden".to_string(),
						message: msg.clone(),
						server_version: None,
						client_version: None,
					},
				)
			}
			ServerError::NotImplemented(msg) => (
				StatusCode::NOT_IMPLEMENTED,
				ErrorResponse {
					error: "not_implemented".to_string(),
					message: msg.clone(),
					server_version: None,
					client_version: None,
				},
			),
			ServerError::Provisioner(e) => {
				use loom_server_weaver::ProvisionerError;
				match e {
					ProvisionerError::WeaverNotFound { id } => (
						StatusCode::NOT_FOUND,
						ErrorResponse {
							error: "weaver_not_found".to_string(),
							message: format!("Weaver not found: {id}"),
							server_version: None,
							client_version: None,
						},
					),
					ProvisionerError::TooManyWeavers { current, max } => (
						StatusCode::TOO_MANY_REQUESTS,
						ErrorResponse {
							error: "too_many_weavers".to_string(),
							message: format!("{current} weavers running (max: {max})"),
							server_version: None,
							client_version: None,
						},
					),
					ProvisionerError::InvalidLifetime { requested, max } => (
						StatusCode::BAD_REQUEST,
						ErrorResponse {
							error: "invalid_lifetime".to_string(),
							message: format!("{requested} hours exceeds max {max} hours"),
							server_version: None,
							client_version: None,
						},
					),
					_ => {
						tracing::error!(error = %e, "provisioner error");
						(
							StatusCode::INTERNAL_SERVER_ERROR,
							ErrorResponse {
								error: "provisioner_error".to_string(),
								message: e.to_string(),
								server_version: None,
								client_version: None,
							},
						)
					}
				}
			}
		};

		(status, Json(error_response)).into_response()
	}
}
