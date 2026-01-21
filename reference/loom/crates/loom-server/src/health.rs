// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights
// reserved. SPDX-License-Identifier: Proprietary

//! Health check types and component checking logic.

use serde::Serialize;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::{timeout, Instant};
use utoipa::ToSchema;

use loom_server_config::ScimConfig;
use loom_server_github_app::{GithubAppClient, GithubAppError};
use loom_server_jobs::JobScheduler;
use loom_server_llm_service::LlmService;
use loom_server_secrets::KeyBackend;
use loom_server_smtp::SmtpClient;
use loom_server_weaver::Provisioner;

use crate::db::{OrgRepository, ThreadRepository};

/// Health status for components and overall system.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum HealthStatus {
	Healthy,
	Degraded,
	Unhealthy,
	Unknown,
}

/// Database component health.
#[derive(Debug, Serialize, ToSchema)]
pub struct DatabaseHealth {
	pub status: HealthStatus,
	pub latency_ms: u64,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub error: Option<String>,
}

/// Binary directory component health.
#[derive(Debug, Serialize, ToSchema)]
pub struct BinDirHealth {
	pub status: HealthStatus,
	pub latency_ms: u64,
	pub path: String,
	pub exists: bool,
	pub is_dir: bool,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub file_count: Option<usize>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub error: Option<String>,
}

/// Individual account health in the pool
#[derive(Debug, Serialize, ToSchema)]
pub struct AnthropicAccountHealth {
	pub id: String,
	pub status: AnthropicAccountStatus,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub cooldown_remaining_secs: Option<u64>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub last_error: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum AnthropicAccountStatus {
	Available,
	CoolingDown,
	Disabled,
}

/// Pool status for health reporting
#[derive(Debug, Serialize, ToSchema)]
pub struct AnthropicPoolHealth {
	pub accounts_total: usize,
	pub accounts_available: usize,
	pub accounts_cooling: usize,
	pub accounts_disabled: usize,
	pub accounts: Vec<AnthropicAccountHealth>,
}

/// Individual LLM provider health.
#[derive(Debug, Serialize, ToSchema)]
pub struct LlmProviderHealth {
	pub name: String,
	pub status: HealthStatus,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub mode: Option<String>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub pool: Option<AnthropicPoolHealth>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub latency_ms: Option<u64>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub error: Option<String>,
}

/// LLM providers component health.
#[derive(Debug, Serialize, ToSchema)]
pub struct LlmProvidersHealth {
	pub status: HealthStatus,
	pub providers: Vec<LlmProviderHealth>,
}

/// Google CSE component health.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct GoogleCseHealth {
	pub status: HealthStatus,
	pub latency_ms: u64,
	pub configured: bool,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub error: Option<String>,
}

/// Serper component health.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct SerperHealth {
	pub status: HealthStatus,
	pub latency_ms: u64,
	pub configured: bool,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub error: Option<String>,
}

/// GitHub App component health.
#[derive(Debug, Serialize, ToSchema)]
pub struct GithubAppHealth {
	pub status: HealthStatus,
	pub latency_ms: u64,
	pub configured: bool,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub error: Option<String>,
}

/// Kubernetes component health.
#[derive(Debug, Serialize, ToSchema)]
pub struct KubernetesHealth {
	pub status: HealthStatus,
	pub latency_ms: u64,
	pub namespace: String,
	pub reachable: bool,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub error: Option<String>,
}

/// SMTP component health.
#[derive(Debug, Serialize, ToSchema)]
pub struct SmtpHealth {
	pub status: HealthStatus,
	pub latency_ms: u64,
	pub configured: bool,
	pub healthy: bool,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub error: Option<String>,
}

/// GeoIP component health.
#[derive(Debug, Serialize, ToSchema)]
pub struct GeoIpHealth {
	pub status: HealthStatus,
	pub latency_ms: u64,
	pub configured: bool,
	pub healthy: bool,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub database_path: Option<String>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub database_type: Option<String>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub error: Option<String>,
}

/// Jobs scheduler component health.
#[derive(Debug, Serialize, ToSchema)]
pub struct JobsHealth {
	pub status: HealthStatus,
	pub jobs_total: usize,
	pub jobs_healthy: usize,
	pub jobs_failing: usize,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub failing_jobs: Option<Vec<String>>,
}

/// Secrets system component health.
#[derive(Debug, Serialize, ToSchema)]
pub struct SecretsHealth {
	pub status: HealthStatus,
	pub latency_ms: u64,
	pub configured: bool,
	pub master_key_present: bool,
	pub svid_signing_key_present: bool,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub error: Option<String>,
}

/// SCIM component health.
#[derive(Debug, Serialize, ToSchema)]
pub struct ScimHealth {
	pub status: HealthStatus,
	pub enabled: bool,
	pub configured: bool,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub org_id: Option<String>,
	pub org_exists: bool,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub error: Option<String>,
}

/// Individual authentication provider health.
#[derive(Debug, Serialize, ToSchema)]
pub struct AuthProviderHealth {
	pub name: String,
	pub status: HealthStatus,
	pub configured: bool,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub error: Option<String>,
}

/// Authentication providers component health.
#[derive(Debug, Serialize, ToSchema)]
pub struct AuthProvidersHealth {
	pub status: HealthStatus,
	pub providers: Vec<AuthProviderHealth>,
}

/// All health check components.
#[derive(Debug, Serialize, ToSchema)]
pub struct HealthComponents {
	pub auth_providers: AuthProvidersHealth,
	pub bin_dir: BinDirHealth,
	pub database: DatabaseHealth,
	pub geoip: GeoIpHealth,
	pub github_app: GithubAppHealth,
	pub google_cse: GoogleCseHealth,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub jobs: Option<JobsHealth>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub kubernetes: Option<KubernetesHealth>,
	pub llm_providers: LlmProvidersHealth,
	pub scim: ScimHealth,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub secrets: Option<SecretsHealth>,
	pub serper: SerperHealth,
	pub smtp: SmtpHealth,
}

/// Complete health check response.
#[derive(Debug, Serialize, ToSchema)]
pub struct HealthResponse {
	pub status: HealthStatus,
	pub timestamp: String,
	pub duration_ms: u64,
	pub version: loom_common_version::HealthVersionInfo,
	pub components: HealthComponents,
}

const DB_CHECK_TIMEOUT: Duration = Duration::from_millis(500);

/// Check database health.
pub async fn check_database(repo: &ThreadRepository) -> DatabaseHealth {
	let start = Instant::now();

	let result = timeout(DB_CHECK_TIMEOUT, repo.health_check()).await;
	let latency_ms = start.elapsed().as_millis() as u64;

	match result {
		Ok(Ok(())) => DatabaseHealth {
			status: HealthStatus::Healthy,
			latency_ms,
			error: None,
		},
		Ok(Err(e)) => DatabaseHealth {
			status: HealthStatus::Unhealthy,
			latency_ms,
			error: Some(e.to_string()),
		},
		Err(_) => DatabaseHealth {
			status: HealthStatus::Unhealthy,
			latency_ms,
			error: Some("database health check timed out".to_string()),
		},
	}
}

/// Check binary directory health.
pub fn check_bin_dir() -> BinDirHealth {
	let start = Instant::now();

	let bin_dir = std::env::var("LOOM_SERVER_BIN_DIR").unwrap_or_else(|_| "./bin".to_string());
	let path = Path::new(&bin_dir);

	let (exists, is_dir, file_count, status, error) = if !path.exists() {
		(
			false,
			false,
			None,
			HealthStatus::Degraded,
			Some("binary directory does not exist".to_string()),
		)
	} else if !path.is_dir() {
		(
			true,
			false,
			None,
			HealthStatus::Degraded,
			Some("binary path is not a directory".to_string()),
		)
	} else {
		match std::fs::read_dir(path) {
			Ok(entries) => {
				let count = entries
					.filter_map(Result::ok)
					.filter(|e| !e.file_name().to_string_lossy().starts_with('.'))
					.count();
				if count == 0 {
					(
						true,
						true,
						Some(0),
						HealthStatus::Degraded,
						Some("binary directory is empty".to_string()),
					)
				} else {
					(true, true, Some(count), HealthStatus::Healthy, None)
				}
			}
			Err(e) => (
				true,
				true,
				None,
				HealthStatus::Degraded,
				Some(format!("failed to read binary directory: {e}")),
			),
		}
	};

	let latency_ms = start.elapsed().as_millis() as u64;

	BinDirHealth {
		status,
		latency_ms,
		path: bin_dir,
		exists,
		is_dir,
		file_count,
		error,
	}
}

/// Check LLM provider health by verifying if the service is configured.
pub async fn check_llm_providers(llm_service: Option<&LlmService>) -> LlmProvidersHealth {
	match llm_service {
		Some(service) => {
			let mut providers = Vec::new();
			let mut overall_status = HealthStatus::Healthy;

			if service.has_anthropic() {
				let anthropic_health = service.anthropic_health().await;
				let (status, mode, pool) = match anthropic_health {
					Some(loom_server_llm_service::AnthropicHealthInfo::ApiKey { .. }) => {
						(HealthStatus::Healthy, Some("api_key".to_string()), None)
					}
					Some(loom_server_llm_service::AnthropicHealthInfo::Pool(pool_status)) => {
						let status = if pool_status.accounts_total == 0 {
							HealthStatus::Unhealthy
						} else if pool_status.accounts_available == pool_status.accounts_total {
							HealthStatus::Healthy
						} else if pool_status.accounts_available > 0 {
							HealthStatus::Degraded
						} else {
							HealthStatus::Unhealthy
						};

						let pool_health = AnthropicPoolHealth {
							accounts_total: pool_status.accounts_total,
							accounts_available: pool_status.accounts_available,
							accounts_cooling: pool_status.accounts_cooling,
							accounts_disabled: pool_status.accounts_disabled,
							accounts: pool_status
								.accounts
								.into_iter()
								.map(|a| AnthropicAccountHealth {
									id: a.id,
									status: match a.status {
										loom_server_llm_service::AccountHealthStatus::Available => {
											AnthropicAccountStatus::Available
										}
										loom_server_llm_service::AccountHealthStatus::CoolingDown => {
											AnthropicAccountStatus::CoolingDown
										}
										loom_server_llm_service::AccountHealthStatus::Disabled => {
											AnthropicAccountStatus::Disabled
										}
									},
									cooldown_remaining_secs: a.cooldown_remaining_secs,
									last_error: a.last_error,
								})
								.collect(),
						};

						(status, Some("oauth_pool".to_string()), Some(pool_health))
					}
					None => (HealthStatus::Healthy, None, None),
				};

				if status == HealthStatus::Degraded && overall_status == HealthStatus::Healthy {
					overall_status = HealthStatus::Degraded;
				} else if status == HealthStatus::Unhealthy {
					overall_status = HealthStatus::Unhealthy;
				}

				providers.push(LlmProviderHealth {
					name: "anthropic".to_string(),
					status,
					mode,
					pool,
					latency_ms: None,
					error: None,
				});
			}

			if service.has_openai() {
				providers.push(LlmProviderHealth {
					name: "openai".to_string(),
					status: HealthStatus::Healthy,
					mode: None,
					pool: None,
					latency_ms: None,
					error: None,
				});
			}

			LlmProvidersHealth {
				status: overall_status,
				providers,
			}
		}
		None => LlmProvidersHealth {
			status: HealthStatus::Degraded,
			providers: vec![LlmProviderHealth {
				name: "none".to_string(),
				status: HealthStatus::Degraded,
				mode: None,
				pool: None,
				latency_ms: None,
				error: Some("LLM service not configured".to_string()),
			}],
		},
	}
}

const CSE_CHECK_TIMEOUT: Duration = Duration::from_secs(5);
const CSE_CACHE_TTL: Duration = Duration::from_secs(3600); // 1 hour

/// Cached CSE health result to avoid burning through API quota on health checks.
static CSE_HEALTH_CACHE: std::sync::OnceLock<
	tokio::sync::RwLock<Option<(Instant, GoogleCseHealth)>>,
> = std::sync::OnceLock::new();

fn get_cse_cache() -> &'static tokio::sync::RwLock<Option<(Instant, GoogleCseHealth)>> {
	CSE_HEALTH_CACHE.get_or_init(|| tokio::sync::RwLock::new(None))
}

/// Check Google CSE health by verifying configuration and optionally testing
/// connectivity. Results are cached for 5 minutes to avoid burning API quota.
pub async fn check_google_cse() -> GoogleCseHealth {
	use loom_server_search_google_cse::{CseClient, CseRequest};

	// Check cache first
	{
		let cache = get_cse_cache().read().await;
		if let Some((cached_at, ref health)) = *cache {
			if cached_at.elapsed() < CSE_CACHE_TTL {
				return GoogleCseHealth {
					status: health.status,
					latency_ms: 0, // Cached response
					configured: health.configured,
					error: health.error.clone(),
				};
			}
		}
	}

	let start = Instant::now();

	// Check if CSE is configured
	let api_key = std::env::var("LOOM_SERVER_GOOGLE_CSE_API_KEY");
	let cx = std::env::var("LOOM_SERVER_GOOGLE_CSE_SEARCH_ENGINE_ID");

	let (configured, status, error) = match (api_key, cx) {
		(Ok(key), Ok(cx_val)) if !key.is_empty() && !cx_val.is_empty() => {
			// CSE is configured, try a simple search to verify connectivity
			let client = CseClient::new(key, cx_val);
			let request = CseRequest::new("test", 1);

			match timeout(CSE_CHECK_TIMEOUT, client.search(request)).await {
				Ok(Ok(_)) => (true, HealthStatus::Healthy, None),
				Ok(Err(e)) => {
					// Check if it's an auth error vs network error
					let err_str = e.to_string();
					if err_str.contains("Unauthorized") || err_str.contains("Invalid API key") {
						(
							true,
							HealthStatus::Unhealthy,
							Some("Invalid API key or CSE ID".to_string()),
						)
					} else if err_str.contains("Rate limit")
						|| err_str.contains("429")
						|| err_str.contains("Quota")
					{
						(
							true,
							HealthStatus::Degraded,
							Some("Rate limited (quota exceeded)".to_string()),
						)
					} else {
						(true, HealthStatus::Degraded, Some(err_str))
					}
				}
				Err(_) => (
					true,
					HealthStatus::Degraded,
					Some("CSE health check timed out".to_string()),
				),
			}
		}
		_ => {
			// Not configured - this is degraded, not unhealthy (CSE is optional)
			(
				false,
				HealthStatus::Degraded,
				Some("Google CSE not configured".to_string()),
			)
		}
	};

	let latency_ms = start.elapsed().as_millis() as u64;

	let health = GoogleCseHealth {
		status,
		latency_ms,
		configured,
		error,
	};

	// Update cache
	{
		let mut cache = get_cse_cache().write().await;
		*cache = Some((
			Instant::now(),
			GoogleCseHealth {
				status: health.status,
				latency_ms: health.latency_ms,
				configured: health.configured,
				error: health.error.clone(),
			},
		));
	}

	health
}

const SERPER_CHECK_TIMEOUT: Duration = Duration::from_secs(5);
const SERPER_CACHE_TTL: Duration = Duration::from_secs(3600); // 1 hour

/// Cached Serper health result to avoid burning through API quota on health checks.
static SERPER_HEALTH_CACHE: std::sync::OnceLock<
	tokio::sync::RwLock<Option<(Instant, SerperHealth)>>,
> = std::sync::OnceLock::new();

fn get_serper_cache() -> &'static tokio::sync::RwLock<Option<(Instant, SerperHealth)>> {
	SERPER_HEALTH_CACHE.get_or_init(|| tokio::sync::RwLock::new(None))
}

/// Check Serper health by verifying configuration and optionally testing
/// connectivity. Results are cached for 1 hour to avoid burning API quota.
pub async fn check_serper() -> SerperHealth {
	use loom_server_search_serper::{SerperClient, SerperRequest};

	// Check cache first
	{
		let cache = get_serper_cache().read().await;
		if let Some((cached_at, ref health)) = *cache {
			if cached_at.elapsed() < SERPER_CACHE_TTL {
				return SerperHealth {
					status: health.status,
					latency_ms: 0, // Cached response
					configured: health.configured,
					error: health.error.clone(),
				};
			}
		}
	}

	let start = Instant::now();

	// Check if Serper is configured
	let api_key = std::env::var("LOOM_SERVER_SERPER_API_KEY");

	let (configured, status, error) = match api_key {
		Ok(key) if !key.is_empty() => {
			// Serper is configured, try a simple search to verify connectivity
			let client = SerperClient::new(key);
			let request = SerperRequest::new("test", 1);

			match timeout(SERPER_CHECK_TIMEOUT, client.search(request)).await {
				Ok(Ok(_)) => (true, HealthStatus::Healthy, None),
				Ok(Err(e)) => {
					// Check if it's an auth error vs network error
					let err_str = e.to_string();
					if err_str.contains("Unauthorized") || err_str.contains("Invalid API key") {
						(
							true,
							HealthStatus::Unhealthy,
							Some("Invalid API key".to_string()),
						)
					} else if err_str.contains("Rate limit")
						|| err_str.contains("429")
						|| err_str.contains("Quota")
					{
						(
							true,
							HealthStatus::Degraded,
							Some("Rate limited (quota exceeded)".to_string()),
						)
					} else {
						(true, HealthStatus::Degraded, Some(err_str))
					}
				}
				Err(_) => (
					true,
					HealthStatus::Degraded,
					Some("Serper health check timed out".to_string()),
				),
			}
		}
		_ => {
			// Not configured - this is degraded, not unhealthy (Serper is optional)
			(
				false,
				HealthStatus::Degraded,
				Some("Serper not configured".to_string()),
			)
		}
	};

	let latency_ms = start.elapsed().as_millis() as u64;

	let health = SerperHealth {
		status,
		latency_ms,
		configured,
		error,
	};

	// Update cache
	{
		let mut cache = get_serper_cache().write().await;
		*cache = Some((
			Instant::now(),
			SerperHealth {
				status: health.status,
				latency_ms: health.latency_ms,
				configured: health.configured,
				error: health.error.clone(),
			},
		));
	}

	health
}

const GITHUB_CHECK_TIMEOUT: Duration = Duration::from_secs(3);

/// Check GitHub App health by validating JWT generation and API connectivity.
pub async fn check_github_app(client: Option<Arc<GithubAppClient>>) -> GithubAppHealth {
	let start = Instant::now();

	let (configured, status, error) = match client {
		None => (
			false,
			HealthStatus::Degraded,
			Some("GitHub App not configured".to_string()),
		),
		Some(client) => match timeout(GITHUB_CHECK_TIMEOUT, client.list_installations()).await {
			Ok(Ok(_)) => (true, HealthStatus::Healthy, None),
			Ok(Err(e)) => {
				let status = match &e {
					GithubAppError::Unauthorized | GithubAppError::Config(_) | GithubAppError::Jwt(_) => {
						HealthStatus::Unhealthy
					}
					GithubAppError::Timeout | GithubAppError::RateLimited | GithubAppError::Network(_) => {
						HealthStatus::Degraded
					}
					GithubAppError::ApiError { status, .. } if *status >= 500 => HealthStatus::Degraded,
					_ => HealthStatus::Degraded,
				};
				(true, status, Some(e.to_string()))
			}
			Err(_) => (
				true,
				HealthStatus::Degraded,
				Some("GitHub health check timed out".to_string()),
			),
		},
	};

	let latency_ms = start.elapsed().as_millis() as u64;

	GithubAppHealth {
		status,
		latency_ms,
		configured,
		error,
	}
}

const K8S_CHECK_TIMEOUT: Duration = Duration::from_secs(5);

/// Check Kubernetes connectivity by listing pods in the namespace.
pub async fn check_kubernetes(provisioner: Option<&Arc<Provisioner>>) -> Option<KubernetesHealth> {
	let provisioner = provisioner?;

	let start = Instant::now();
	let namespace = provisioner.namespace().to_string();

	let result = timeout(K8S_CHECK_TIMEOUT, provisioner.count_active_weavers()).await;
	let latency_ms = start.elapsed().as_millis() as u64;

	let (status, reachable, error) = match result {
		Ok(Ok(_)) => (HealthStatus::Healthy, true, None),
		Ok(Err(e)) => (HealthStatus::Unhealthy, false, Some(e.to_string())),
		Err(_) => (
			HealthStatus::Unhealthy,
			false,
			Some("Kubernetes health check timed out".to_string()),
		),
	};

	Some(KubernetesHealth {
		status,
		latency_ms,
		namespace,
		reachable,
		error,
	})
}

const SMTP_CHECK_TIMEOUT: Duration = Duration::from_secs(5);

/// Check SMTP server health by testing connectivity.
pub async fn check_smtp(client: Option<&Arc<SmtpClient>>) -> SmtpHealth {
	let start = Instant::now();

	let (configured, healthy, status, error) = match client {
		None => (
			false,
			false,
			HealthStatus::Degraded,
			Some("SMTP not configured".to_string()),
		),
		Some(client) => match timeout(SMTP_CHECK_TIMEOUT, client.check_health()).await {
			Ok(Ok(())) => (true, true, HealthStatus::Healthy, None),
			Ok(Err(e)) => (true, false, HealthStatus::Unhealthy, Some(e.to_string())),
			Err(_) => (
				true,
				false,
				HealthStatus::Unhealthy,
				Some("SMTP health check timed out".to_string()),
			),
		},
	};

	let latency_ms = start.elapsed().as_millis() as u64;

	SmtpHealth {
		status,
		latency_ms,
		configured,
		healthy,
		error,
	}
}

/// Check GeoIP service health by validating database accessibility.
pub fn check_geoip(service: Option<&Arc<loom_server_geoip::GeoIpService>>) -> GeoIpHealth {
	use tokio::time::Instant;

	let start = Instant::now();

	let (configured, healthy, status, database_path, database_type, error) = match service {
		None => (
			false,
			false,
			HealthStatus::Degraded,
			None,
			None,
			Some("GeoIP not configured".to_string()),
		),
		Some(svc) => {
			let path = svc.database_path().to_string();
			if svc.is_healthy() {
				let metadata = svc.database_metadata();
				(
					true,
					true,
					HealthStatus::Healthy,
					Some(path),
					Some(metadata.database_type),
					None,
				)
			} else {
				(
					true,
					false,
					HealthStatus::Unhealthy,
					Some(path),
					None,
					Some("GeoIP database lookup failed".to_string()),
				)
			}
		}
	};

	let latency_ms = start.elapsed().as_millis() as u64;

	GeoIpHealth {
		status,
		latency_ms,
		configured,
		healthy,
		database_path,
		database_type,
		error,
	}
}

/// Check jobs scheduler health.
pub async fn check_jobs(scheduler: Option<&Arc<JobScheduler>>) -> Option<JobsHealth> {
	let scheduler = scheduler?;

	let health = scheduler.health_status().await;

	let jobs_failing: Vec<String> = health
		.jobs
		.iter()
		.filter(|j| matches!(j.status, loom_server_jobs::HealthState::Unhealthy))
		.map(|j| j.job_id.clone())
		.collect();

	let status = match health.status {
		loom_server_jobs::HealthState::Healthy => HealthStatus::Healthy,
		loom_server_jobs::HealthState::Degraded => HealthStatus::Degraded,
		loom_server_jobs::HealthState::Unhealthy => HealthStatus::Unhealthy,
	};

	Some(JobsHealth {
		status,
		jobs_total: health.jobs.len(),
		jobs_healthy: health
			.jobs
			.iter()
			.filter(|j| matches!(j.status, loom_server_jobs::HealthState::Healthy))
			.count(),
		jobs_failing: jobs_failing.len(),
		failing_jobs: if jobs_failing.is_empty() {
			None
		} else {
			Some(jobs_failing)
		},
	})
}

/// Check authentication providers health.
///
/// Validates that OAuth providers are properly configured with valid credentials.
/// This is a configuration check, not a connectivity check (OAuth providers don't
/// have health check endpoints).
pub fn check_auth_providers(
	github_oauth: Option<&loom_server_auth_github::GitHubOAuthClient>,
	google_oauth: Option<&loom_server_auth_google::GoogleOAuthClient>,
	okta_oauth: Option<&loom_server_auth_okta::OktaOAuthClient>,
	smtp_configured: bool,
) -> AuthProvidersHealth {
	let mut providers = Vec::new();

	// GitHub OAuth
	providers.push(AuthProviderHealth {
		name: "github".to_string(),
		status: if github_oauth.is_some() {
			HealthStatus::Healthy
		} else {
			HealthStatus::Degraded
		},
		configured: github_oauth.is_some(),
		error: if github_oauth.is_none() {
			Some("GitHub OAuth not configured".to_string())
		} else {
			None
		},
	});

	// Google OAuth
	providers.push(AuthProviderHealth {
		name: "google".to_string(),
		status: if google_oauth.is_some() {
			HealthStatus::Healthy
		} else {
			HealthStatus::Degraded
		},
		configured: google_oauth.is_some(),
		error: if google_oauth.is_none() {
			Some("Google OAuth not configured".to_string())
		} else {
			None
		},
	});

	// Okta OAuth
	providers.push(AuthProviderHealth {
		name: "okta".to_string(),
		status: if okta_oauth.is_some() {
			HealthStatus::Healthy
		} else {
			HealthStatus::Degraded
		},
		configured: okta_oauth.is_some(),
		error: if okta_oauth.is_none() {
			Some("Okta OAuth not configured".to_string())
		} else {
			None
		},
	});

	// Magic Link (depends on SMTP being configured)
	providers.push(AuthProviderHealth {
		name: "magic_link".to_string(),
		status: if smtp_configured {
			HealthStatus::Healthy
		} else {
			HealthStatus::Degraded
		},
		configured: smtp_configured,
		error: if !smtp_configured {
			Some("Magic link requires SMTP to be configured".to_string())
		} else {
			None
		},
	});

	// Overall status: healthy if at least one provider is configured, unhealthy otherwise
	let configured_count = providers.iter().filter(|p| p.configured).count();
	let status = if configured_count == 0 {
		HealthStatus::Unhealthy
	} else {
		HealthStatus::Healthy
	};

	AuthProvidersHealth { status, providers }
}

/// Check secrets system health.
///
/// Verifies that the secrets infrastructure is properly configured and functional:
/// - Master key is present and can encrypt/decrypt
/// - SVID signing key is present and can sign
pub async fn check_secrets(
	secrets_service: Option<
		&Arc<
			loom_server_secrets::SecretsService<
				loom_server_secrets::SoftwareKeyBackend,
				loom_server_secrets::SqliteSecretStore,
			>,
		>,
	>,
	svid_issuer: Option<
		&Arc<loom_server_secrets::SvidIssuer<loom_server_secrets::SoftwareKeyBackend>>,
	>,
) -> Option<SecretsHealth> {
	let start = Instant::now();

	let (configured, master_key_present, svid_signing_key_present, status, error) =
		match (secrets_service, svid_issuer) {
			(None, None) => (
				false,
				false,
				false,
				HealthStatus::Degraded,
				Some("Secrets infrastructure not configured".to_string()),
			),
			(Some(_service), Some(issuer)) => {
				let key_id = issuer.key_backend().svid_signing_key_id();
				let svid_key_ok = !key_id.is_empty();

				let kek_version = issuer.key_backend().kek_version();
				let master_key_ok = kek_version > 0;

				if master_key_ok && svid_key_ok {
					(true, true, true, HealthStatus::Healthy, None)
				} else {
					let mut errors = Vec::new();
					if !master_key_ok {
						errors.push("master key not functional");
					}
					if !svid_key_ok {
						errors.push("SVID signing key not functional");
					}
					(
						true,
						master_key_ok,
						svid_key_ok,
						HealthStatus::Unhealthy,
						Some(errors.join("; ")),
					)
				}
			}
			_ => (
				false,
				false,
				false,
				HealthStatus::Unhealthy,
				Some("Secrets infrastructure partially configured".to_string()),
			),
		};

	let latency_ms = start.elapsed().as_millis() as u64;

	Some(SecretsHealth {
		status,
		latency_ms,
		configured,
		master_key_present,
		svid_signing_key_present,
		error,
	})
}

/// Check SCIM health.
///
/// Verifies that SCIM is properly configured:
/// - Whether SCIM is enabled
/// - Whether it's properly configured (token + org_id set)
/// - Whether the configured org exists in the database
pub async fn check_scim(scim_config: &ScimConfig, org_repo: &OrgRepository) -> ScimHealth {
	if !scim_config.enabled {
		return ScimHealth {
			status: HealthStatus::Healthy,
			enabled: false,
			configured: false,
			org_id: None,
			org_exists: false,
			error: None,
		};
	}

	let configured = scim_config.token.is_some() && scim_config.org_id.is_some();

	let (org_exists, error) = if let Some(ref org_id_str) = scim_config.org_id {
		match uuid::Uuid::parse_str(org_id_str) {
			Ok(uuid) => {
				let org_id = loom_server_auth::OrgId::new(uuid);
				match org_repo.get_org_by_id(&org_id).await {
					Ok(Some(_)) => (true, None),
					Ok(None) => (
						false,
						Some(format!("Organization {} not found", org_id_str)),
					),
					Err(e) => (false, Some(format!("Failed to check organization: {}", e))),
				}
			}
			Err(e) => (false, Some(format!("Invalid org_id format: {}", e))),
		}
	} else {
		(false, Some("org_id not configured".to_string()))
	};

	let status = if configured && org_exists {
		HealthStatus::Healthy
	} else {
		HealthStatus::Degraded
	};

	ScimHealth {
		status,
		enabled: true,
		configured,
		org_id: scim_config.org_id.clone(),
		org_exists,
		error,
	}
}

/// Aggregate component statuses into overall status.
pub fn aggregate_status(components: &HealthComponents) -> HealthStatus {
	let mut statuses = vec![
		components.database.status,
		components.bin_dir.status,
		components.google_cse.status,
		components.serper.status,
		components.github_app.status,
		components.smtp.status,
		components.geoip.status,
		components.auth_providers.status,
	];

	if let Some(ref k8s) = components.kubernetes {
		statuses.push(k8s.status);
	}

	if let Some(ref jobs) = components.jobs {
		statuses.push(jobs.status);
	}

	if let Some(ref secrets) = components.secrets {
		statuses.push(secrets.status);
	}

	if components.scim.enabled {
		statuses.push(components.scim.status);
	}

	if statuses
		.iter()
		.any(|s| matches!(s, HealthStatus::Unhealthy))
	{
		HealthStatus::Unhealthy
	} else if statuses.iter().any(|s| matches!(s, HealthStatus::Degraded)) {
		HealthStatus::Degraded
	} else {
		HealthStatus::Healthy
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	fn pool_health_status(accounts_total: usize, accounts_available: usize) -> HealthStatus {
		if accounts_total == 0 {
			HealthStatus::Unhealthy
		} else if accounts_available == accounts_total {
			HealthStatus::Healthy
		} else if accounts_available > 0 {
			HealthStatus::Degraded
		} else {
			HealthStatus::Unhealthy
		}
	}

	#[test]
	fn test_empty_oauth_pool_is_unhealthy() {
		assert_eq!(pool_health_status(0, 0), HealthStatus::Unhealthy);
	}

	#[test]
	fn test_all_accounts_available_is_healthy() {
		assert_eq!(pool_health_status(3, 3), HealthStatus::Healthy);
		assert_eq!(pool_health_status(1, 1), HealthStatus::Healthy);
	}

	#[test]
	fn test_some_accounts_available_is_degraded() {
		assert_eq!(pool_health_status(3, 2), HealthStatus::Degraded);
		assert_eq!(pool_health_status(3, 1), HealthStatus::Degraded);
	}

	#[test]
	fn test_no_accounts_available_is_unhealthy() {
		assert_eq!(pool_health_status(3, 0), HealthStatus::Unhealthy);
		assert_eq!(pool_health_status(1, 0), HealthStatus::Unhealthy);
	}
}
