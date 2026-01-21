// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

#[derive(Debug, Deserialize, IntoParams)]
pub struct GithubInstallationByRepoQuery {
	pub owner: String,
	pub repo: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct GithubSearchCodeRequest {
	pub owner: String,
	pub repo: String,
	pub query: String,
	#[serde(default = "default_github_per_page")]
	pub per_page: u32,
	#[serde(default = "default_github_page")]
	pub page: u32,
}

fn default_github_per_page() -> u32 {
	30
}

fn default_github_page() -> u32 {
	1
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct GithubRepoInfoRequest {
	pub owner: String,
	pub repo: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct GithubFileContentsRequest {
	pub owner: String,
	pub repo: String,
	pub path: String,
	#[serde(rename = "ref")]
	pub git_ref: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct GithubRepoInfoResponse {
	pub id: i64,
	pub full_name: String,
	pub description: Option<String>,
	pub private: bool,
	pub default_branch: String,
	pub language: Option<String>,
	pub stargazers_count: u32,
	pub html_url: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct GithubFileContentsResponse {
	pub name: String,
	pub path: String,
	pub sha: String,
	pub size: u64,
	pub encoding: String,
	pub content: String,
}
