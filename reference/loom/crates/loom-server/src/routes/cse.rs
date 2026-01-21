// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights
// reserved. SPDX-License-Identifier: Proprietary

//! Google Custom Search Engine proxy HTTP handler.

use axum::{extract::State, http::StatusCode, response::IntoResponse, Json};
use loom_server_search_google_cse::{CseError, CseRequest};

pub use loom_server_api::cse::*;

use crate::{api::AppState, db::cse::CseCacheExt, error::ServerError};

#[utoipa::path(
    post,
    path = "/proxy/cse",
    request_body = CseProxyRequest,
    responses(
        (status = 200, description = "Search results", body = CseProxyResponse),
        (status = 400, description = "Invalid request", body = crate::error::ErrorResponse),
        (status = 500, description = "CSE not configured or error", body = crate::error::ErrorResponse)
    ),
    tag = "google-cse"
)]
/// POST /proxy/cse - Proxy requests to Google Custom Search Engine.
#[axum::debug_handler]
pub async fn proxy_cse(
	State(state): State<AppState>,
	Json(body): Json<CseProxyRequest>,
) -> Result<impl IntoResponse, ServerError> {
	let query = body.query.trim().to_string();
	let max_results = body.max_results.unwrap_or(5).clamp(1, 10);

	if query.is_empty() {
		tracing::warn!("proxy_cse: empty query");
		return Err(ServerError::BadRequest("query must not be empty".into()));
	}

	// Try cache first
	if let Some(cached) = state.repo.get_cse_cache(&query, max_results).await? {
		tracing::info!(
				query = %query,
				max_results = max_results,
				results_count = cached.results.len(),
				"proxy_cse: returning cached response"
		);

		let response = CseProxyResponse {
			query: cached.query,
			results: cached
				.results
				.into_iter()
				.map(|item| CseProxyResultItem {
					title: item.title,
					url: item.url,
					snippet: item.snippet,
					display_link: item.display_link,
					rank: item.rank,
				})
				.collect(),
		};

		return Ok((StatusCode::OK, Json(response)));
	}

	tracing::debug!(
			query = %query,
			max_results = max_results,
			"proxy_cse: cache miss, calling Google CSE"
	);

	// Get CSE client from state, or return error if not configured
	let client = state.cse_client.as_ref().ok_or_else(|| {
		tracing::error!("proxy_cse: Google CSE not configured");
		ServerError::Internal("Google CSE is not configured on the server".to_string())
	})?;

	let request = CseRequest::new(query.clone(), max_results);

	let cse_response = client.search(request).await.map_err(|e| match e {
		CseError::Timeout => {
			tracing::warn!("proxy_cse: timeout contacting Google CSE");
			ServerError::UpstreamTimeout("Google CSE request timed out".into())
		}
		CseError::RateLimited => {
			tracing::warn!("proxy_cse: rate limited by Google CSE");
			ServerError::ServiceUnavailable("Google CSE rate limit exceeded; try again later".into())
		}
		CseError::Unauthorized => {
			tracing::error!("proxy_cse: invalid API key or CSE ID");
			ServerError::Internal("Google CSE authentication failed".into())
		}
		CseError::Network(e) => {
			tracing::error!(error = %e, "proxy_cse: network error");
			ServerError::UpstreamError(format!("Failed to contact Google CSE: {e}"))
		}
		CseError::InvalidResponse(msg) => {
			tracing::error!(error = %msg, "proxy_cse: invalid response");
			ServerError::UpstreamError(format!("Invalid Google CSE response: {msg}"))
		}
		CseError::ApiError { status, message } => {
			tracing::warn!(status = status, message = %message, "proxy_cse: API error");
			ServerError::UpstreamError(format!("Google CSE error: {status} - {message}"))
		}
	})?;

	// Store in cache
	if let Err(e) = state.repo.put_cse_cache(&cse_response, max_results).await {
		tracing::warn!(error = %e, "proxy_cse: failed to write to cache");
	}

	tracing::info!(
			query = %query,
			results_count = cse_response.results.len(),
			"proxy_cse: returning results"
	);

	let response = CseProxyResponse {
		query: cse_response.query,
		results: cse_response
			.results
			.into_iter()
			.map(|item| CseProxyResultItem {
				title: item.title,
				url: item.url,
				snippet: item.snippet,
				display_link: item.display_link,
				rank: item.rank,
			})
			.collect(),
	};

	Ok((StatusCode::OK, Json(response)))
}
