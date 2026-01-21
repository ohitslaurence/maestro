// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights
// reserved. SPDX-License-Identifier: Proprietary

//! Shared pagination utilities for API handlers.

#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct PaginationParams {
	pub limit: Option<i32>,
	pub offset: Option<i32>,
}

impl PaginationParams {
	pub fn limit_clamped(&self, default: i32, max: i32) -> i32 {
		self.limit.unwrap_or(default).min(max).max(1)
	}

	pub fn offset_or_default(&self) -> i32 {
		self.offset.unwrap_or(0).max(0)
	}
}

#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct ScimPaginationParams {
	#[serde(rename = "startIndex")]
	pub start_index: Option<i32>,
	pub count: Option<i32>,
}

impl ScimPaginationParams {
	pub fn to_offset_limit(&self, default_count: i32, max_count: i32) -> (i32, i32) {
		let offset = self.start_index.unwrap_or(1).saturating_sub(1).max(0);
		let limit = self.count.unwrap_or(default_count).min(max_count).max(1);
		(offset, limit)
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_pagination_defaults() {
		let params = PaginationParams::default();
		assert_eq!(params.limit_clamped(20, 100), 20);
		assert_eq!(params.offset_or_default(), 0);
	}

	#[test]
	fn test_pagination_clamping() {
		let params = PaginationParams {
			limit: Some(500),
			offset: Some(-5),
		};
		assert_eq!(params.limit_clamped(20, 100), 100);
		assert_eq!(params.offset_or_default(), 0);

		let params = PaginationParams {
			limit: Some(0),
			offset: Some(10),
		};
		assert_eq!(params.limit_clamped(20, 100), 1);
		assert_eq!(params.offset_or_default(), 10);
	}

	#[test]
	fn test_scim_pagination() {
		let params = ScimPaginationParams::default();
		assert_eq!(params.to_offset_limit(10, 100), (0, 10));

		let params = ScimPaginationParams {
			start_index: Some(11),
			count: Some(25),
		};
		assert_eq!(params.to_offset_limit(10, 100), (10, 25));

		let params = ScimPaginationParams {
			start_index: Some(0),
			count: Some(200),
		};
		assert_eq!(params.to_offset_limit(10, 100), (0, 100));
	}
}
