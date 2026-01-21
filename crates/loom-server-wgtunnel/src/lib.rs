// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

pub mod config;
pub mod derp;
pub mod devices;
pub mod error;
pub mod ip_allocator;
pub mod peer_stream;
pub mod sessions;
pub mod types;
pub mod weavers;

pub use config::{ConfigError, WgTunnelConfig};
pub use derp::DerpMapService;
pub use devices::{Device, DeviceService};
pub use error::{Result, WgError};
pub use ip_allocator::IpAllocator;
pub use peer_stream::{PeerEvent, PeerNotifier};
pub use sessions::{CreateSessionRequest, CreateSessionResponse, Session, SessionService};
pub use types::{
	CreateSessionRequest as CreateSessionApiRequest, DeviceResponse, RegisterDeviceRequest,
	RegisterWeaverRequest, RegisterWeaverResponse, SessionListItem, SessionResponse,
	UpdateEndpointRequest, WeaverResponse,
};
pub use weavers::{WeaverWg, WeaverWgService};

use loom_server_db::WgTunnelRepository;
use sqlx::SqlitePool;
use std::sync::Arc;

#[derive(Clone)]
pub struct WgTunnelServices {
	pub device_service: DeviceService,
	pub weaver_service: WeaverWgService,
	pub session_service: SessionService,
	pub derp_service: DerpMapService,
	pub peer_notifier: Arc<PeerNotifier>,
	pub config: Arc<WgTunnelConfig>,
}

impl WgTunnelServices {
	pub async fn new(db: SqlitePool, config: WgTunnelConfig) -> Result<Self> {
		let config = Arc::new(config);
		let repo = WgTunnelRepository::new(db);
		let ip_allocator = Arc::new(IpAllocator::new(repo.clone()).await?);
		let peer_notifier = Arc::new(PeerNotifier::new());
		let device_service = DeviceService::new(repo.clone());
		let weaver_service = WeaverWgService::new(repo.clone(), ip_allocator.clone());
		let session_service = SessionService::new(
			repo,
			ip_allocator,
			peer_notifier.clone(),
			device_service.clone(),
			weaver_service.clone(),
			config.clone(),
		);
		let derp_service = DerpMapService::new((*config).clone());

		Ok(Self {
			device_service,
			weaver_service,
			session_service,
			derp_service,
			peer_notifier,
			config,
		})
	}

	pub fn is_enabled(&self) -> bool {
		self.config.enabled
	}
}
