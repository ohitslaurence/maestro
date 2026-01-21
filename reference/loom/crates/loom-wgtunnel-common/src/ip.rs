// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use std::net::Ipv6Addr;
use std::sync::atomic::{AtomicU64, Ordering};
use thiserror::Error;

pub const NETWORK_PREFIX: &str = "fd7a:115c:a1e0::/48";
pub const WEAVER_SUBNET: &str = "fd7a:115c:a1e0:1::/64";
pub const CLIENT_SUBNET: &str = "fd7a:115c:a1e0:2::/64";
pub const SERVER_IP: &str = "fd7a:115c:a1e0::1";

const WEAVER_SUBNET_BASE: u128 = 0xfd7a_115c_a1e0_0001_0000_0000_0000_0000;
const CLIENT_SUBNET_BASE: u128 = 0xfd7a_115c_a1e0_0002_0000_0000_0000_0000;
const SUBNET_HOST_MASK: u128 = 0x0000_0000_0000_0000_FFFF_FFFF_FFFF_FFFF;
const SUBNET_PREFIX_MASK: u128 = 0xFFFF_FFFF_FFFF_FFFF_0000_0000_0000_0000;

#[derive(Error, Debug)]
pub enum IpError {
	#[error("IP address pool exhausted")]
	PoolExhausted,

	#[error("invalid IPv6 address: {0}")]
	InvalidAddress(String),

	#[error("IP address not in expected subnet: {0}")]
	NotInSubnet(String),
}

pub type Result<T> = std::result::Result<T, IpError>;

pub struct IpAllocator {
	weaver_counter: AtomicU64,
	client_counter: AtomicU64,
}

impl Default for IpAllocator {
	fn default() -> Self {
		Self::new()
	}
}

impl IpAllocator {
	pub fn new() -> Self {
		Self {
			weaver_counter: AtomicU64::new(1),
			client_counter: AtomicU64::new(1),
		}
	}

	pub fn allocate_weaver_ip(&self) -> Result<Ipv6Addr> {
		let host = self.weaver_counter.fetch_add(1, Ordering::SeqCst);
		if host as u128 > SUBNET_HOST_MASK {
			return Err(IpError::PoolExhausted);
		}
		let addr = WEAVER_SUBNET_BASE | (host as u128);
		Ok(Ipv6Addr::from(addr))
	}

	pub fn allocate_client_ip(&self) -> Result<Ipv6Addr> {
		let host = self.client_counter.fetch_add(1, Ordering::SeqCst);
		if host as u128 > SUBNET_HOST_MASK {
			return Err(IpError::PoolExhausted);
		}
		let addr = CLIENT_SUBNET_BASE | (host as u128);
		Ok(Ipv6Addr::from(addr))
	}
}

pub fn server_ip() -> Ipv6Addr {
	"fd7a:115c:a1e0::1".parse().unwrap()
}

pub fn is_weaver_ip(addr: Ipv6Addr) -> bool {
	let bits: u128 = addr.into();
	(bits & SUBNET_PREFIX_MASK) == WEAVER_SUBNET_BASE
}

pub fn is_client_ip(addr: Ipv6Addr) -> bool {
	let bits: u128 = addr.into();
	(bits & SUBNET_PREFIX_MASK) == CLIENT_SUBNET_BASE
}

pub fn parse_ipv6(s: &str) -> Result<Ipv6Addr> {
	s.parse()
		.map_err(|_| IpError::InvalidAddress(s.to_string()))
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn allocate_weaver_ips() {
		let allocator = IpAllocator::new();

		let ip1 = allocator.allocate_weaver_ip().unwrap();
		let ip2 = allocator.allocate_weaver_ip().unwrap();

		assert_ne!(ip1, ip2);
		assert!(is_weaver_ip(ip1));
		assert!(is_weaver_ip(ip2));
		assert!(!is_client_ip(ip1));
	}

	#[test]
	fn allocate_client_ips() {
		let allocator = IpAllocator::new();

		let ip1 = allocator.allocate_client_ip().unwrap();
		let ip2 = allocator.allocate_client_ip().unwrap();

		assert_ne!(ip1, ip2);
		assert!(is_client_ip(ip1));
		assert!(is_client_ip(ip2));
		assert!(!is_weaver_ip(ip1));
	}

	#[test]
	fn server_ip_is_correct() {
		let ip = server_ip();
		assert_eq!(ip.to_string(), "fd7a:115c:a1e0::1");
	}

	#[test]
	fn weaver_ip_format() {
		let allocator = IpAllocator::new();
		let ip = allocator.allocate_weaver_ip().unwrap();
		assert!(ip.to_string().starts_with("fd7a:115c:a1e0:1:"));
	}

	#[test]
	fn client_ip_format() {
		let allocator = IpAllocator::new();
		let ip = allocator.allocate_client_ip().unwrap();
		assert!(ip.to_string().starts_with("fd7a:115c:a1e0:2:"));
	}

	#[test]
	fn parse_valid_ipv6() {
		let ip = parse_ipv6("fd7a:115c:a1e0::1").unwrap();
		assert_eq!(ip, server_ip());
	}

	#[test]
	fn parse_invalid_ipv6() {
		let result = parse_ipv6("not-an-ip");
		assert!(result.is_err());
	}
}
