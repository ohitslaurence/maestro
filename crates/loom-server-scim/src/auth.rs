// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use axum::{
	extract::{Request, State},
	http::StatusCode,
	middleware::Next,
	response::Response,
};
use loom_common_secret::SecretString;
use subtle::ConstantTimeEq;
use tracing::warn;

pub async fn scim_auth_middleware(
	State(expected_token): State<Option<SecretString>>,
	request: Request,
	next: Next,
) -> Result<Response, StatusCode> {
	let Some(expected) = expected_token else {
		warn!("SCIM auth failed: no token configured");
		return Err(StatusCode::UNAUTHORIZED);
	};

	let auth_header = request
		.headers()
		.get("Authorization")
		.and_then(|h| h.to_str().ok());

	let Some(auth_value) = auth_header else {
		warn!("SCIM auth failed: missing Authorization header");
		return Err(StatusCode::UNAUTHORIZED);
	};

	let token = if let Some(bearer) = auth_value.strip_prefix("Bearer ") {
		bearer.trim()
	} else {
		warn!("SCIM auth failed: invalid Authorization format");
		return Err(StatusCode::UNAUTHORIZED);
	};

	let expected_bytes = expected.expose().as_bytes();
	let token_bytes = token.as_bytes();

	if expected_bytes.len() != token_bytes.len() {
		warn!("SCIM auth failed: token length mismatch");
		return Err(StatusCode::UNAUTHORIZED);
	}

	if expected_bytes.ct_eq(token_bytes).into() {
		Ok(next.run(request).await)
	} else {
		warn!("SCIM auth failed: invalid token");
		Err(StatusCode::UNAUTHORIZED)
	}
}
