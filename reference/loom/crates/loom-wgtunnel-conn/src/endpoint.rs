// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use std::net::SocketAddr;
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct DiscoveredEndpoint {
	pub addr: SocketAddr,
	pub source: EndpointSource,
	pub latency: Option<Duration>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EndpointSource {
	Stun,
	ServerHint,
	Direct,
}

pub fn select_best_endpoint(endpoints: &[DiscoveredEndpoint]) -> Option<&DiscoveredEndpoint> {
	if endpoints.is_empty() {
		return None;
	}

	let with_latency: Vec<_> = endpoints.iter().filter(|e| e.latency.is_some()).collect();

	if !with_latency.is_empty() {
		return with_latency.into_iter().min_by_key(|e| e.latency.unwrap());
	}

	for source in [
		EndpointSource::Direct,
		EndpointSource::Stun,
		EndpointSource::ServerHint,
	] {
		if let Some(ep) = endpoints.iter().find(|e| e.source == source) {
			return Some(ep);
		}
	}

	endpoints.first()
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::net::{Ipv4Addr, SocketAddrV4};

	fn make_addr(port: u16) -> SocketAddr {
		SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(192, 168, 1, 1), port))
	}

	#[test]
	fn test_select_best_by_latency() {
		let endpoints = vec![
			DiscoveredEndpoint {
				addr: make_addr(1000),
				source: EndpointSource::Stun,
				latency: Some(Duration::from_millis(50)),
			},
			DiscoveredEndpoint {
				addr: make_addr(1001),
				source: EndpointSource::Direct,
				latency: Some(Duration::from_millis(10)),
			},
			DiscoveredEndpoint {
				addr: make_addr(1002),
				source: EndpointSource::ServerHint,
				latency: Some(Duration::from_millis(100)),
			},
		];

		let best = select_best_endpoint(&endpoints).unwrap();
		assert_eq!(best.addr.port(), 1001);
		assert_eq!(best.latency, Some(Duration::from_millis(10)));
	}

	#[test]
	fn test_select_best_by_source_priority() {
		let endpoints = vec![
			DiscoveredEndpoint {
				addr: make_addr(1000),
				source: EndpointSource::ServerHint,
				latency: None,
			},
			DiscoveredEndpoint {
				addr: make_addr(1001),
				source: EndpointSource::Stun,
				latency: None,
			},
		];

		let best = select_best_endpoint(&endpoints).unwrap();
		assert_eq!(best.addr.port(), 1001);
		assert_eq!(best.source, EndpointSource::Stun);
	}

	#[test]
	fn test_select_best_prefers_direct() {
		let endpoints = vec![
			DiscoveredEndpoint {
				addr: make_addr(1000),
				source: EndpointSource::Stun,
				latency: None,
			},
			DiscoveredEndpoint {
				addr: make_addr(1001),
				source: EndpointSource::Direct,
				latency: None,
			},
		];

		let best = select_best_endpoint(&endpoints).unwrap();
		assert_eq!(best.addr.port(), 1001);
		assert_eq!(best.source, EndpointSource::Direct);
	}

	#[test]
	fn test_select_best_empty() {
		let endpoints: Vec<DiscoveredEndpoint> = vec![];
		assert!(select_best_endpoint(&endpoints).is_none());
	}
}
