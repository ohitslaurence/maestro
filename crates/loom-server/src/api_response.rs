// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights
// reserved. SPDX-License-Identifier: Proprietary

//! API response helpers and macros.
//!
//! This module provides common response patterns for HTTP handlers:
//! - Error response helpers (bad_request, conflict, not_found, internal_error)
//! - Macros for early-return error handling (parse_id!, parse_role!, validate_slug_or_error!)
//! - Trait implementations for error response types (impl_api_error_response!)

use axum::{http::StatusCode, Json};
use serde::Serialize;

use crate::validation::{IdParseError, RoleParseError, SlugValidationError};

/// Trait for API error response types that have `error` and `message` fields.
pub trait ApiErrorResponse: Serialize + Send {
	fn new(error: impl Into<String>, message: impl Into<String>) -> Self;
}

/// Implement `ApiErrorResponse` for a struct with `error` and `message` fields.
///
/// # Example
///
/// ```ignore
/// impl_api_error_response!(OrgErrorResponse);
/// ```
#[macro_export]
macro_rules! impl_api_error_response {
	($ty:ty) => {
		impl $crate::api_response::ApiErrorResponse for $ty {
			fn new(error: impl Into<String>, message: impl Into<String>) -> Self {
				Self {
					error: error.into(),
					message: message.into(),
				}
			}
		}
	};
}

/// Parse an ID and return early with an error response if parsing fails.
///
/// # Example
///
/// ```ignore
/// let org_id = parse_id!(
///     OrgErrorResponse,
///     shared_parse_org_id(&org_id, &t(locale, "server.api.org.invalid_id"))
/// );
/// ```
#[macro_export]
macro_rules! parse_id {
	($error_ty:ty, $parse_expr:expr) => {
		match $parse_expr {
			Ok(id) => id,
			Err(e) => {
				return $crate::api_response::id_parse_error::<$error_ty>(e).into_response();
			}
		}
	};
}

/// Parse a role and return early with an error response if parsing fails.
///
/// # Example
///
/// ```ignore
/// let role = parse_role!(
///     OrgErrorResponse,
///     parse_org_role(role_str, &t(locale, "server.api.org.invalid_role"))
/// );
/// ```
#[macro_export]
macro_rules! parse_role {
	($error_ty:ty, $parse_expr:expr) => {
		match $parse_expr {
			Ok(role) => role,
			Err(e) => {
				return $crate::api_response::role_parse_error::<$error_ty>(e).into_response();
			}
		}
	};
}

/// Validate a slug and return early with an error response if validation fails.
///
/// # Example
///
/// ```ignore
/// validate_slug_or_error!(
///     OrgErrorResponse,
///     validate_slug_with_error(
///         &payload.slug,
///         3,
///         50,
///         &t(locale, "server.api.org.invalid_slug_length"),
///         &t(locale, "server.api.org.invalid_slug_format")
///     )
/// );
/// ```
#[macro_export]
macro_rules! validate_slug_or_error {
	($error_ty:ty, $validate_expr:expr) => {
		if let Err(e) = $validate_expr {
			return $crate::api_response::slug_validation_error::<$error_ty>(e).into_response();
		}
	};
}

/// Create a 400 Bad Request response from an IdParseError.
pub fn id_parse_error<T: ApiErrorResponse>(e: IdParseError) -> (StatusCode, Json<T>) {
	(StatusCode::BAD_REQUEST, Json(T::new(e.error, e.message)))
}

/// Create a 400 Bad Request response from a RoleParseError.
pub fn role_parse_error<T: ApiErrorResponse>(e: RoleParseError) -> (StatusCode, Json<T>) {
	(StatusCode::BAD_REQUEST, Json(T::new(e.error, e.message)))
}

/// Create a 400 Bad Request response from a SlugValidationError.
pub fn slug_validation_error<T: ApiErrorResponse>(e: SlugValidationError) -> (StatusCode, Json<T>) {
	(StatusCode::BAD_REQUEST, Json(T::new(e.error, e.message)))
}

/// Create a 400 Bad Request response.
pub fn bad_request<T: ApiErrorResponse>(
	error: impl Into<String>,
	message: impl Into<String>,
) -> (StatusCode, Json<T>) {
	(StatusCode::BAD_REQUEST, Json(T::new(error, message)))
}

/// Create a 409 Conflict response.
pub fn conflict<T: ApiErrorResponse>(
	error: impl Into<String>,
	message: impl Into<String>,
) -> (StatusCode, Json<T>) {
	(StatusCode::CONFLICT, Json(T::new(error, message)))
}

/// Create a 404 Not Found response.
pub fn not_found<T: ApiErrorResponse>(message: impl Into<String>) -> (StatusCode, Json<T>) {
	(StatusCode::NOT_FOUND, Json(T::new("not_found", message)))
}

/// Create a 404 Not Found response with a custom error code.
pub fn not_found_with_code<T: ApiErrorResponse>(
	error: impl Into<String>,
	message: impl Into<String>,
) -> (StatusCode, Json<T>) {
	(StatusCode::NOT_FOUND, Json(T::new(error, message)))
}

/// Create a 500 Internal Server Error response.
pub fn internal_error<T: ApiErrorResponse>(message: impl Into<String>) -> (StatusCode, Json<T>) {
	(
		StatusCode::INTERNAL_SERVER_ERROR,
		Json(T::new("internal_error", message)),
	)
}

/// Create a 403 Forbidden response.
pub fn forbidden<T: ApiErrorResponse>(
	error: impl Into<String>,
	message: impl Into<String>,
) -> (StatusCode, Json<T>) {
	(StatusCode::FORBIDDEN, Json(T::new(error, message)))
}

/// Create a 401 Unauthorized response.
pub fn unauthorized<T: ApiErrorResponse>(
	error: impl Into<String>,
	message: impl Into<String>,
) -> (StatusCode, Json<T>) {
	(StatusCode::UNAUTHORIZED, Json(T::new(error, message)))
}
