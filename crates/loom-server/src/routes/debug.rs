// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights
// reserved. SPDX-License-Identifier: Proprietary

//! Debug and tracing HTTP handlers.

use axum::{
	extract::{Path, Query, State},
	response::IntoResponse,
	Json,
};

use crate::{api::AppState, error::ServerError, query_tracing::TraceTimeline};

#[utoipa::path(
    get,
    path = "/api/debug/query-traces/{trace_id}",
    params(
        ("trace_id" = String, Path, description = "Trace ID")
    ),
    responses(
        (status = 200, description = "Trace timeline"),
        (status = 404, description = "Trace not found", body = crate::error::ErrorResponse)
    ),
    tag = "debug"
)]
/// GET /api/debug/query-traces/{trace_id} - Get a query trace by ID.
///
/// Returns the full trace timeline with all events and their durations.
#[axum::debug_handler]
pub async fn get_query_trace(
	State(state): State<AppState>,
	Path(trace_id): Path<String>,
) -> Result<impl IntoResponse, ServerError> {
	tracing::debug!(trace_id = %trace_id, "fetching query trace");

	let tracer = state
		.trace_store
		.get(&trace_id)
		.await
		.ok_or_else(|| ServerError::NotFound(format!("Trace not found: {trace_id}")))?;

	let timeline = TraceTimeline::from_tracer(&tracer);

	tracing::info!(
			trace_id = %trace_id,
			query_id = %timeline.query_id,
			total_duration_ms = timeline.total_duration_ms,
			"returning query trace"
	);

	Ok(Json(timeline))
}

#[utoipa::path(
    get,
    path = "/api/debug/query-traces",
    params(
        ("session_id" = Option<String>, Query, description = "Filter by session ID")
    ),
    responses(
        (status = 200, description = "List of traces")
    ),
    tag = "debug"
)]
/// GET /api/debug/query-traces - List all trace IDs.
///
/// Optionally filter by session_id query parameter.
#[axum::debug_handler]
pub async fn list_query_traces(
	State(state): State<AppState>,
	Query(params): Query<std::collections::HashMap<String, String>>,
) -> Result<impl IntoResponse, ServerError> {
	let session_id = params.get("session_id").cloned();

	tracing::debug!(session_id = ?session_id, "listing query traces");

	let traces = if let Some(session_id) = session_id {
		state.trace_store.get_session_traces(&session_id).await
	} else {
		let trace_ids = state.trace_store.list_trace_ids().await;
		// Convert IDs back to tracers (simple version)
		let mut results = Vec::new();
		for id in trace_ids {
			if let Some(tracer) = state.trace_store.get(&id).await {
				results.push(tracer);
			}
		}
		results
	};

	let response = serde_json::json!({
			"traces": traces.iter().map(|t| {
					serde_json::json!({
							"trace_id": t.trace_id.as_str(),
							"query_id": t.query_id,
							"session_id": t.session_id,
							"event_count": t.events.len(),
							"total_duration_ms": t.total_duration().as_millis() as u64,
							"has_error": t.has_error(),
					})
			}).collect::<Vec<_>>(),
			"count": traces.len(),
	});

	Ok(Json(response))
}

#[utoipa::path(
    get,
    path = "/api/debug/query-traces/stats",
    responses(
        (status = 200, description = "Trace statistics")
    ),
    tag = "debug"
)]
/// GET /api/debug/query-traces/stats - Get trace store statistics.
///
/// Returns aggregated statistics about all traces in the store.
#[axum::debug_handler]
pub async fn get_trace_stats(
	State(state): State<AppState>,
) -> Result<impl IntoResponse, ServerError> {
	use std::time::Duration;

	tracing::debug!("fetching trace store statistics");

	let stats = state.trace_store.get_stats().await;
	let slow_traces = state
		.trace_store
		.get_slow_traces(Duration::from_secs(5))
		.await;

	let response = serde_json::json!({
			"total_traces": stats.total_traces,
			"traces_with_errors": stats.traces_with_errors,
			"slow_traces": stats.slow_traces,
			"avg_events_per_trace": stats.avg_events_per_trace,
			"slow_trace_details": slow_traces,
	});

	tracing::info!(
		total_traces = stats.total_traces,
		error_traces = stats.traces_with_errors,
		slow_traces = stats.slow_traces,
		"returning trace statistics"
	);

	Ok(Json(response))
}
