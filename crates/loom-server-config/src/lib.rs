// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! Centralized configuration management for Loom server.
//!
//! This crate provides:
//! - Layered configuration from multiple sources (defaults, TOML file, environment)
//! - Type-safe configuration with validation
//! - Consistent environment variable naming (`LOOM_SERVER_*`)
//!
//! # Usage
//!
//! ```ignore
//! use loom_server_config::load_config;
//!
//! let config = load_config()?;
//! println!("Server listening on {}:{}", config.http.host, config.http.port);
//! ```

pub mod error;
pub mod layer;
pub mod sections;
pub mod sources;

pub use error::ConfigError;
pub use layer::ServerConfigLayer;
pub use sections::*;
pub use sources::{ConfigSource, DefaultsSource, EnvSource, Precedence, TomlSource};

use tracing::{debug, info};

/// Fully resolved server configuration.
#[derive(Debug, Clone, Default)]
pub struct ServerConfig {
	pub http: HttpConfig,
	pub database: DatabaseConfig,
	pub auth: AuthConfig,
	pub llm: LlmConfig,
	pub weaver: WeaverConfig,
	pub smtp: Option<SmtpConfig>,
	pub oauth: OAuthConfig,
	pub github_app: Option<GitHubAppConfig>,
	pub geoip: Option<GeoIpConfig>,
	pub jobs: JobsConfig,
	pub search: SearchConfig,
	pub paths: PathsConfig,
	pub logging: LoggingConfig,
	pub audit: AuditConfig,
	pub scim: ScimConfig,
	pub analytics: AnalyticsConfig,
}

impl ServerConfig {
	/// Get the socket address string for binding.
	pub fn socket_addr(&self) -> String {
		format!("{}:{}", self.http.host, self.http.port)
	}
}

/// Load configuration from all sources with standard precedence.
///
/// Precedence (highest to lowest):
/// 1. Environment variables (`LOOM_SERVER_*`)
/// 2. Config file (`/etc/loom/server.toml`)
/// 3. Built-in defaults
pub fn load_config() -> Result<ServerConfig, ConfigError> {
	let mut sources: Vec<Box<dyn ConfigSource>> = vec![
		Box::new(DefaultsSource),
		Box::new(TomlSource::system()),
		Box::new(EnvSource),
	];

	sources.sort_by_key(|s| s.precedence());

	let mut merged = ServerConfigLayer::default();
	for source in sources {
		debug!(source = source.name(), "loading configuration source");
		let layer = source.load()?;
		merged.merge(layer);
	}

	finalize(merged)
}

/// Load configuration from environment only (for testing or simple deployments).
pub fn load_config_from_env() -> Result<ServerConfig, ConfigError> {
	let mut merged = ServerConfigLayer::default();
	merged.merge(EnvSource.load()?);
	finalize(merged)
}

/// Load configuration with a custom config file path.
pub fn load_config_with_file(
	config_path: impl Into<std::path::PathBuf>,
) -> Result<ServerConfig, ConfigError> {
	let mut sources: Vec<Box<dyn ConfigSource>> = vec![
		Box::new(DefaultsSource),
		Box::new(TomlSource::new(config_path)),
		Box::new(EnvSource),
	];

	sources.sort_by_key(|s| s.precedence());

	let mut merged = ServerConfigLayer::default();
	for source in sources {
		debug!(source = source.name(), "loading configuration source");
		let layer = source.load()?;
		merged.merge(layer);
	}

	finalize(merged)
}

/// Finalize configuration layer into resolved config.
fn finalize(layer: ServerConfigLayer) -> Result<ServerConfig, ConfigError> {
	let http = layer.http.unwrap_or_default().finalize();
	let database = layer.database.unwrap_or_default().finalize();
	let auth = layer.auth.unwrap_or_default().finalize();
	let llm = layer.llm.unwrap_or_default().finalize();
	let weaver = layer.weaver.unwrap_or_default().finalize();
	let jobs = layer.jobs.unwrap_or_default().finalize();
	let paths = layer.paths.unwrap_or_default().finalize();
	let logging = layer.logging.unwrap_or_default().finalize();
	let search = layer.search.unwrap_or_default().finalize();
	let oauth = layer.oauth.unwrap_or_default().finalize();
	let audit = layer.audit.unwrap_or_default().finalize();

	let smtp = layer.smtp.and_then(|l| l.finalize());
	let github_app = layer.github_app.and_then(|l| l.finalize());
	let geoip = layer.geoip.and_then(|l| l.finalize());

	let scim_token = loom_common_config::load_secret_env("LOOM_SERVER_SCIM_TOKEN")
		.map_err(|e| ConfigError::Secret(e.to_string()))?;
	let scim = layer.scim.unwrap_or_default().finalize(scim_token);
	let analytics = layer.analytics.unwrap_or_default().finalize();

	validate_config(&auth)?;

	info!(
		host = %http.host,
		port = http.port,
		database = %database.url,
		llm_provider = %llm.provider,
		weaver_enabled = weaver.enabled,
		smtp_configured = smtp.is_some(),
		github_app_configured = github_app.is_some(),
		geoip_configured = geoip.is_some(),
		audit_enabled = audit.enabled,
		scim_enabled = scim.enabled,
		analytics_enabled = analytics.enabled,
		"Server configuration loaded"
	);

	Ok(ServerConfig {
		http,
		database,
		auth,
		llm,
		weaver,
		smtp,
		oauth,
		github_app,
		geoip,
		jobs,
		search,
		paths,
		logging,
		audit,
		scim,
		analytics,
	})
}

/// Validate cross-field configuration rules.
fn validate_config(auth: &AuthConfig) -> Result<(), ConfigError> {
	if auth.dev_mode && auth.environment == "production" {
		return Err(ConfigError::Validation(
			"LOOM_SERVER_AUTH_DEV_MODE=1 is set while LOOM_SERVER_ENV=production. \
			 This is a security risk. Remove LOOM_SERVER_AUTH_DEV_MODE or set LOOM_SERVER_ENV \
			 to a non-production value."
				.to_string(),
		));
	}

	Ok(())
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_dev_mode_production_validation() {
		let auth = AuthConfig {
			dev_mode: true,
			environment: "production".to_string(),
			..Default::default()
		};
		let result = validate_config(&auth);
		assert!(result.is_err());
		assert!(result.unwrap_err().to_string().contains("security risk"));
	}

	#[test]
	fn test_dev_mode_development_ok() {
		let auth = AuthConfig {
			dev_mode: true,
			environment: "development".to_string(),
			..Default::default()
		};
		let result = validate_config(&auth);
		assert!(result.is_ok());
	}

	#[test]
	fn test_socket_addr() {
		let config = ServerConfig {
			http: HttpConfig {
				host: "127.0.0.1".to_string(),
				port: 9000,
				base_url: "http://localhost:9000".to_string(),
			},
			database: DatabaseConfig::default(),
			auth: AuthConfig::default(),
			llm: LlmConfig::default(),
			weaver: WeaverConfig::default(),
			smtp: None,
			oauth: OAuthConfig::default(),
			github_app: None,
			geoip: None,
			jobs: JobsConfig::default(),
			search: SearchConfig::default(),
			paths: PathsConfig::default(),
			logging: LoggingConfig::default(),
			audit: AuditConfig::default(),
			scim: ScimConfig::default(),
			analytics: AnalyticsConfig::default(),
		};
		assert_eq!(config.socket_addr(), "127.0.0.1:9000");
	}
}
