// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! Secrets client implementation.

use chrono::{DateTime, Utc};
use loom_common_secret::SecretString;
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, instrument, warn};

use crate::error::{SecretsClientError, SecretsClientResult};
use crate::SecretScope;

/// Default paths for K8s service account credentials.
const SA_TOKEN_PATH: &str = "/var/run/secrets/kubernetes.io/serviceaccount/token";
const SA_NAMESPACE_PATH: &str = "/var/run/secrets/kubernetes.io/serviceaccount/namespace";

/// Default server URL for in-cluster access.
const DEFAULT_SERVER_URL: &str = "http://loom-server.loom.svc.cluster.local:8080";

/// Buffer time before SVID expiry to trigger refresh (60 seconds).
const SVID_REFRESH_BUFFER_SECS: i64 = 60;

/// Configuration for the secrets client.
pub struct ClientConfig {
	/// Server URL for the secrets service.
	pub server_url: String,
	/// Whether to allow insecure HTTP connections (for in-cluster use only).
	/// Default: false (require HTTPS)
	pub allow_insecure: bool,
}

/// Cached SVID with expiry.
struct CachedSvid {
	token: SecretString,
	expires_at: DateTime<Utc>,
}

/// Request to obtain SVID.
#[derive(Debug, Serialize)]
struct SvidRequest {
	pod_name: String,
	pod_namespace: String,
}

/// Response from SVID endpoint.
#[derive(Debug, Deserialize)]
struct SvidResponse {
	token: String,
	expires_at: DateTime<Utc>,
	#[allow(dead_code)]
	spiffe_id: String,
}

/// Response from secrets endpoint.
#[derive(Debug, Deserialize)]
struct SecretResponse {
	#[allow(dead_code)]
	name: String,
	#[allow(dead_code)]
	scope: String,
	#[allow(dead_code)]
	version: i32,
	value: String,
}

/// Error response from server.
#[derive(Debug, Deserialize)]
struct ErrorResponse {
	error: String,
	message: Option<String>,
}

/// Client for accessing secrets from weavers.
pub struct SecretsClient {
	http_client: reqwest::Client,
	server_url: String,
	svid: Arc<RwLock<Option<CachedSvid>>>,
}

impl SecretsClient {
	/// Create a new secrets client with default configuration.
	///
	/// Uses environment variables and K8s defaults:
	/// - `LOOM_SECRETS_SERVER_URL` or default in-cluster URL
	/// - `LOOM_SECRETS_ALLOW_INSECURE=1` or `true` to allow HTTP (for in-cluster use)
	pub fn new() -> SecretsClientResult<Self> {
		let server_url =
			std::env::var("LOOM_SECRETS_SERVER_URL").unwrap_or_else(|_| DEFAULT_SERVER_URL.to_string());

		let allow_insecure = std::env::var("LOOM_SECRETS_ALLOW_INSECURE")
			.map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
			.unwrap_or(false);

		Self::with_config(ClientConfig {
			server_url,
			allow_insecure,
		})
	}

	/// Create a client with a specific configuration.
	pub fn with_config(config: ClientConfig) -> SecretsClientResult<Self> {
		if !config.allow_insecure && !config.server_url.starts_with("https://") {
			return Err(SecretsClientError::Configuration(
				"server URL must use HTTPS (set allow_insecure=true for in-cluster HTTP)".into(),
			));
		}

		let http_client = reqwest::Client::builder()
			.timeout(std::time::Duration::from_secs(30))
			.redirect(reqwest::redirect::Policy::none())
			.build()
			.map_err(|e| {
				SecretsClientError::Configuration(format!("failed to create HTTP client: {e}"))
			})?;

		Ok(Self {
			http_client,
			server_url: config.server_url,
			svid: Arc::new(RwLock::new(None)),
		})
	}

	/// Create a client with a specific server URL.
	///
	/// # Security
	///
	/// This method requires HTTPS by default. Use `with_config` with
	/// `allow_insecure: true` for in-cluster HTTP connections.
	pub fn with_server_url(server_url: String) -> SecretsClientResult<Self> {
		Self::with_config(ClientConfig {
			server_url,
			allow_insecure: false,
		})
	}

	/// Get a secret by name and scope.
	///
	/// # Security
	///
	/// - Automatically obtains/refreshes SVID as needed
	/// - Returns a `SecretString` that auto-redacts in logs
	#[instrument(skip(self), fields(scope = ?scope, name = %name))]
	pub async fn get_secret(
		&self,
		scope: SecretScope,
		name: &str,
	) -> SecretsClientResult<SecretString> {
		let svid = self.ensure_svid().await?;

		let url = format!(
			"{}/internal/weaver-secrets/v1/secrets/{}/{}",
			self.server_url,
			scope.path_segment(),
			name
		);

		debug!(url = %url, "Fetching secret");

		let response = self
			.http_client
			.get(&url)
			.header(AUTHORIZATION, format!("Bearer {}", svid.expose()))
			.send()
			.await?;

		let status = response.status();

		if status.is_success() {
			let body: SecretResponse = response
				.json()
				.await
				.map_err(|e| SecretsClientError::InvalidResponse(e.to_string()))?;

			Ok(SecretString::new(body.value))
		} else if status == reqwest::StatusCode::NOT_FOUND {
			Err(SecretsClientError::SecretNotFound(name.to_string()))
		} else if status == reqwest::StatusCode::FORBIDDEN {
			let body: ErrorResponse = response.json().await.unwrap_or(ErrorResponse {
				error: "access denied".to_string(),
				message: None,
			});
			Err(SecretsClientError::AccessDenied(
				body.message.unwrap_or(body.error),
			))
		} else if status == reqwest::StatusCode::UNAUTHORIZED {
			// SVID might be invalid/expired, clear cache and retry once
			{
				let mut svid_lock = self.svid.write().await;
				*svid_lock = None;
			}
			Err(SecretsClientError::SvidExpired)
		} else {
			let body = response.text().await.unwrap_or_default();
			let safe_body = sanitize_body_for_error(&body, 200);
			Err(SecretsClientError::SecretFetch(format!(
				"HTTP {}: {}",
				status, safe_body
			)))
		}
	}

	/// Get a secret with automatic retry on SVID expiry.
	pub async fn get_secret_with_retry(
		&self,
		scope: SecretScope,
		name: &str,
	) -> SecretsClientResult<SecretString> {
		match self.get_secret(scope, name).await {
			Ok(secret) => Ok(secret),
			Err(SecretsClientError::SvidExpired) => {
				// Retry once after clearing cache
				self.get_secret(scope, name).await
			}
			Err(e) => Err(e),
		}
	}

	/// Get a valid SVID token, obtaining one if needed.
	///
	/// This is useful for other services that need to authenticate with SVID,
	/// such as the WireGuard tunnel registration.
	pub async fn get_svid(&self) -> SecretsClientResult<SecretString> {
		self.ensure_svid().await
	}

	/// Ensure we have a valid SVID, obtaining one if needed.
	async fn ensure_svid(&self) -> SecretsClientResult<SecretString> {
		// Check cache
		{
			let cached = self.svid.read().await;
			if let Some(ref svid) = *cached {
				let now = Utc::now();
				let expires_with_buffer =
					svid.expires_at - chrono::Duration::seconds(SVID_REFRESH_BUFFER_SECS);
				if now < expires_with_buffer {
					return Ok(SecretString::new(svid.token.expose().to_string()));
				}
			}
		}

		// Need to obtain new SVID
		let new_svid = self.obtain_svid().await?;
		let token = SecretString::new(new_svid.token.expose().to_string());

		// Update cache
		{
			let mut cached = self.svid.write().await;
			*cached = Some(new_svid);
		}

		Ok(token)
	}

	/// Obtain a new SVID from the server.
	#[instrument(skip(self))]
	async fn obtain_svid(&self) -> SecretsClientResult<CachedSvid> {
		// Read K8s service account token
		let sa_token = SecretString::new(
			tokio::fs::read_to_string(SA_TOKEN_PATH)
				.await
				.map_err(|e| {
					SecretsClientError::ServiceAccountToken(format!(
						"failed to read {}: {}",
						SA_TOKEN_PATH, e
					))
				})?
				.trim()
				.to_string(),
		);

		// Read namespace
		let namespace = tokio::fs::read_to_string(SA_NAMESPACE_PATH)
			.await
			.map_err(|e| {
				SecretsClientError::ServiceAccountToken(format!(
					"failed to read {}: {}",
					SA_NAMESPACE_PATH, e
				))
			})?
			.trim()
			.to_string();

		// Get pod name from hostname
		let pod_name = std::env::var("HOSTNAME")
			.or_else(|_| hostname::get().map(|h| h.to_string_lossy().to_string()))
			.map_err(|e| SecretsClientError::Configuration(format!("failed to get hostname: {e}")))?;

		let request = SvidRequest {
			pod_name: pod_name.clone(),
			pod_namespace: namespace.clone(),
		};

		debug!(pod_name = %pod_name, namespace = %namespace, "Obtaining SVID");

		let url = format!("{}/internal/weaver-auth/token", self.server_url);

		let response = self
			.http_client
			.post(&url)
			.header(AUTHORIZATION, format!("Bearer {}", sa_token.expose()))
			.header(CONTENT_TYPE, "application/json")
			.json(&request)
			.send()
			.await?;

		let status = response.status();

		if status.is_success() {
			let body: SvidResponse = response
				.json()
				.await
				.map_err(|e| SecretsClientError::InvalidResponse(e.to_string()))?;

			debug!(spiffe_id = %body.spiffe_id, expires_at = %body.expires_at, "Obtained SVID");

			Ok(CachedSvid {
				token: SecretString::new(body.token),
				expires_at: body.expires_at,
			})
		} else {
			let body = response.text().await.unwrap_or_default();
			let safe_body = sanitize_body_for_error(&body, 200);
			warn!(status = %status, "Failed to obtain SVID");
			Err(SecretsClientError::SvidIssuance(format!(
				"HTTP {}: {}",
				status, safe_body
			)))
		}
	}
}

impl std::fmt::Debug for SecretsClient {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.debug_struct("SecretsClient")
			.field("server_url", &self.server_url)
			.field(
				"has_cached_svid",
				&self.svid.try_read().map(|s| s.is_some()).unwrap_or(false),
			)
			.finish()
	}
}

fn sanitize_body_for_error(body: &str, max_len: usize) -> String {
	let sanitized: String = body
		.chars()
		.filter(|c| !c.is_control() || *c == ' ')
		.take(max_len)
		.collect();
	if body.len() > max_len {
		format!("{sanitized}...")
	} else {
		sanitized
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn scope_path_segments() {
		assert_eq!(SecretScope::Org.path_segment(), "org");
		assert_eq!(SecretScope::Repo.path_segment(), "repo");
		assert_eq!(SecretScope::Weaver.path_segment(), "weaver");
	}

	#[test]
	fn client_debug_does_not_leak_token() {
		let client = SecretsClient::with_config(ClientConfig {
			server_url: "http://localhost:8080".to_string(),
			allow_insecure: true,
		})
		.unwrap();
		let debug = format!("{:?}", client);
		assert!(!debug.contains("Bearer"));
		assert!(!debug.contains("token"));
	}

	#[test]
	fn rejects_http_without_allow_insecure() {
		let result = SecretsClient::with_server_url("http://localhost:8080".to_string());
		assert!(result.is_err());
		let err = result.unwrap_err();
		assert!(matches!(err, SecretsClientError::Configuration(_)));
	}

	#[test]
	fn allows_https_by_default() {
		let result = SecretsClient::with_server_url("https://localhost:8080".to_string());
		assert!(result.is_ok());
	}

	#[test]
	fn allows_http_with_insecure_flag() {
		let result = SecretsClient::with_config(ClientConfig {
			server_url: "http://localhost:8080".to_string(),
			allow_insecure: true,
		});
		assert!(result.is_ok());
	}
}
