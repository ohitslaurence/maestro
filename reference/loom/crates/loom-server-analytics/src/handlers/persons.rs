// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use axum::{http::StatusCode, response::IntoResponse, Json};
use std::sync::Arc;
use tracing::instrument;

use loom_analytics_core::{AnalyticsKeyType, PersonId};
use loom_server_api::analytics::{
	AnalyticsErrorResponse, ListPersonsQuery, ListPersonsResponse, PersonIdentityResponse,
	PersonResponse,
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

fn not_found(message: &str) -> impl IntoResponse {
	error_response(StatusCode::NOT_FOUND, "not_found", message)
}

fn internal_error(message: &str) -> impl IntoResponse {
	error_response(StatusCode::INTERNAL_SERVER_ERROR, "internal_error", message)
}

pub fn person_to_response(person: &loom_analytics_core::PersonWithIdentities) -> PersonResponse {
	PersonResponse {
		id: person.person.id.to_string(),
		org_id: person.person.org_id.to_string(),
		properties: person.person.properties.clone(),
		identities: person
			.identities
			.iter()
			.map(|i| PersonIdentityResponse {
				id: i.id.0.to_string(),
				distinct_id: i.distinct_id.clone(),
				identity_type: i.identity_type.as_str().to_string(),
				created_at: i.created_at,
			})
			.collect(),
		created_at: person.person.created_at,
		updated_at: person.person.updated_at,
	}
}

#[instrument(skip(state, query))]
pub async fn list_persons_impl<R: AnalyticsRepository>(
	state: Arc<AnalyticsState<R>>,
	api_key_ctx: AnalyticsApiKeyContext,
	query: ListPersonsQuery,
) -> impl IntoResponse {
	// ReadWrite key required for querying persons
	if api_key_ctx.key_type != AnalyticsKeyType::ReadWrite {
		return forbidden("ReadWrite API key required to query persons").into_response();
	}

	let limit = query.limit.min(100);
	let offset = query.offset;

	let persons = match state
		.repository
		.list_persons(api_key_ctx.org_id, limit, offset)
		.await
	{
		Ok(p) => p,
		Err(e) => {
			tracing::error!(error = %e, "Failed to list persons");
			return internal_error("Failed to list persons").into_response();
		}
	};

	let total = match state.repository.count_persons(api_key_ctx.org_id).await {
		Ok(t) => t,
		Err(e) => {
			tracing::error!(error = %e, "Failed to count persons");
			return internal_error("Failed to count persons").into_response();
		}
	};

	// Get identities for each person
	let mut person_responses = Vec::with_capacity(persons.len());
	for person in persons {
		let identities = match state.repository.list_identities_for_person(person.id).await {
			Ok(ids) => ids,
			Err(e) => {
				tracing::error!(error = %e, person_id = %person.id, "Failed to get identities");
				return internal_error("Failed to get person identities").into_response();
			}
		};

		let person_with_identities = loom_analytics_core::PersonWithIdentities::new(person, identities);
		person_responses.push(person_to_response(&person_with_identities));
	}

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
		Json(ListPersonsResponse {
			persons: person_responses,
			total,
			limit,
			offset,
		}),
	)
		.into_response()
}

#[instrument(skip(state))]
pub async fn get_person_impl<R: AnalyticsRepository>(
	state: Arc<AnalyticsState<R>>,
	api_key_ctx: AnalyticsApiKeyContext,
	person_id: String,
) -> impl IntoResponse {
	// ReadWrite key required for querying persons
	if api_key_ctx.key_type != AnalyticsKeyType::ReadWrite {
		return forbidden("ReadWrite API key required to query persons").into_response();
	}

	let person_id: PersonId = match person_id.parse() {
		Ok(id) => id,
		Err(_) => {
			return error_response(
				StatusCode::BAD_REQUEST,
				"invalid_id",
				"Invalid person ID format",
			)
			.into_response();
		}
	};

	let person_with_identities = match state.repository.get_person_with_identities(person_id).await {
		Ok(Some(p)) => p,
		Ok(None) => {
			return not_found("Person not found").into_response();
		}
		Err(e) => {
			tracing::error!(error = %e, "Failed to get person");
			return internal_error("Failed to get person").into_response();
		}
	};

	// Verify the person belongs to the same org as the API key
	if person_with_identities.person.org_id != api_key_ctx.org_id {
		return not_found("Person not found").into_response();
	}

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
		Json(person_to_response(&person_with_identities)),
	)
		.into_response()
}

#[instrument(skip(state))]
pub async fn get_person_by_distinct_id_impl<R: AnalyticsRepository>(
	state: Arc<AnalyticsState<R>>,
	api_key_ctx: AnalyticsApiKeyContext,
	distinct_id: String,
) -> impl IntoResponse {
	// ReadWrite key required for querying persons
	if api_key_ctx.key_type != AnalyticsKeyType::ReadWrite {
		return forbidden("ReadWrite API key required to query persons").into_response();
	}

	// Find identity by distinct_id
	let identity = match state
		.repository
		.get_identity_by_distinct_id(api_key_ctx.org_id, &distinct_id)
		.await
	{
		Ok(Some(i)) => i,
		Ok(None) => {
			return not_found("Person not found for distinct_id").into_response();
		}
		Err(e) => {
			tracing::error!(error = %e, "Failed to get identity by distinct_id");
			return internal_error("Failed to get person").into_response();
		}
	};

	let person_with_identities = match state
		.repository
		.get_person_with_identities(identity.person_id)
		.await
	{
		Ok(Some(p)) => p,
		Ok(None) => {
			return not_found("Person not found").into_response();
		}
		Err(e) => {
			tracing::error!(error = %e, "Failed to get person");
			return internal_error("Failed to get person").into_response();
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

	(
		StatusCode::OK,
		Json(person_to_response(&person_with_identities)),
	)
		.into_response()
}

#[cfg(test)]
mod tests {
	use super::*;
	use loom_analytics_core::{OrgId, Person, PersonIdentity, PersonWithIdentities};

	#[test]
	fn person_to_response_converts_correctly() {
		let org_id = OrgId::new();
		let person = Person::new(org_id).with_properties(serde_json::json!({"name": "Alice"}));
		let identity = PersonIdentity::identified(person.id, "user@example.com".to_string());
		let person_with_identities = PersonWithIdentities::new(person.clone(), vec![identity.clone()]);

		let response = person_to_response(&person_with_identities);

		assert_eq!(response.id, person.id.to_string());
		assert_eq!(response.org_id, org_id.to_string());
		assert_eq!(response.properties["name"], "Alice");
		assert_eq!(response.identities.len(), 1);
		assert_eq!(response.identities[0].distinct_id, "user@example.com");
		assert_eq!(response.identities[0].identity_type, "identified");
	}
}
