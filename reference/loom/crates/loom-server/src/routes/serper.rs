// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights
// reserved. SPDX-License-Identifier: Proprietary

//! Serper.dev proxy HTTP handler.

use axum::{extract::State, http::StatusCode, response::IntoResponse, Json};
use loom_server_search_serper::{SerperError, SerperRequest};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{api::AppState, error::ServerError};

#[derive(Debug, Deserialize, ToSchema)]
pub struct SerperProxyRequest {
	pub query: String,
	pub max_results: Option<u32>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct SerperProxyResponse {
	pub query: String,
	pub results: Vec<SerperProxyResultItem>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct SerperProxyResultItem {
	pub title: String,
	pub url: String,
	pub snippet: String,
	pub position: u32,
}

#[utoipa::path(
	post,
	path = "/proxy/serper",
	request_body = SerperProxyRequest,
	responses(
		(status = 200, description = "Search results", body = SerperProxyResponse),
		(status = 400, description = "Invalid request", body = crate::error::ErrorResponse),
		(status = 500, description = "Serper not configured or error", body = crate::error::ErrorResponse)
	),
	tag = "serper"
)]
/// POST /proxy/serper - Proxy requests to Serper.dev Google Search.
#[axum::debug_handler]
pub async fn proxy_serper(
	State(state): State<AppState>,
	Json(body): Json<SerperProxyRequest>,
) -> Result<impl IntoResponse, ServerError> {
	let query = body.query.trim().to_string();
	let max_results = body.max_results.unwrap_or(10).clamp(1, 100);

	if query.is_empty() {
		tracing::warn!("proxy_serper: empty query");
		return Err(ServerError::BadRequest("query must not be empty".into()));
	}

	let client = state.serper_client.as_ref().ok_or_else(|| {
		tracing::error!("proxy_serper: Serper not configured");
		ServerError::Internal("Serper is not configured on the server".to_string())
	})?;

	let request = SerperRequest::new(query.clone(), max_results);

	let serper_response = client.search(request).await.map_err(|e| match e {
		SerperError::Timeout => {
			tracing::warn!("proxy_serper: timeout contacting Serper");
			ServerError::UpstreamTimeout("Serper request timed out".into())
		}
		SerperError::RateLimited => {
			tracing::warn!("proxy_serper: rate limited by Serper");
			ServerError::ServiceUnavailable("Serper rate limit exceeded; try again later".into())
		}
		SerperError::Unauthorized => {
			tracing::error!("proxy_serper: invalid API key");
			ServerError::Internal("Serper authentication failed".into())
		}
		SerperError::Network(e) => {
			tracing::error!(error = %e, "proxy_serper: network error");
			ServerError::UpstreamError(format!("Failed to contact Serper: {e}"))
		}
		SerperError::InvalidResponse(msg) => {
			tracing::error!(error = %msg, "proxy_serper: invalid response");
			ServerError::UpstreamError(format!("Invalid Serper response: {msg}"))
		}
		SerperError::ApiError { status, message } => {
			tracing::warn!(status = status, message = %message, "proxy_serper: API error");
			ServerError::UpstreamError(format!("Serper error: {status} - {message}"))
		}
	})?;

	tracing::info!(
		query = %query,
		results_count = serper_response.results.len(),
		"proxy_serper: returning results"
	);

	let response = SerperProxyResponse {
		query: serper_response.query,
		results: serper_response
			.results
			.into_iter()
			.map(|item| SerperProxyResultItem {
				title: item.title,
				url: item.url,
				snippet: item.snippet,
				position: item.position,
			})
			.collect(),
	};

	Ok((StatusCode::OK, Json(response)))
}
