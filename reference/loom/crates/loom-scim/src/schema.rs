// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use crate::types::Meta;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SchemaAttribute {
	pub name: String,
	#[serde(rename = "type")]
	pub attr_type: String,
	pub multi_valued: bool,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub description: Option<String>,
	pub required: bool,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub canonical_values: Option<Vec<String>>,
	pub case_exact: bool,
	pub mutability: String,
	pub returned: String,
	pub uniqueness: String,
	#[serde(default, skip_serializing_if = "Vec::is_empty")]
	pub sub_attributes: Vec<SchemaAttribute>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Schema {
	pub schemas: Vec<String>,
	pub id: String,
	pub name: String,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub description: Option<String>,
	pub attributes: Vec<SchemaAttribute>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub meta: Option<Meta>,
}
