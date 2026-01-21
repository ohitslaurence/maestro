// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use crate::error::{Result, WgError};
use loom_server_db::WgTunnelRepository;
use std::net::Ipv6Addr;
use std::sync::atomic::{AtomicU64, Ordering};
use tracing::instrument;
use uuid::Uuid;

const WEAVER_SUBNET_BASE: u128 = 0xfd7a_115c_a1e0_0001_0000_0000_0000_0000;
const CLIENT_SUBNET_BASE: u128 = 0xfd7a_115c_a1e0_0002_0000_0000_0000_0000;
const SUBNET_HOST_MASK: u128 = 0x0000_0000_0000_0000_FFFF_FFFF_FFFF_FFFF;

pub struct IpAllocator {
	repo: WgTunnelRepository,
	weaver_counter: AtomicU64,
	client_counter: AtomicU64,
}

impl IpAllocator {
	pub async fn new(repo: WgTunnelRepository) -> Result<Self> {
		let allocator = Self {
			repo,
			weaver_counter: AtomicU64::new(1),
			client_counter: AtomicU64::new(1),
		};
		allocator.initialize_counters().await?;
		Ok(allocator)
	}

	async fn initialize_counters(&self) -> Result<()> {
		let weaver_max = self.get_max_host_number("weaver").await?;
		let client_max = self.get_max_host_number("client").await?;

		self.weaver_counter.store(weaver_max + 1, Ordering::SeqCst);
		self.client_counter.store(client_max + 1, Ordering::SeqCst);

		Ok(())
	}

	async fn get_max_host_number(&self, allocation_type: &str) -> Result<u64> {
		let ips = self.repo.get_allocated_ips_by_type(allocation_type).await?;

		let mut max_host: u64 = 0;
		for (ip_str,) in ips {
			if let Ok(addr) = ip_str.parse::<Ipv6Addr>() {
				let addr_u128: u128 = addr.into();
				let host = (addr_u128 & SUBNET_HOST_MASK) as u64;
				if host > max_host {
					max_host = host;
				}
			}
		}

		Ok(max_host)
	}

	#[instrument(skip(self), fields(%weaver_id))]
	pub async fn allocate_weaver_ip(&self, weaver_id: Uuid) -> Result<Ipv6Addr> {
		let existing = self.repo.get_allocation_for_entity(weaver_id).await?;

		if let Some((ip_str,)) = existing {
			return ip_str
				.parse()
				.map_err(|_| WgError::IpAllocation("invalid stored IP".to_string()));
		}

		let host = self.weaver_counter.fetch_add(1, Ordering::SeqCst);
		if host as u128 > SUBNET_HOST_MASK {
			return Err(WgError::IpAllocation(
				"weaver IP pool exhausted".to_string(),
			));
		}

		let addr = Ipv6Addr::from(WEAVER_SUBNET_BASE | (host as u128));
		let ip_str = addr.to_string();

		self
			.repo
			.insert_ip_allocation(&ip_str, "weaver", weaver_id)
			.await?;

		Ok(addr)
	}

	#[instrument(skip(self), fields(%session_id))]
	pub async fn allocate_client_ip(&self, session_id: Uuid) -> Result<Ipv6Addr> {
		let existing = self.repo.get_allocation_for_entity(session_id).await?;

		if let Some((ip_str,)) = existing {
			return ip_str
				.parse()
				.map_err(|_| WgError::IpAllocation("invalid stored IP".to_string()));
		}

		let host = self.client_counter.fetch_add(1, Ordering::SeqCst);
		if host as u128 > SUBNET_HOST_MASK {
			return Err(WgError::IpAllocation(
				"client IP pool exhausted".to_string(),
			));
		}

		let addr = Ipv6Addr::from(CLIENT_SUBNET_BASE | (host as u128));
		let ip_str = addr.to_string();

		self
			.repo
			.insert_ip_allocation(&ip_str, "client", session_id)
			.await?;

		Ok(addr)
	}

	#[instrument(skip(self), fields(%ip))]
	pub async fn release_ip(&self, ip: Ipv6Addr) -> Result<()> {
		self.repo.release_ip(ip).await?;

		Ok(())
	}
}
