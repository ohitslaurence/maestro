// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use crate::error::ScimError;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PatchOp {
	Add,
	Remove,
	Replace,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PatchOperation {
	pub op: PatchOp,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub path: Option<String>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub value: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PatchRequest {
	pub schemas: Vec<String>,
	#[serde(rename = "Operations")]
	pub operations: Vec<PatchOperation>,
}

impl PatchRequest {
	pub fn validate(&self) -> Result<(), ScimError> {
		if !self
			.schemas
			.contains(&"urn:ietf:params:scim:api:messages:2.0:PatchOp".to_string())
		{
			return Err(ScimError::InvalidSyntax(
				"Missing PatchOp schema".to_string(),
			));
		}
		for op in &self.operations {
			if op.op == PatchOp::Remove && op.path.is_none() {
				return Err(ScimError::InvalidPath("Remove requires path".to_string()));
			}
		}
		Ok(())
	}
}
