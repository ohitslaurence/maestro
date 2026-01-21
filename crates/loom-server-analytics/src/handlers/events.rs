// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use axum::{http::StatusCode, response::IntoResponse, Json};
use std::sync::Arc;
use tracing::instrument;

use loom_analytics_core::{AnalyticsKeyType, Event};
use loom_server_api::analytics::{
	AnalyticsErrorResponse, CountEventsQuery, CountEventsResponse, EventResponse,
	ExportEventsRequest, ExportEventsResponse, ListEventsQuery, ListEventsResponse,
};

use crate::middleware::AnalyticsApiKeyContext;
use crate::repository::AnalyticsRepository;

use super::capture::AnalyticsState;

fn error_response(status: StatusCode, error: &str, message: &str) -> impl IntoResponse {
	(
		status,
		Json(AnalyticsErrorResponse {
			error: error.to_string(),
			message: message.to_string(),
		}),
	)
}

fn forbidden(message: &str) -> impl IntoResponse {
	error_response(StatusCode::FORBIDDEN, "forbidden", message)
}

fn internal_error(message: &str) -> impl IntoResponse {
	error_response(StatusCode::INTERNAL_SERVER_ERROR, "internal_error", message)
}

fn event_to_response(event: &Event) -> EventResponse {
	EventResponse {
		id: event.id.to_string(),
		org_id: event.org_id.to_string(),
		person_id: event.person_id.map(|id| id.to_string()),
		distinct_id: event.distinct_id.clone(),
		event_name: event.event_name.clone(),
		properties: event.properties.clone(),
		timestamp: event.timestamp,
		user_agent: event.user_agent.clone(),
		lib: event.lib.clone(),
		lib_version: event.lib_version.clone(),
		created_at: event.created_at,
	}
}

#[instrument(skip(state, query))]
pub async fn list_events_impl<R: AnalyticsRepository>(
	state: Arc<AnalyticsState<R>>,
	api_key_ctx: AnalyticsApiKeyContext,
	query: ListEventsQuery,
) -> impl IntoResponse {
	// ReadWrite key required for querying events
	if api_key_ctx.key_type != AnalyticsKeyType::ReadWrite {
		return forbidden("ReadWrite API key required to query events").into_response();
	}

	let limit = query.limit.min(100);
	let offset = query.offset;

	let events = match state
		.repository
		.list_events(
			api_key_ctx.org_id,
			query.distinct_id.as_deref(),
			query.event_name.as_deref(),
			query.start_time,
			query.end_time,
			limit,
			offset,
		)
		.await
	{
		Ok(e) => e,
		Err(e) => {
			tracing::error!(error = %e, "Failed to list events");
			return internal_error("Failed to list events").into_response();
		}
	};

	let total = match state
		.repository
		.count_events(
			api_key_ctx.org_id,
			query.distinct_id.as_deref(),
			query.event_name.as_deref(),
			query.start_time,
			query.end_time,
		)
		.await
	{
		Ok(t) => t,
		Err(e) => {
			tracing::error!(error = %e, "Failed to count events");
			return internal_error("Failed to count events").into_response();
		}
	};

	let event_responses: Vec<EventResponse> = events.iter().map(event_to_response).collect();

	// Update last_used_at for the API key
	if let Err(e) = state
		.repository
		.update_api_key_last_used(api_key_ctx.api_key_id)
		.await
	{
		tracing::warn!(error = %e, "Failed to update API key last_used_at");
	}

	(
		StatusCode::OK,
		Json(ListEventsResponse {
			events: event_responses,
			total,
			limit,
			offset,
		}),
	)
		.into_response()
}

#[instrument(skip(state, query))]
pub async fn count_events_impl<R: AnalyticsRepository>(
	state: Arc<AnalyticsState<R>>,
	api_key_ctx: AnalyticsApiKeyContext,
	query: CountEventsQuery,
) -> impl IntoResponse {
	// ReadWrite key required for querying events
	if api_key_ctx.key_type != AnalyticsKeyType::ReadWrite {
		return forbidden("ReadWrite API key required to query events").into_response();
	}

	let count = match state
		.repository
		.count_events(
			api_key_ctx.org_id,
			query.distinct_id.as_deref(),
			query.event_name.as_deref(),
			query.start_time,
			query.end_time,
		)
		.await
	{
		Ok(c) => c,
		Err(e) => {
			tracing::error!(error = %e, "Failed to count events");
			return internal_error("Failed to count events").into_response();
		}
	};

	// Update last_used_at for the API key
	if let Err(e) = state
		.repository
		.update_api_key_last_used(api_key_ctx.api_key_id)
		.await
	{
		tracing::warn!(error = %e, "Failed to update API key last_used_at");
	}

	(StatusCode::OK, Json(CountEventsResponse { count })).into_response()
}

#[instrument(skip(state, payload))]
pub async fn export_events_impl<R: AnalyticsRepository>(
	state: Arc<AnalyticsState<R>>,
	api_key_ctx: AnalyticsApiKeyContext,
	payload: ExportEventsRequest,
) -> impl IntoResponse {
	// ReadWrite key required for exporting events
	if api_key_ctx.key_type != AnalyticsKeyType::ReadWrite {
		return forbidden("ReadWrite API key required to export events").into_response();
	}

	let limit = payload.limit.min(10000);

	let events = match state
		.repository
		.list_events(
			api_key_ctx.org_id,
			payload.distinct_id.as_deref(),
			payload.event_name.as_deref(),
			payload.start_time,
			payload.end_time,
			limit,
			0,
		)
		.await
	{
		Ok(e) => e,
		Err(e) => {
			tracing::error!(error = %e, "Failed to export events");
			return internal_error("Failed to export events").into_response();
		}
	};

	let event_responses: Vec<EventResponse> = events.iter().map(event_to_response).collect();
	let total_exported = event_responses.len() as u64;

	// Update last_used_at for the API key
	if let Err(e) = state
		.repository
		.update_api_key_last_used(api_key_ctx.api_key_id)
		.await
	{
		tracing::warn!(error = %e, "Failed to update API key last_used_at");
	}

	(
		StatusCode::OK,
		Json(ExportEventsResponse {
			events: event_responses,
			total_exported,
		}),
	)
		.into_response()
}

#[cfg(test)]
mod tests {
	use super::*;
	use loom_analytics_core::OrgId;

	#[test]
	fn event_to_response_converts_correctly() {
		let org_id = OrgId::new();
		let event = Event::new(org_id, "user123".to_string(), "button_clicked".to_string());

		let response = event_to_response(&event);

		assert_eq!(response.id, event.id.to_string());
		assert_eq!(response.org_id, org_id.to_string());
		assert_eq!(response.distinct_id, "user123");
		assert_eq!(response.event_name, "button_clicked");
	}
}
