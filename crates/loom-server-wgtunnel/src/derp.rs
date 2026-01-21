// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use crate::config::WgTunnelConfig;
use crate::error::Result;
use loom_wgtunnel_common::DerpMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::instrument;

#[derive(Clone)]
pub struct DerpMapService {
	config: Arc<RwLock<WgTunnelConfig>>,
}

impl DerpMapService {
	pub fn new(config: WgTunnelConfig) -> Self {
		Self {
			config: Arc::new(RwLock::new(config)),
		}
	}

	#[instrument(skip(self))]
	pub async fn get_derp_map(&self) -> Result<DerpMap> {
		let mut config = self.config.write().await;
		let map = config.load_derp_map().await?;
		Ok(map.clone())
	}

	#[instrument(skip(self))]
	pub async fn refresh_derp_map(&self) -> Result<DerpMap> {
		let mut config = self.config.write().await;
		config.derp_map = None;
		let map = config.load_derp_map().await?;
		Ok(map.clone())
	}

	pub async fn get_cached_derp_map(&self) -> Option<DerpMap> {
		let config = self.config.read().await;
		config.derp_map.clone()
	}

	pub async fn is_enabled(&self) -> bool {
		let config = self.config.read().await;
		config.enabled
	}
}
