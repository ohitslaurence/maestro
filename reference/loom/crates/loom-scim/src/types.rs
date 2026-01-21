// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

pub const SCHEMA_CORE_USER: &str = "urn:ietf:params:scim:schemas:core:2.0:User";
pub const SCHEMA_ENTERPRISE_USER: &str =
	"urn:ietf:params:scim:schemas:extension:enterprise:2.0:User";
pub const SCHEMA_CORE_GROUP: &str = "urn:ietf:params:scim:schemas:core:2.0:Group";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Meta {
	pub resource_type: String,
	pub created: DateTime<Utc>,
	pub last_modified: DateTime<Utc>,
	pub location: Option<String>,
	pub version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Name {
	#[serde(skip_serializing_if = "Option::is_none")]
	pub formatted: Option<String>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub family_name: Option<String>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub given_name: Option<String>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub middle_name: Option<String>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub honorific_prefix: Option<String>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub honorific_suffix: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScimEmail {
	pub value: String,
	#[serde(rename = "type", skip_serializing_if = "Option::is_none")]
	pub email_type: Option<String>,
	#[serde(default)]
	pub primary: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScimPhoneNumber {
	pub value: String,
	#[serde(rename = "type", skip_serializing_if = "Option::is_none")]
	pub phone_type: Option<String>,
	#[serde(default)]
	pub primary: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScimUser {
	pub schemas: Vec<String>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub id: Option<String>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub external_id: Option<String>,
	pub user_name: String,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub name: Option<Name>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub display_name: Option<String>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub nick_name: Option<String>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub profile_url: Option<String>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub title: Option<String>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub user_type: Option<String>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub preferred_language: Option<String>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub locale: Option<String>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub timezone: Option<String>,
	#[serde(default = "default_active")]
	pub active: bool,
	#[serde(default, skip_serializing_if = "Vec::is_empty")]
	pub emails: Vec<ScimEmail>,
	#[serde(default, skip_serializing_if = "Vec::is_empty")]
	pub phone_numbers: Vec<ScimPhoneNumber>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub meta: Option<Meta>,
}

fn default_active() -> bool {
	true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupMember {
	pub value: String,
	#[serde(rename = "$ref", skip_serializing_if = "Option::is_none")]
	pub ref_: Option<String>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub display: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScimGroup {
	pub schemas: Vec<String>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub id: Option<String>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub external_id: Option<String>,
	pub display_name: String,
	#[serde(default, skip_serializing_if = "Vec::is_empty")]
	pub members: Vec<GroupMember>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub meta: Option<Meta>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListResponse<T> {
	pub schemas: Vec<String>,
	pub total_results: i64,
	pub items_per_page: i64,
	pub start_index: i64,
	#[serde(rename = "Resources")]
	pub resources: Vec<T>,
}

impl<T> ListResponse<T> {
	pub fn new(resources: Vec<T>, total_results: i64, start_index: i64, items_per_page: i64) -> Self {
		Self {
			schemas: vec!["urn:ietf:params:scim:api:messages:2.0:ListResponse".to_string()],
			total_results,
			items_per_page,
			start_index,
			resources,
		}
	}
}

pub trait ScimResource {
	fn id(&self) -> Option<&str>;
	fn external_id(&self) -> Option<&str>;
	fn meta(&self) -> Option<&Meta>;
}

impl ScimResource for ScimUser {
	fn id(&self) -> Option<&str> {
		self.id.as_deref()
	}
	fn external_id(&self) -> Option<&str> {
		self.external_id.as_deref()
	}
	fn meta(&self) -> Option<&Meta> {
		self.meta.as_ref()
	}
}

impl ScimResource for ScimGroup {
	fn id(&self) -> Option<&str> {
		self.id.as_deref()
	}
	fn external_id(&self) -> Option<&str> {
		self.external_id.as_deref()
	}
	fn meta(&self) -> Option<&Meta> {
		self.meta.as_ref()
	}
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthenticationScheme {
	#[serde(rename = "type")]
	pub scheme_type: String,
	pub name: String,
	pub description: String,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub spec_uri: Option<String>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub documentation_uri: Option<String>,
	pub primary: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Supported {
	pub supported: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BulkSupported {
	pub supported: bool,
	pub max_operations: i32,
	pub max_payload_size: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FilterSupported {
	pub supported: bool,
	pub max_results: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServiceProviderConfig {
	pub schemas: Vec<String>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub documentation_uri: Option<String>,
	pub patch: Supported,
	pub bulk: BulkSupported,
	pub filter: FilterSupported,
	pub change_password: Supported,
	pub sort: Supported,
	pub etag: Supported,
	pub authentication_schemes: Vec<AuthenticationScheme>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub meta: Option<Meta>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SchemaExtension {
	pub schema: String,
	pub required: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceType {
	pub schemas: Vec<String>,
	pub id: String,
	pub name: String,
	pub description: String,
	pub endpoint: String,
	pub schema: String,
	#[serde(default, skip_serializing_if = "Vec::is_empty")]
	pub schema_extensions: Vec<SchemaExtension>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub meta: Option<Meta>,
}
