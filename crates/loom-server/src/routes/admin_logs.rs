// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights
// reserved. SPDX-License-Identifier: Proprietary

//! Admin log streaming endpoints.
//!
//! Provides endpoints for viewing and streaming server logs:
//! - GET /api/admin/logs - Paginated recent logs
//! - GET /api/admin/logs/stream - SSE stream of real-time logs
//!
//! # Security
//!
//! All endpoints require `system_admin` role.

use std::convert::Infallible;

use axum::{
	extract::{Query, State},
	response::sse::{Event, Sse},
	Json,
};
use futures::stream::Stream;
use loom_server_logs::{LogEntry, LogLevel};
use serde::{Deserialize, Serialize};
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt;
use utoipa::{IntoParams, ToSchema};

use crate::api::AppState;
use crate::auth_middleware::RequireAuth;

/// Query parameters for listing logs.
#[derive(Debug, Deserialize, IntoParams)]
pub struct ListLogsParams {
	/// Maximum number of log entries to return (default: 100, max: 1000)
	#[param(default = 100)]
	pub limit: Option<usize>,
	/// Minimum log level to include (trace, debug, info, warn, error)
	pub level: Option<String>,
	/// Filter by target prefix (e.g., "loom_server")
	pub target: Option<String>,
	/// Only return entries with ID greater than this value (for pagination/polling)
	pub after_id: Option<u64>,
}

/// Response for listing logs.
#[derive(Debug, Serialize, ToSchema)]
pub struct ListLogsResponse {
	/// Log entries (newest last)
	pub entries: Vec<LogEntry>,
	/// Total entries in buffer
	pub buffer_size: usize,
	/// Buffer capacity
	pub buffer_capacity: usize,
	/// Current entry ID counter (for after_id pagination)
	pub current_id: u64,
}

/// GET /api/admin/logs - List recent server logs.
#[utoipa::path(
    get,
    path = "/api/admin/logs",
    params(ListLogsParams),
    responses(
        (status = 200, description = "Recent log entries", body = ListLogsResponse),
        (status = 401, description = "Not authenticated"),
        (status = 403, description = "Not authorized (system_admin required)")
    ),
    tag = "admin"
)]
#[tracing::instrument(
	skip(state),
	fields(
		actor_id = %current_user.user.id,
		limit = ?params.limit,
		level = ?params.level,
		target = ?params.target
	)
)]
pub async fn list_logs(
	State(state): State<AppState>,
	RequireAuth(current_user): RequireAuth,
	Query(params): Query<ListLogsParams>,
) -> Json<ListLogsResponse> {
	let limit = params.limit.unwrap_or(100).min(1000);

	let min_level = params.level.as_deref().and_then(parse_level);

	let entries =
		state
			.log_buffer
			.get_entries(limit, min_level, params.target.as_deref(), params.after_id);

	Json(ListLogsResponse {
		entries,
		buffer_size: state.log_buffer.len(),
		buffer_capacity: state.log_buffer.capacity(),
		current_id: state.log_buffer.current_id(),
	})
}

/// Query parameters for log streaming.
#[derive(Debug, Deserialize, IntoParams)]
pub struct StreamLogsParams {
	/// Minimum log level to include (trace, debug, info, warn, error)
	pub level: Option<String>,
	/// Filter by target prefix (e.g., "loom_server")
	pub target: Option<String>,
}

/// GET /api/admin/logs/stream - Stream server logs via SSE.
#[utoipa::path(
    get,
    path = "/api/admin/logs/stream",
    params(StreamLogsParams),
    responses(
        (status = 200, description = "SSE log stream", content_type = "text/event-stream"),
        (status = 401, description = "Not authenticated"),
        (status = 403, description = "Not authorized (system_admin required)")
    ),
    tag = "admin"
)]
#[tracing::instrument(
	skip(state),
	fields(
		actor_id = %current_user.user.id,
		level = ?params.level,
		target = ?params.target
	)
)]
pub async fn stream_logs(
	State(state): State<AppState>,
	RequireAuth(current_user): RequireAuth,
	Query(params): Query<StreamLogsParams>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
	let min_level = params.level.as_deref().and_then(parse_level);
	let target_prefix = params.target;

	let rx = state.log_buffer.subscribe();
	let stream = BroadcastStream::new(rx);

	let sse_stream = stream.filter_map(move |result| {
		match result {
			Ok(entry) => {
				// Apply filters
				if let Some(min) = min_level {
					if entry.level < min {
						return None;
					}
				}
				if let Some(ref prefix) = target_prefix {
					if !entry.target.starts_with(prefix) {
						return None;
					}
				}

				// Serialize to JSON
				match serde_json::to_string(&entry) {
					Ok(json) => Some(Ok::<_, Infallible>(Event::default().data(json))),
					Err(_) => None,
				}
			}
			Err(_) => None, // Lagged or closed
		}
	});

	Sse::new(sse_stream).keep_alive(
		axum::response::sse::KeepAlive::new()
			.interval(std::time::Duration::from_secs(15))
			.text("keep-alive"),
	)
}

fn parse_level(s: &str) -> Option<LogLevel> {
	match s.to_lowercase().as_str() {
		"trace" => Some(LogLevel::Trace),
		"debug" => Some(LogLevel::Debug),
		"info" => Some(LogLevel::Info),
		"warn" | "warning" => Some(LogLevel::Warn),
		"error" => Some(LogLevel::Error),
		_ => None,
	}
}
