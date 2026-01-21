// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use chrono::{DateTime, Utc};
use loom_server_db::BranchProtectionRuleRecord;
use loom_server_scm::BranchProtectionRule;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateProtectionRuleRequest {
	pub pattern: String,
	#[serde(default = "default_true")]
	pub block_direct_push: bool,
	#[serde(default = "default_true")]
	pub block_force_push: bool,
	#[serde(default = "default_true")]
	pub block_deletion: bool,
}

fn default_true() -> bool {
	true
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ProtectionRuleResponse {
	pub id: Uuid,
	pub repo_id: Uuid,
	pub pattern: String,
	pub block_direct_push: bool,
	pub block_force_push: bool,
	pub block_deletion: bool,
	pub created_at: DateTime<Utc>,
}

impl From<BranchProtectionRule> for ProtectionRuleResponse {
	fn from(rule: BranchProtectionRule) -> Self {
		Self {
			id: rule.id,
			repo_id: rule.repo_id,
			pattern: rule.pattern,
			block_direct_push: rule.block_direct_push,
			block_force_push: rule.block_force_push,
			block_deletion: rule.block_deletion,
			created_at: rule.created_at,
		}
	}
}

impl From<BranchProtectionRuleRecord> for ProtectionRuleResponse {
	fn from(rule: BranchProtectionRuleRecord) -> Self {
		Self {
			id: rule.id,
			repo_id: rule.repo_id,
			pattern: rule.pattern,
			block_direct_push: rule.block_direct_push,
			block_force_push: rule.block_force_push,
			block_deletion: rule.block_deletion,
			created_at: rule.created_at,
		}
	}
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ListProtectionRulesResponse {
	pub rules: Vec<ProtectionRuleResponse>,
}
