// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use axum::Json;
use chrono::Utc;
use loom_scim::types::{SCHEMA_CORE_GROUP, SCHEMA_CORE_USER};
use loom_scim::{ListResponse, Meta, ResourceType};

fn user_resource_type() -> ResourceType {
	ResourceType {
		schemas: vec!["urn:ietf:params:scim:schemas:core:2.0:ResourceType".to_string()],
		id: "User".to_string(),
		name: "User".to_string(),
		description: "User Account".to_string(),
		endpoint: "/api/scim/Users".to_string(),
		schema: SCHEMA_CORE_USER.to_string(),
		schema_extensions: vec![],
		meta: Some(Meta {
			resource_type: "ResourceType".to_string(),
			created: Utc::now(),
			last_modified: Utc::now(),
			location: Some("/api/scim/ResourceTypes/User".to_string()),
			version: None,
		}),
	}
}

fn group_resource_type() -> ResourceType {
	ResourceType {
		schemas: vec!["urn:ietf:params:scim:schemas:core:2.0:ResourceType".to_string()],
		id: "Group".to_string(),
		name: "Group".to_string(),
		description: "Group".to_string(),
		endpoint: "/api/scim/Groups".to_string(),
		schema: SCHEMA_CORE_GROUP.to_string(),
		schema_extensions: vec![],
		meta: Some(Meta {
			resource_type: "ResourceType".to_string(),
			created: Utc::now(),
			last_modified: Utc::now(),
			location: Some("/api/scim/ResourceTypes/Group".to_string()),
			version: None,
		}),
	}
}

pub async fn list_resource_types() -> Json<ListResponse<ResourceType>> {
	let resource_types = vec![user_resource_type(), group_resource_type()];
	Json(ListResponse::new(resource_types, 2, 1, 2))
}

pub async fn get_resource_type(
	axum::extract::Path(id): axum::extract::Path<String>,
) -> Result<Json<ResourceType>, axum::http::StatusCode> {
	match id.as_str() {
		"User" => Ok(Json(user_resource_type())),
		"Group" => Ok(Json(group_resource_type())),
		_ => Err(axum::http::StatusCode::NOT_FOUND),
	}
}
