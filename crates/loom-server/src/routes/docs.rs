// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights
// reserved. SPDX-License-Identifier: Proprietary

//! Documentation search endpoint.

use axum::{
	extract::{Query, State},
	http::StatusCode,
	response::IntoResponse,
	Json,
};
use loom_server_docs::{search_docs, DocSearchHit, DocSearchParams, DocsRepository};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::api::AppState;

#[derive(Debug, Deserialize)]
pub struct SearchQuery {
	pub q: String,
	pub diataxis: Option<String>,
	pub limit: Option<u32>,
	pub offset: Option<u32>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct SearchResponse {
	pub hits: Vec<DocSearchHit>,
	pub limit: u32,
	pub offset: u32,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct SearchError {
	pub error: String,
}

#[utoipa::path(
    get,
    path = "/docs/search",
    params(
        ("q" = String, Query, description = "Search query"),
        ("diataxis" = Option<String>, Query, description = "Filter by diataxis type: tutorial, how-to, reference, explanation"),
        ("limit" = Option<u32>, Query, description = "Maximum results (default: 20, max: 50)"),
        ("offset" = Option<u32>, Query, description = "Pagination offset (default: 0)")
    ),
    responses(
        (status = 200, description = "Search results", body = SearchResponse),
        (status = 400, description = "Invalid query", body = SearchError),
        (status = 500, description = "Search failed", body = SearchError)
    ),
    tag = "docs"
)]
/// GET /docs/search - Search documentation
pub async fn search_handler(
	State(state): State<AppState>,
	Query(query): Query<SearchQuery>,
) -> impl IntoResponse {
	let pool = state.repo.pool();
	let q = query.q.trim();
	if q.is_empty() {
		return (
			StatusCode::BAD_REQUEST,
			Json(SearchError {
				error: "Query parameter 'q' is required".to_string(),
			}),
		)
			.into_response();
	}

	let limit = query.limit.unwrap_or(20).min(50);
	let offset = query.offset.unwrap_or(0);

	if let Some(ref d) = query.diataxis {
		let valid = ["tutorial", "how-to", "reference", "explanation"];
		if !valid.contains(&d.as_str()) {
			return (
				StatusCode::BAD_REQUEST,
				Json(SearchError {
					error: format!(
						"Invalid diataxis value '{}'. Must be one of: {:?}",
						d, valid
					),
				}),
			)
				.into_response();
		}
	}

	let params = DocSearchParams {
		query: q.to_string(),
		diataxis: query.diataxis,
		limit,
		offset,
	};

	let docs_repo = DocsRepository::new(pool.clone());
	match search_docs(&docs_repo, &params).await {
		Ok(hits) => Json(SearchResponse {
			hits,
			limit,
			offset,
		})
		.into_response(),
		Err(e) => {
			tracing::error!("Docs search error: {}", e);
			(
				StatusCode::INTERNAL_SERVER_ERROR,
				Json(SearchError {
					error: "Search failed".to_string(),
				}),
			)
				.into_response()
		}
	}
}
