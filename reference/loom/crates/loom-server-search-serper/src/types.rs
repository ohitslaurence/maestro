// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights
// reserved. SPDX-License-Identifier: Proprietary

//! Types for Serper.dev API.

use serde::{Deserialize, Serialize};

/// Request parameters for a Serper search.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone)]
pub struct SerperRequest {
	pub query: String,
	pub num: u32,
}

impl SerperRequest {
	/// Creates a new Serper request with the given query and result count.
	/// The `num` parameter is clamped to the valid range of 1-100.
	pub fn new(query: impl Into<String>, num: u32) -> Self {
		Self {
			query: query.into(),
			num: num.clamp(1, 100),
		}
	}
}

/// Response from a Serper search.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerperResponse {
	pub query: String,
	pub results: Vec<SerperResultItem>,
}

/// A single search result item.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerperResultItem {
	pub title: String,
	pub url: String,
	pub snippet: String,
	pub position: u32,
}

#[cfg(test)]
mod tests {
	use super::*;
	use proptest::prelude::*;

	proptest! {
		/// Property: num is always clamped to 1-100 range.
		/// This ensures we never send invalid values to the Serper API,
		/// which only accepts 1-100 results per request.
		#[test]
		fn serper_request_num_is_clamped(num in 0u32..200) {
			let request = SerperRequest::new("test", num);
			prop_assert!(request.num >= 1 && request.num <= 100);
		}

		/// Property: query is preserved exactly as provided.
		/// This ensures we don't accidentally modify user search terms.
		#[test]
		fn serper_request_preserves_query(query in "\\PC*") {
			let request = SerperRequest::new(query.clone(), 10);
			prop_assert_eq!(request.query, query);
		}
	}

	#[test]
	fn test_serper_request_clamps_zero_to_one() {
		let request = SerperRequest::new("test", 0);
		assert_eq!(request.num, 1);
	}

	#[test]
	fn test_serper_request_clamps_large_to_hundred() {
		let request = SerperRequest::new("test", 150);
		assert_eq!(request.num, 100);
	}

	#[test]
	fn test_serper_request_valid_range_unchanged() {
		for num in [1, 10, 50, 100] {
			let request = SerperRequest::new("test", num);
			assert_eq!(request.num, num);
		}
	}
}
