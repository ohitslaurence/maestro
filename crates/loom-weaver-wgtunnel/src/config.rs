// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use crate::error::ConfigError;
use url::Url;

fn validate_https_url(url: &Url) -> Result<(), ConfigError> {
	if url.scheme() != "https" {
		return Err(ConfigError::Parse(
			"server URL must use https://".to_string(),
		));
	}
	Ok(())
}

#[derive(Debug, Clone)]
pub struct WeaverWgConfig {
	pub server_url: Url,
	pub weaver_id: String,
	pub derp_home_region: Option<u16>,
	pub mtu: u16,
	pub heartbeat_interval_secs: u64,
	pub enabled: bool,
}

impl WeaverWgConfig {
	pub fn from_env() -> Result<Self, ConfigError> {
		let server_url: Url = std::env::var("LOOM_SERVER_URL")
			.map_err(|_| ConfigError::MissingEnv("LOOM_SERVER_URL".to_string()))?
			.parse()
			.map_err(|e| ConfigError::Parse(format!("invalid LOOM_SERVER_URL: {e}")))?;
		validate_https_url(&server_url)?;

		let weaver_id = std::env::var("LOOM_WEAVER_ID")
			.map_err(|_| ConfigError::MissingEnv("LOOM_WEAVER_ID".to_string()))?;

		let derp_home_region = std::env::var("LOOM_WG_DERP_REGION")
			.ok()
			.and_then(|s| s.parse().ok());

		let mtu = std::env::var("LOOM_WG_MTU")
			.ok()
			.and_then(|s| s.parse().ok())
			.unwrap_or(1280);

		let heartbeat_interval_secs = std::env::var("LOOM_WG_HEARTBEAT_SECS")
			.ok()
			.and_then(|s| s.parse().ok())
			.unwrap_or(30);

		let enabled = std::env::var("LOOM_WG_ENABLED")
			.map(|v| v != "0" && v.to_lowercase() != "false")
			.unwrap_or(true);

		Ok(Self {
			server_url,
			weaver_id,
			derp_home_region,
			mtu,
			heartbeat_interval_secs,
			enabled,
		})
	}

	pub fn new(server_url: Url, weaver_id: String) -> Result<Self, ConfigError> {
		validate_https_url(&server_url)?;
		Ok(Self {
			server_url,
			weaver_id,
			derp_home_region: None,
			mtu: 1280,
			heartbeat_interval_secs: 30,
			enabled: true,
		})
	}

	#[cfg(test)]
	pub fn new_insecure(server_url: Url, weaver_id: String) -> Self {
		Self {
			server_url,
			weaver_id,
			derp_home_region: None,
			mtu: 1280,
			heartbeat_interval_secs: 30,
			enabled: true,
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_config_new() {
		let config = WeaverWgConfig::new(
			"https://loom.example.com".parse().unwrap(),
			"weaver-123".to_string(),
		)
		.unwrap();
		assert_eq!(config.weaver_id, "weaver-123");
		assert_eq!(config.mtu, 1280);
		assert_eq!(config.heartbeat_interval_secs, 30);
		assert!(config.enabled);
	}

	#[test]
	fn test_config_new_rejects_http() {
		let result = WeaverWgConfig::new(
			"http://loom.example.com".parse().unwrap(),
			"weaver-123".to_string(),
		);
		assert!(result.is_err());
	}
}
