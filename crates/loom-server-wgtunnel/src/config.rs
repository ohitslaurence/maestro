// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use crate::error::{Result, WgError};
use loom_wgtunnel_common::{
	apply_overlay, fetch_derp_map, load_overlay_file, DerpMap, DerpOverlay, DEFAULT_DERP_MAP_URL,
};
use std::path::PathBuf;
use tracing::instrument;

#[derive(Debug, Clone)]
pub struct WgTunnelConfig {
	pub enabled: bool,
	pub ip_prefix: ipnet::Ipv6Net,
	pub derp_map_url: String,
	pub derp_overlay_file: Option<PathBuf>,
	pub derp_map: Option<DerpMap>,
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
	#[error("invalid IP prefix: {0}")]
	InvalidIpPrefix(String),

	#[error("failed to load DERP map: {0}")]
	DerpMapLoad(String),

	#[error("failed to load DERP overlay: {0}")]
	DerpOverlayLoad(String),
}

impl Default for WgTunnelConfig {
	fn default() -> Self {
		Self {
			enabled: true,
			ip_prefix: "fd7a:115c:a1e0::/48".parse().unwrap(),
			derp_map_url: DEFAULT_DERP_MAP_URL.to_string(),
			derp_overlay_file: None,
			derp_map: None,
		}
	}
}

impl WgTunnelConfig {
	pub fn from_env() -> std::result::Result<Self, ConfigError> {
		let enabled = std::env::var("LOOM_WG_ENABLED")
			.map(|v| v.parse().unwrap_or(true))
			.unwrap_or(true);

		let ip_prefix = std::env::var("LOOM_WG_IP_PREFIX")
			.unwrap_or_else(|_| "fd7a:115c:a1e0::/48".to_string())
			.parse()
			.map_err(|e| ConfigError::InvalidIpPrefix(format!("{e}")))?;

		let derp_map_url =
			std::env::var("LOOM_WG_DERP_MAP_URL").unwrap_or_else(|_| DEFAULT_DERP_MAP_URL.to_string());

		let derp_overlay_file = std::env::var("LOOM_WG_DERP_OVERLAY_FILE")
			.ok()
			.map(PathBuf::from);

		Ok(Self {
			enabled,
			ip_prefix,
			derp_map_url,
			derp_overlay_file,
			derp_map: None,
		})
	}

	#[instrument(skip(self), fields(url = %self.derp_map_url))]
	pub async fn load_derp_map(&mut self) -> Result<&DerpMap> {
		if self.derp_map.is_some() {
			return Ok(self.derp_map.as_ref().unwrap());
		}

		let base_map = fetch_derp_map(&self.derp_map_url)
			.await
			.map_err(|e| WgError::DerpMap(e.to_string()))?;

		let overlay = if let Some(ref path) = self.derp_overlay_file {
			load_overlay_file(path)
				.await
				.map_err(|e| WgError::DerpMap(format!("failed to load overlay: {e}")))?
		} else {
			DerpOverlay::default()
		};

		let map = apply_overlay(&base_map, &overlay);
		self.derp_map = Some(map);

		Ok(self.derp_map.as_ref().unwrap())
	}

	pub fn get_derp_map(&self) -> Option<&DerpMap> {
		self.derp_map.as_ref()
	}
}
