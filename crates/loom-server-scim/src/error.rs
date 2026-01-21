// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use loom_scim::error::ScimErrorResponse;
use loom_scim::{ScimError, ScimErrorType};
use loom_server_db::DbError;

#[derive(Debug, thiserror::Error)]
pub enum ScimApiError {
	#[error("unauthorized")]
	Unauthorized,
	#[error("not found: {0}")]
	NotFound(String),
	#[error("bad request: {0}")]
	BadRequest(String),
	#[error("conflict: {0}")]
	Conflict(String),
	#[error("internal error: {0}")]
	Internal(String),
	#[error(transparent)]
	Scim(#[from] ScimError),
	#[error(transparent)]
	Database(#[from] sqlx::Error),
}

impl From<DbError> for ScimApiError {
	fn from(e: DbError) -> Self {
		match e {
			DbError::NotFound(msg) => ScimApiError::NotFound(msg),
			DbError::Conflict(msg) => ScimApiError::Conflict(msg),
			DbError::Sqlx(e) => ScimApiError::Database(e),
			DbError::Internal(msg) => ScimApiError::Internal(msg),
			DbError::Serialization(e) => ScimApiError::Internal(e.to_string()),
		}
	}
}

impl IntoResponse for ScimApiError {
	fn into_response(self) -> Response {
		let (status, error_type, detail) = match &self {
			ScimApiError::Unauthorized => (StatusCode::UNAUTHORIZED, None, "Unauthorized".to_string()),
			ScimApiError::NotFound(msg) => (
				StatusCode::NOT_FOUND,
				Some(ScimErrorType::NoTarget),
				msg.clone(),
			),
			ScimApiError::BadRequest(msg) => (
				StatusCode::BAD_REQUEST,
				Some(ScimErrorType::InvalidSyntax),
				msg.clone(),
			),
			ScimApiError::Conflict(msg) => (
				StatusCode::CONFLICT,
				Some(ScimErrorType::Uniqueness),
				msg.clone(),
			),
			ScimApiError::Internal(msg) => (StatusCode::INTERNAL_SERVER_ERROR, None, msg.clone()),
			ScimApiError::Scim(e) => match e {
				ScimError::InvalidFilter(msg) => (
					StatusCode::BAD_REQUEST,
					Some(ScimErrorType::InvalidFilter),
					msg.clone(),
				),
				ScimError::NotFound(msg) => (
					StatusCode::NOT_FOUND,
					Some(ScimErrorType::NoTarget),
					msg.clone(),
				),
				ScimError::Uniqueness(msg) => (
					StatusCode::CONFLICT,
					Some(ScimErrorType::Uniqueness),
					msg.clone(),
				),
				ScimError::InvalidSyntax(msg) => (
					StatusCode::BAD_REQUEST,
					Some(ScimErrorType::InvalidSyntax),
					msg.clone(),
				),
				ScimError::InvalidPath(msg) => (
					StatusCode::BAD_REQUEST,
					Some(ScimErrorType::InvalidPath),
					msg.clone(),
				),
				ScimError::TooMany => (
					StatusCode::PAYLOAD_TOO_LARGE,
					Some(ScimErrorType::TooMany),
					"Too many operations".to_string(),
				),
				_ => (
					StatusCode::BAD_REQUEST,
					Some(ScimErrorType::InvalidValue),
					e.to_string(),
				),
			},
			ScimApiError::Database(e) => (
				StatusCode::INTERNAL_SERVER_ERROR,
				None,
				format!("Database error: {}", e),
			),
		};

		let body = ScimErrorResponse::new(status.as_u16(), error_type, detail);
		(status, Json(body)).into_response()
	}
}
