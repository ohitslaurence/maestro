// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use serde::{Deserialize, Serialize};

/// GitHub App installation info stored in the database.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GithubInstallation {
	pub installation_id: i64,
	pub account_id: i64,
	pub account_login: String,
	pub account_type: String,
	pub app_slug: Option<String>,
	pub repositories_selection: String,
	pub suspended_at: Option<String>,
	pub created_at: String,
	pub updated_at: String,
}

/// GitHub repository linked to an installation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GithubRepo {
	pub repository_id: i64,
	pub owner: String,
	pub name: String,
	pub full_name: String,
	pub private: bool,
	pub default_branch: Option<String>,
}

/// Installation info with minimal fields for lookups.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GithubInstallationInfo {
	pub installation_id: i64,
	pub account_login: String,
	pub account_type: String,
	pub repositories_selection: String,
}
