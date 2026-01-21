// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use std::collections::HashMap;
use std::net::IpAddr;
use std::time::{Duration, Instant};

const MIN_TTL: Duration = Duration::from_secs(5);
const MAX_TTL: Duration = Duration::from_secs(600);
#[allow(dead_code)] // Available for future use when TTL not provided
const DEFAULT_TTL: Duration = Duration::from_secs(60);
const MAX_ENTRIES: usize = 10_000;

#[derive(Debug, Clone)]
struct DnsEntry {
	hostname: String,
	expires_at: Instant,
	last_used: Instant,
}

#[derive(Debug)]
pub struct DnsCache {
	entries: HashMap<IpAddr, DnsEntry>,
	max_entries: usize,
}

impl Default for DnsCache {
	fn default() -> Self {
		Self::new()
	}
}

impl DnsCache {
	pub fn new() -> Self {
		DnsCache {
			entries: HashMap::new(),
			max_entries: MAX_ENTRIES,
		}
	}

	#[allow(dead_code)] // Used in tests; available for custom cache sizing
	pub fn with_max_entries(max_entries: usize) -> Self {
		DnsCache {
			entries: HashMap::new(),
			max_entries,
		}
	}

	pub fn insert(&mut self, ip: IpAddr, hostname: String, ttl_secs: u32) {
		let ttl = Duration::from_secs(ttl_secs as u64).clamp(MIN_TTL, MAX_TTL);
		let now = Instant::now();

		if self.entries.len() >= self.max_entries {
			self.evict_lru();
		}

		self.entries.insert(
			ip,
			DnsEntry {
				hostname,
				expires_at: now + ttl,
				last_used: now,
			},
		);
	}

	pub fn lookup(&mut self, ip: &IpAddr) -> Option<String> {
		let now = Instant::now();

		if let Some(entry) = self.entries.get_mut(ip) {
			if entry.expires_at > now {
				entry.last_used = now;
				return Some(entry.hostname.clone());
			} else {
				self.entries.remove(ip);
			}
		}

		None
	}

	#[allow(dead_code)] // Used in tests; useful for diagnostics
	pub fn len(&self) -> usize {
		self.entries.len()
	}

	#[allow(dead_code)] // Used in tests; useful for diagnostics
	pub fn is_empty(&self) -> bool {
		self.entries.is_empty()
	}

	#[allow(dead_code)] // Available for periodic maintenance
	pub fn cleanup_expired(&mut self) {
		let now = Instant::now();
		self.entries.retain(|_, entry| entry.expires_at > now);
	}

	fn evict_lru(&mut self) {
		if let Some((oldest_ip, _)) = self
			.entries
			.iter()
			.min_by_key(|(_, entry)| entry.last_used)
			.map(|(k, v)| (*k, v.clone()))
		{
			self.entries.remove(&oldest_ip);
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::net::Ipv4Addr;

	#[test]
	fn test_insert_and_lookup() {
		let mut cache = DnsCache::new();
		let ip = IpAddr::V4(Ipv4Addr::new(8, 8, 8, 8));

		cache.insert(ip, "dns.google".to_string(), 60);
		assert_eq!(cache.lookup(&ip), Some("dns.google".to_string()));
	}

	#[test]
	fn test_lookup_miss() {
		let mut cache = DnsCache::new();
		let ip = IpAddr::V4(Ipv4Addr::new(8, 8, 8, 8));

		assert_eq!(cache.lookup(&ip), None);
	}

	#[test]
	fn test_ttl_clamping() {
		let mut cache = DnsCache::new();
		let ip = IpAddr::V4(Ipv4Addr::new(1, 1, 1, 1));

		cache.insert(ip, "one.one.one.one".to_string(), 1);
		assert!(cache.lookup(&ip).is_some());

		cache.insert(ip, "one.one.one.one".to_string(), 10000);
		assert!(cache.lookup(&ip).is_some());
	}

	#[test]
	fn test_lru_eviction() {
		let mut cache = DnsCache::with_max_entries(3);

		cache.insert(IpAddr::V4(Ipv4Addr::new(1, 0, 0, 1)), "a".to_string(), 60);
		cache.insert(IpAddr::V4(Ipv4Addr::new(1, 0, 0, 2)), "b".to_string(), 60);
		cache.insert(IpAddr::V4(Ipv4Addr::new(1, 0, 0, 3)), "c".to_string(), 60);

		cache.lookup(&IpAddr::V4(Ipv4Addr::new(1, 0, 0, 2)));
		cache.lookup(&IpAddr::V4(Ipv4Addr::new(1, 0, 0, 3)));

		cache.insert(IpAddr::V4(Ipv4Addr::new(1, 0, 0, 4)), "d".to_string(), 60);

		assert!(cache
			.lookup(&IpAddr::V4(Ipv4Addr::new(1, 0, 0, 1)))
			.is_none());
		assert!(cache
			.lookup(&IpAddr::V4(Ipv4Addr::new(1, 0, 0, 2)))
			.is_some());
		assert!(cache
			.lookup(&IpAddr::V4(Ipv4Addr::new(1, 0, 0, 3)))
			.is_some());
		assert!(cache
			.lookup(&IpAddr::V4(Ipv4Addr::new(1, 0, 0, 4)))
			.is_some());
	}

	#[test]
	fn test_max_entries_enforced() {
		let mut cache = DnsCache::with_max_entries(100);

		for i in 0..200u8 {
			cache.insert(
				IpAddr::V4(Ipv4Addr::new(10, 0, 0, i)),
				format!("host{}", i),
				60,
			);
		}

		assert!(cache.len() <= 100);
	}
}
