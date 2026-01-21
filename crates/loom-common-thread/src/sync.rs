// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights
// reserved. SPDX-License-Identifier: Proprietary

use std::sync::Arc;

use async_trait::async_trait;
use reqwest::StatusCode;
use tokio::sync::Mutex;
use tracing::{debug, error, info, warn};
use url::Url;

use crate::error::{ThreadStoreError, ThreadSyncError};
use crate::model::{Thread, ThreadId, ThreadSummary};
use crate::pending_sync::{PendingSyncStore, SyncOperation};
use crate::store::{LocalThreadStore, ThreadStore};

pub struct ThreadSyncClient {
	base_url: Url,
	http: reqwest::Client,
	retry_config: loom_common_http::RetryConfig,
	auth_token: Option<loom_common_secret::SecretString>,
}

impl ThreadSyncClient {
	pub fn new(base_url: Url, http: reqwest::Client) -> Self {
		Self {
			base_url,
			http,
			retry_config: loom_common_http::RetryConfig::default(),
			auth_token: None,
		}
	}

	pub fn with_auth_token(mut self, token: loom_common_secret::SecretString) -> Self {
		self.auth_token = Some(token);
		self
	}

	pub fn with_retry_config(mut self, config: loom_common_http::RetryConfig) -> Self {
		self.retry_config = config;
		self
	}

	fn apply_auth(&self, req: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
		if let Some(token) = &self.auth_token {
			req.header("Authorization", format!("Bearer {}", token.expose()))
		} else {
			req
		}
	}

	fn threads_url(&self) -> Result<Url, ThreadSyncError> {
		self
			.base_url
			.join("threads")
			.map_err(|e| ThreadSyncError::InvalidUrl(e.to_string()))
	}

	fn thread_url(&self, id: &ThreadId) -> Result<Url, ThreadSyncError> {
		self
			.base_url
			.join(&format!("threads/{id}"))
			.map_err(|e| ThreadSyncError::InvalidUrl(e.to_string()))
	}

	pub async fn upsert_thread(&self, thread: &Thread) -> Result<(), ThreadSyncError> {
		let url = self.thread_url(&thread.id)?;

		debug!(
				thread_id = %thread.id,
				version = thread.version,
				url = %url,
				"upserting thread to server"
		);

		let response = loom_common_http::retry(&self.retry_config, || async {
			let req = self.http.put(url.clone()).json(thread);
			self
				.apply_auth(req)
				.send()
				.await
				.map_err(ThreadSyncError::from)
		})
		.await?;

		match response.status() {
			StatusCode::OK | StatusCode::CREATED | StatusCode::NO_CONTENT => {
				info!(
						thread_id = %thread.id,
						version = thread.version,
						"thread synced to server"
				);
				Ok(())
			}
			StatusCode::CONFLICT => {
				warn!(
						thread_id = %thread.id,
						version = thread.version,
						"thread sync conflict"
				);
				Err(ThreadSyncError::Conflict)
			}
			status if status.is_server_error() => {
				let message = response.text().await.unwrap_or_default();
				Err(ThreadSyncError::Server { status, message })
			}
			status => Err(ThreadSyncError::UnexpectedStatus(status)),
		}
	}

	pub async fn get_thread(&self, id: &ThreadId) -> Result<Option<Thread>, ThreadSyncError> {
		let url = self.thread_url(id)?;

		debug!(thread_id = %id, url = %url, "fetching thread from server");

		let response = loom_common_http::retry(&self.retry_config, || async {
			let req = self.http.get(url.clone());
			self
				.apply_auth(req)
				.send()
				.await
				.map_err(ThreadSyncError::from)
		})
		.await?;

		match response.status() {
			StatusCode::OK => {
				let thread: Thread = response.json().await.map_err(ThreadSyncError::Network)?;
				debug!(
						thread_id = %id,
						version = thread.version,
						"fetched thread from server"
				);
				Ok(Some(thread))
			}
			StatusCode::NOT_FOUND => {
				debug!(thread_id = %id, "thread not found on server");
				Ok(None)
			}
			status if status.is_server_error() => {
				let message = response.text().await.unwrap_or_default();
				Err(ThreadSyncError::Server { status, message })
			}
			status => Err(ThreadSyncError::UnexpectedStatus(status)),
		}
	}

	pub async fn list_threads(&self, limit: u32) -> Result<Vec<ThreadSummary>, ThreadSyncError> {
		let mut url = self.threads_url()?;
		url
			.query_pairs_mut()
			.append_pair("limit", &limit.to_string());

		debug!(url = %url, limit = limit, "listing threads from server");

		let response = loom_common_http::retry(&self.retry_config, || async {
			let req = self.http.get(url.clone());
			self
				.apply_auth(req)
				.send()
				.await
				.map_err(ThreadSyncError::from)
		})
		.await?;

		match response.status() {
			StatusCode::OK => {
				let summaries: Vec<ThreadSummary> =
					response.json().await.map_err(ThreadSyncError::Network)?;
				debug!(count = summaries.len(), "listed threads from server");
				Ok(summaries)
			}
			status if status.is_server_error() => {
				let message = response.text().await.unwrap_or_default();
				Err(ThreadSyncError::Server { status, message })
			}
			status => Err(ThreadSyncError::UnexpectedStatus(status)),
		}
	}

	pub async fn delete_thread(&self, id: &ThreadId) -> Result<(), ThreadSyncError> {
		let url = self.thread_url(id)?;

		debug!(thread_id = %id, url = %url, "deleting thread from server");

		let response = loom_common_http::retry(&self.retry_config, || async {
			let req = self.http.delete(url.clone());
			self
				.apply_auth(req)
				.send()
				.await
				.map_err(ThreadSyncError::from)
		})
		.await?;

		match response.status() {
			StatusCode::OK | StatusCode::NO_CONTENT => {
				info!(thread_id = %id, "deleted thread from server");
				Ok(())
			}
			StatusCode::NOT_FOUND => {
				debug!(thread_id = %id, "thread not found on server for deletion");
				Ok(())
			}
			status if status.is_server_error() => {
				let message = response.text().await.unwrap_or_default();
				Err(ThreadSyncError::Server { status, message })
			}
			status => Err(ThreadSyncError::UnexpectedStatus(status)),
		}
	}
}

pub struct SyncingThreadStore {
	local: LocalThreadStore,
	sync_client: Option<ThreadSyncClient>,
	pending_store: Option<Arc<Mutex<PendingSyncStore>>>,
}

impl SyncingThreadStore {
	pub fn new(local: LocalThreadStore, sync_client: Option<ThreadSyncClient>) -> Self {
		Self {
			local,
			sync_client,
			pending_store: None,
		}
	}

	pub fn local_only(local: LocalThreadStore) -> Self {
		Self::new(local, None)
	}

	pub fn with_sync(local: LocalThreadStore, sync_client: ThreadSyncClient) -> Self {
		Self::new(local, Some(sync_client))
	}

	pub fn with_pending_store(mut self, pending_store: PendingSyncStore) -> Self {
		self.pending_store = Some(Arc::new(Mutex::new(pending_store)));
		self
	}

	pub async fn retry_pending(&self) -> Result<usize, ThreadStoreError> {
		let Some(sync_client) = &self.sync_client else {
			return Ok(0);
		};

		let Some(pending_store) = &self.pending_store else {
			return Ok(0);
		};

		let store = pending_store.lock().await;
		let queue = store.load().await?;
		drop(store);

		let mut success_count = 0;

		for entry in &queue.entries {
			match entry.operation {
				SyncOperation::Upsert => {
					if let Some(thread) = self.local.load(&entry.thread_id).await? {
						match sync_client.upsert_thread(&thread).await {
							Ok(()) => {
								let store = pending_store.lock().await;
								let _ = store
									.remove_pending(&entry.thread_id, &SyncOperation::Upsert)
									.await;
								success_count += 1;
								info!(
									thread_id = %entry.thread_id,
									"successfully retried pending upsert"
								);
							}
							Err(e) => {
								warn!(
									thread_id = %entry.thread_id,
									error = %e,
									"pending upsert retry failed"
								);
							}
						}
					}
				}
				SyncOperation::Delete => match sync_client.delete_thread(&entry.thread_id).await {
					Ok(()) => {
						let store = pending_store.lock().await;
						let _ = store
							.remove_pending(&entry.thread_id, &SyncOperation::Delete)
							.await;
						success_count += 1;
						info!(
							thread_id = %entry.thread_id,
							"successfully retried pending delete"
						);
					}
					Err(e) => {
						warn!(
							thread_id = %entry.thread_id,
							error = %e,
							"pending delete retry failed"
						);
					}
				},
			}
		}

		Ok(success_count)
	}

	pub async fn pending_count(&self) -> usize {
		let Some(pending_store) = &self.pending_store else {
			return 0;
		};

		let store = pending_store.lock().await;
		store.load().await.map(|q| q.len()).unwrap_or(0)
	}
}

#[async_trait]
impl ThreadStore for SyncingThreadStore {
	async fn load(&self, id: &ThreadId) -> Result<Option<Thread>, ThreadStoreError> {
		self.local.load(id).await
	}

	async fn save(&self, thread: &Thread) -> Result<(), ThreadStoreError> {
		self.local.save(thread).await?;

		if thread.is_private {
			debug!(
					thread_id = %thread.id,
					"skipping sync for private (local-only) thread"
			);
			return Ok(());
		}

		if let Some(sync_client) = &self.sync_client {
			let thread_clone = thread.clone();
			let sync_client_base_url = sync_client.base_url.clone();
			let http_clone = sync_client.http.clone();
			let retry_config = sync_client.retry_config.clone();
			let auth_token = sync_client.auth_token.clone();
			let pending_store = self.pending_store.clone();

			tokio::spawn(async move {
				let client = ThreadSyncClient {
					base_url: sync_client_base_url,
					http: http_clone,
					retry_config,
					auth_token,
				};

				match client.upsert_thread(&thread_clone).await {
					Ok(()) => {
						debug!(
								thread_id = %thread_clone.id,
								"background sync completed"
						);
						if let Some(store) = pending_store {
							let store = store.lock().await;
							let _ = store
								.remove_pending(&thread_clone.id, &SyncOperation::Upsert)
								.await;
						}
					}
					Err(e) => {
						error!(
								thread_id = %thread_clone.id,
								error = %e,
								"background sync failed"
						);
						if let Some(store) = pending_store {
							let store = store.lock().await;
							let _ = store
								.add_pending(
									thread_clone.id.clone(),
									SyncOperation::Upsert,
									Some(e.to_string()),
								)
								.await;
						}
					}
				}
			});
		}

		Ok(())
	}

	async fn save_and_sync(&self, thread: &Thread) -> Result<(), ThreadStoreError> {
		self.local.save(thread).await?;

		if thread.is_private {
			debug!(
					thread_id = %thread.id,
					"skipping sync for private (local-only) thread"
			);
			return Ok(());
		}

		if let Some(sync_client) = &self.sync_client {
			info!(
					thread_id = %thread.id,
					"syncing thread to server (blocking)"
			);

			match sync_client.upsert_thread(thread).await {
				Ok(()) => {
					info!(
							thread_id = %thread.id,
							"thread synced to server"
					);
				}
				Err(e) => {
					error!(
							thread_id = %thread.id,
							error = %e,
							"sync to server failed"
					);
					return Err(ThreadStoreError::Sync(e));
				}
			}
		} else {
			debug!(
					thread_id = %thread.id,
					"no sync client configured, skipping server sync"
			);
		}

		Ok(())
	}

	async fn list(&self, limit: u32) -> Result<Vec<ThreadSummary>, ThreadStoreError> {
		self.local.list(limit).await
	}

	async fn delete(&self, id: &ThreadId) -> Result<(), ThreadStoreError> {
		let thread = self.local.load(id).await?;

		self.local.delete(id).await?;

		if let Some(ref t) = thread {
			if t.is_private {
				debug!(
						thread_id = %t.id,
						"skipping delete sync for private (local-only) thread"
				);
				return Ok(());
			}
		}

		if let Some(sync_client) = &self.sync_client {
			let id_clone = id.clone();
			let sync_client_base_url = sync_client.base_url.clone();
			let http_clone = sync_client.http.clone();
			let retry_config = sync_client.retry_config.clone();
			let auth_token = sync_client.auth_token.clone();
			let pending_store = self.pending_store.clone();

			tokio::spawn(async move {
				let client = ThreadSyncClient {
					base_url: sync_client_base_url,
					http: http_clone,
					retry_config,
					auth_token,
				};

				match client.delete_thread(&id_clone).await {
					Ok(()) => {
						debug!(
								thread_id = %id_clone,
								"background delete sync completed"
						);
						if let Some(store) = pending_store {
							let store = store.lock().await;
							let _ = store
								.remove_pending(&id_clone, &SyncOperation::Delete)
								.await;
						}
					}
					Err(e) => {
						error!(
								thread_id = %id_clone,
								error = %e,
								"background delete sync failed"
						);
						if let Some(store) = pending_store {
							let store = store.lock().await;
							let _ = store
								.add_pending(id_clone.clone(), SyncOperation::Delete, Some(e.to_string()))
								.await;
						}
					}
				}
			});
		}

		Ok(())
	}
}
