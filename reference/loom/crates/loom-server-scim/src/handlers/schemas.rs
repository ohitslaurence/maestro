// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use axum::extract::Path;
use axum::http::StatusCode;
use axum::Json;
use loom_scim::types::{SCHEMA_CORE_GROUP, SCHEMA_CORE_USER};
use loom_scim::{ListResponse, Schema, SchemaAttribute};

fn user_schema() -> Schema {
	Schema {
		schemas: vec!["urn:ietf:params:scim:schemas:core:2.0:Schema".to_string()],
		id: SCHEMA_CORE_USER.to_string(),
		name: "User".to_string(),
		description: Some("User Account".to_string()),
		attributes: vec![
			SchemaAttribute {
				name: "userName".to_string(),
				attr_type: "string".to_string(),
				multi_valued: false,
				description: Some("Unique identifier for the user".to_string()),
				required: true,
				canonical_values: None,
				case_exact: false,
				mutability: "readWrite".to_string(),
				returned: "default".to_string(),
				uniqueness: "server".to_string(),
				sub_attributes: vec![],
			},
			SchemaAttribute {
				name: "displayName".to_string(),
				attr_type: "string".to_string(),
				multi_valued: false,
				description: Some("Display name of the user".to_string()),
				required: false,
				canonical_values: None,
				case_exact: false,
				mutability: "readWrite".to_string(),
				returned: "default".to_string(),
				uniqueness: "none".to_string(),
				sub_attributes: vec![],
			},
			SchemaAttribute {
				name: "active".to_string(),
				attr_type: "boolean".to_string(),
				multi_valued: false,
				description: Some("User active status".to_string()),
				required: false,
				canonical_values: None,
				case_exact: false,
				mutability: "readWrite".to_string(),
				returned: "default".to_string(),
				uniqueness: "none".to_string(),
				sub_attributes: vec![],
			},
			SchemaAttribute {
				name: "emails".to_string(),
				attr_type: "complex".to_string(),
				multi_valued: true,
				description: Some("Email addresses".to_string()),
				required: false,
				canonical_values: None,
				case_exact: false,
				mutability: "readWrite".to_string(),
				returned: "default".to_string(),
				uniqueness: "none".to_string(),
				sub_attributes: vec![],
			},
		],
		meta: None,
	}
}

fn group_schema() -> Schema {
	Schema {
		schemas: vec!["urn:ietf:params:scim:schemas:core:2.0:Schema".to_string()],
		id: SCHEMA_CORE_GROUP.to_string(),
		name: "Group".to_string(),
		description: Some("Group".to_string()),
		attributes: vec![
			SchemaAttribute {
				name: "displayName".to_string(),
				attr_type: "string".to_string(),
				multi_valued: false,
				description: Some("Display name of the group".to_string()),
				required: true,
				canonical_values: None,
				case_exact: false,
				mutability: "readWrite".to_string(),
				returned: "default".to_string(),
				uniqueness: "server".to_string(),
				sub_attributes: vec![],
			},
			SchemaAttribute {
				name: "members".to_string(),
				attr_type: "complex".to_string(),
				multi_valued: true,
				description: Some("Group members".to_string()),
				required: false,
				canonical_values: None,
				case_exact: false,
				mutability: "readWrite".to_string(),
				returned: "default".to_string(),
				uniqueness: "none".to_string(),
				sub_attributes: vec![],
			},
		],
		meta: None,
	}
}

pub async fn list_schemas() -> Json<ListResponse<Schema>> {
	let schemas = vec![user_schema(), group_schema()];
	Json(ListResponse::new(schemas, 2, 1, 2))
}

pub async fn get_schema(Path(id): Path<String>) -> Result<Json<Schema>, StatusCode> {
	match id.as_str() {
		s if s == SCHEMA_CORE_USER => Ok(Json(user_schema())),
		s if s == SCHEMA_CORE_GROUP => Ok(Json(group_schema())),
		_ => Err(StatusCode::NOT_FOUND),
	}
}
