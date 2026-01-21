// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights
// reserved. SPDX-License-Identifier: Proprietary

//! CSE cache extension for ThreadRepository.

use loom_server_db::CseRepository;
use loom_server_search_google_cse::CseResponse;

use crate::db::ThreadRepository;
use crate::error::ServerError;

/// Extension trait for CSE cache operations.
pub trait CseCacheExt {
	fn get_cse_cache(
		&self,
		query: &str,
		max_results: u32,
	) -> impl std::future::Future<Output = Result<Option<CseResponse>, ServerError>> + Send;

	fn put_cse_cache(
		&self,
		response: &CseResponse,
		max_results: u32,
	) -> impl std::future::Future<Output = Result<(), ServerError>> + Send;
}

impl CseCacheExt for ThreadRepository {
	async fn get_cse_cache(
		&self,
		query: &str,
		max_results: u32,
	) -> Result<Option<CseResponse>, ServerError> {
		let cse_repo = CseRepository::new(self.pool().clone());

		match cse_repo.get_cached_results(query, max_results).await? {
			Some(json) => {
				let response: CseResponse = serde_json::from_str(&json)?;
				Ok(Some(response))
			}
			None => Ok(None),
		}
	}

	async fn put_cse_cache(
		&self,
		response: &CseResponse,
		max_results: u32,
	) -> Result<(), ServerError> {
		let cse_repo = CseRepository::new(self.pool().clone());
		let json = serde_json::to_string(response)?;

		cse_repo
			.cache_results(&response.query, max_results, &json)
			.await?;
		cse_repo.cleanup_expired().await?;

		Ok(())
	}
}
