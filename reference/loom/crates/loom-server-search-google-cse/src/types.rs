// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights
// reserved. SPDX-License-Identifier: Proprietary

//! Types for Google Custom Search Engine API.

use serde::{Deserialize, Serialize};

/// Request parameters for a CSE search.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone)]
pub struct CseRequest {
	pub query: String,
	pub num: u32,
}

impl CseRequest {
	/// Creates a new CSE request with the given query and result count.
	/// The `num` parameter is clamped to the valid range of 1-10.
	pub fn new(query: impl Into<String>, num: u32) -> Self {
		Self {
			query: query.into(),
			num: num.clamp(1, 10),
		}
	}
}

/// Response from a CSE search.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CseResponse {
	pub query: String,
	pub results: Vec<CseResultItem>,
}

/// A single search result item.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CseResultItem {
	pub title: String,
	pub url: String,
	pub snippet: String,
	pub display_link: Option<String>,
	pub rank: u32,
}

#[cfg(test)]
mod tests {
	use super::*;
	use proptest::prelude::*;

	proptest! {
			/// Property: num is always clamped to 1-10 range.
			/// This ensures we never send invalid values to the Google API,
			/// which only accepts 1-10 results per request.
			#[test]
			fn cse_request_num_is_clamped(num in 0u32..100) {
					let request = CseRequest::new("test", num);
					prop_assert!(request.num >= 1 && request.num <= 10);
			}

			/// Property: query is preserved exactly as provided.
			/// This ensures we don't accidentally modify user search terms.
			#[test]
			fn cse_request_preserves_query(query in "\\PC*") {
					let request = CseRequest::new(query.clone(), 5);
					prop_assert_eq!(request.query, query);
			}
	}

	#[test]
	fn test_cse_request_clamps_zero_to_one() {
		let request = CseRequest::new("test", 0);
		assert_eq!(request.num, 1);
	}

	#[test]
	fn test_cse_request_clamps_large_to_ten() {
		let request = CseRequest::new("test", 100);
		assert_eq!(request.num, 10);
	}

	#[test]
	fn test_cse_request_valid_range_unchanged() {
		for num in 1..=10 {
			let request = CseRequest::new("test", num);
			assert_eq!(request.num, num);
		}
	}
}
