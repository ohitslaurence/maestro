// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Debug, Deserialize, ToSchema)]
pub struct CseProxyRequest {
	pub query: String,
	pub max_results: Option<u32>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct CseProxyResponse {
	pub query: String,
	pub results: Vec<CseProxyResultItem>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct CseProxyResultItem {
	pub title: String,
	pub url: String,
	pub snippet: String,
	pub display_link: Option<String>,
	pub rank: u32,
}
