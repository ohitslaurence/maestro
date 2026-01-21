// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use loom_wgtunnel_common::WgPublicKey;
use std::collections::HashMap;
use std::net::Ipv6Addr;
use tracing::{debug, instrument, warn};

pub struct Router {
	routes: HashMap<Ipv6Addr, WgPublicKey>,
}

impl Router {
	pub fn new() -> Self {
		Self {
			routes: HashMap::new(),
		}
	}

	#[instrument(skip(self), fields(%ip, peer = %peer))]
	pub fn add_route(&mut self, ip: Ipv6Addr, peer: WgPublicKey) {
		if let Some(existing_peer) = self.routes.get(&ip) {
			if existing_peer == &peer {
				return;
			}
			warn!(
				old_peer = %existing_peer,
				new_peer = %peer,
				"IP address reassigned to different peer"
			);
		}
		self.routes.insert(ip, peer);
		debug!("added route");
	}

	#[instrument(skip(self), fields(peer = %peer))]
	pub fn remove_peer(&mut self, peer: &WgPublicKey) {
		let ips_to_remove: Vec<Ipv6Addr> = self
			.routes
			.iter()
			.filter(|(_, p)| *p == peer)
			.map(|(ip, _)| *ip)
			.collect();

		let count = ips_to_remove.len();
		for ip in ips_to_remove {
			self.routes.remove(&ip);
		}
		debug!(count, "removed routes for peer");
	}

	pub fn route(&self, dst_ip: Ipv6Addr) -> Option<&WgPublicKey> {
		self.routes.get(&dst_ip)
	}

	pub fn route_count(&self) -> usize {
		self.routes.len()
	}

	pub fn routes_for_peer(&self, peer: &WgPublicKey) -> Vec<Ipv6Addr> {
		self
			.routes
			.iter()
			.filter(|(_, p)| *p == peer)
			.map(|(ip, _)| *ip)
			.collect()
	}
}

impl Default for Router {
	fn default() -> Self {
		Self::new()
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use loom_wgtunnel_common::WgKeyPair;

	#[test]
	fn test_add_and_route() {
		let mut router = Router::new();
		let keypair = WgKeyPair::generate();
		let public_key = keypair.public_key().clone();
		let ip: Ipv6Addr = "fd7a:115c:a1e0::2".parse().unwrap();

		router.add_route(ip, public_key.clone());

		let found = router.route(ip);
		assert!(found.is_some());
		assert_eq!(found.unwrap(), &public_key);
	}

	#[test]
	fn test_route_not_found() {
		let router = Router::new();
		let ip: Ipv6Addr = "fd7a:115c:a1e0::2".parse().unwrap();

		let found = router.route(ip);
		assert!(found.is_none());
	}

	#[test]
	fn test_remove_peer() {
		let mut router = Router::new();
		let keypair = WgKeyPair::generate();
		let public_key = keypair.public_key().clone();

		let ip1: Ipv6Addr = "fd7a:115c:a1e0::2".parse().unwrap();
		let ip2: Ipv6Addr = "fd7a:115c:a1e0::3".parse().unwrap();

		router.add_route(ip1, public_key.clone());
		router.add_route(ip2, public_key.clone());

		assert_eq!(router.route_count(), 2);

		router.remove_peer(&public_key);

		assert_eq!(router.route_count(), 0);
		assert!(router.route(ip1).is_none());
		assert!(router.route(ip2).is_none());
	}

	#[test]
	fn test_routes_for_peer() {
		let mut router = Router::new();
		let keypair1 = WgKeyPair::generate();
		let keypair2 = WgKeyPair::generate();

		let pk1 = keypair1.public_key().clone();
		let pk2 = keypair2.public_key().clone();

		let ip1: Ipv6Addr = "fd7a:115c:a1e0::2".parse().unwrap();
		let ip2: Ipv6Addr = "fd7a:115c:a1e0::3".parse().unwrap();
		let ip3: Ipv6Addr = "fd7a:115c:a1e0::4".parse().unwrap();

		router.add_route(ip1, pk1.clone());
		router.add_route(ip2, pk1.clone());
		router.add_route(ip3, pk2.clone());

		let routes = router.routes_for_peer(&pk1);
		assert_eq!(routes.len(), 2);
		assert!(routes.contains(&ip1));
		assert!(routes.contains(&ip2));
	}

	#[test]
	fn test_remove_peer_preserves_other_peers() {
		let mut router = Router::new();
		let keypair1 = WgKeyPair::generate();
		let keypair2 = WgKeyPair::generate();

		let pk1 = keypair1.public_key().clone();
		let pk2 = keypair2.public_key().clone();

		let ip1: Ipv6Addr = "fd7a:115c:a1e0::2".parse().unwrap();
		let ip2: Ipv6Addr = "fd7a:115c:a1e0::3".parse().unwrap();

		router.add_route(ip1, pk1.clone());
		router.add_route(ip2, pk2.clone());

		router.remove_peer(&pk1);

		assert!(router.route(ip1).is_none());
		assert!(router.route(ip2).is_some());
		assert_eq!(router.route(ip2).unwrap(), &pk2);
	}

	#[test]
	fn test_add_route_same_peer_is_noop() {
		let mut router = Router::new();
		let keypair = WgKeyPair::generate();
		let pk = keypair.public_key().clone();
		let ip: Ipv6Addr = "fd7a:115c:a1e0::2".parse().unwrap();

		router.add_route(ip, pk.clone());
		router.add_route(ip, pk.clone());

		assert_eq!(router.route_count(), 1);
		assert_eq!(router.route(ip).unwrap(), &pk);
	}

	#[test]
	fn test_add_route_different_peer_overwrites() {
		let mut router = Router::new();
		let keypair1 = WgKeyPair::generate();
		let keypair2 = WgKeyPair::generate();

		let pk1 = keypair1.public_key().clone();
		let pk2 = keypair2.public_key().clone();
		let ip: Ipv6Addr = "fd7a:115c:a1e0::2".parse().unwrap();

		router.add_route(ip, pk1.clone());
		router.add_route(ip, pk2.clone());

		assert_eq!(router.route_count(), 1);
		assert_eq!(router.route(ip).unwrap(), &pk2);
	}
}
