// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights
// reserved. SPDX-License-Identifier: Proprietary

//! Client information extraction and GeoIP lookup.
//!
//! This module provides utilities for extracting client metadata from HTTP requests,
//! including IP address, user agent, and geolocation via MaxMind GeoIP lookup.

use axum::http::HeaderMap;
use loom_server_geoip::GeoIpService;
use std::net::IpAddr;
use std::sync::Arc;

/// Extracted client information for session metadata.
#[derive(Debug, Clone, Default)]
pub struct ClientInfo {
	pub ip_address: Option<String>,
	pub user_agent: Option<String>,
	pub geo_city: Option<String>,
	pub geo_region: Option<String>,
	pub geo_country: Option<String>,
}

impl From<ClientInfo> for loom_server_session::ClientInfo {
	fn from(info: ClientInfo) -> Self {
		Self {
			ip_address: info.ip_address,
			user_agent: info.user_agent,
			geo_city: info.geo_city,
			geo_country: info.geo_country,
		}
	}
}

impl ClientInfo {
	/// Extract client info from request headers with optional GeoIP lookup.
	#[tracing::instrument(level = "debug", skip(headers, geoip))]
	pub fn from_headers(headers: &HeaderMap, geoip: Option<&Arc<GeoIpService>>) -> Self {
		let ip_address = extract_client_ip(headers);
		let user_agent = headers
			.get("user-agent")
			.and_then(|v| v.to_str().ok())
			.map(|s| s.to_string());

		let (geo_city, geo_region, geo_country) = match (&ip_address, geoip) {
			(Some(ip_str), Some(svc)) => lookup_geo(ip_str, svc),
			_ => (None, None, None),
		};

		tracing::debug!(
			ip = ?ip_address,
			geo_city = ?geo_city,
			geo_region = ?geo_region,
			geo_country = ?geo_country,
			"Client info extracted"
		);

		Self {
			ip_address,
			user_agent,
			geo_city,
			geo_region,
			geo_country,
		}
	}
}

/// Extract client IP from request headers.
///
/// Checks headers in order of preference:
/// 1. `X-Forwarded-For` (first IP in chain, for reverse proxies)
/// 2. `X-Real-IP` (nginx style)
/// 3. `CF-Connecting-IP` (Cloudflare)
fn extract_client_ip(headers: &HeaderMap) -> Option<String> {
	if let Some(xff) = headers.get("x-forwarded-for") {
		if let Ok(xff_str) = xff.to_str() {
			if let Some(first_ip) = xff_str.split(',').next() {
				let ip = first_ip.trim();
				if !ip.is_empty() {
					return Some(ip.to_string());
				}
			}
		}
	}

	if let Some(real_ip) = headers.get("x-real-ip") {
		if let Ok(ip) = real_ip.to_str() {
			let ip = ip.trim();
			if !ip.is_empty() {
				return Some(ip.to_string());
			}
		}
	}

	if let Some(cf_ip) = headers.get("cf-connecting-ip") {
		if let Ok(ip) = cf_ip.to_str() {
			let ip = ip.trim();
			if !ip.is_empty() {
				return Some(ip.to_string());
			}
		}
	}

	None
}

/// Perform GeoIP lookup for an IP address string.
fn lookup_geo(
	ip_str: &str,
	geoip: &GeoIpService,
) -> (Option<String>, Option<String>, Option<String>) {
	match ip_str.parse::<IpAddr>() {
		Ok(ip) => match geoip.lookup(ip) {
			Ok(location) => (location.city, location.region, location.country),
			Err(e) => {
				tracing::debug!(ip = %ip_str, error = %e, "GeoIP lookup failed");
				(None, None, None)
			}
		},
		Err(_) => {
			tracing::debug!(ip = %ip_str, "Invalid IP address for GeoIP lookup");
			(None, None, None)
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_extract_client_ip_xff() {
		let mut headers = HeaderMap::new();
		headers.insert(
			"x-forwarded-for",
			"203.0.113.195, 70.41.3.18".parse().unwrap(),
		);
		assert_eq!(
			extract_client_ip(&headers),
			Some("203.0.113.195".to_string())
		);
	}

	#[test]
	fn test_extract_client_ip_real_ip() {
		let mut headers = HeaderMap::new();
		headers.insert("x-real-ip", "198.51.100.178".parse().unwrap());
		assert_eq!(
			extract_client_ip(&headers),
			Some("198.51.100.178".to_string())
		);
	}

	#[test]
	fn test_extract_client_ip_cf() {
		let mut headers = HeaderMap::new();
		headers.insert("cf-connecting-ip", "192.0.2.1".parse().unwrap());
		assert_eq!(extract_client_ip(&headers), Some("192.0.2.1".to_string()));
	}

	#[test]
	fn test_extract_client_ip_precedence() {
		let mut headers = HeaderMap::new();
		headers.insert("x-forwarded-for", "1.1.1.1".parse().unwrap());
		headers.insert("x-real-ip", "2.2.2.2".parse().unwrap());
		headers.insert("cf-connecting-ip", "3.3.3.3".parse().unwrap());
		assert_eq!(extract_client_ip(&headers), Some("1.1.1.1".to_string()));
	}

	#[test]
	fn test_extract_client_ip_none() {
		let headers = HeaderMap::new();
		assert_eq!(extract_client_ip(&headers), None);
	}

	#[test]
	fn test_client_info_without_geoip() {
		let mut headers = HeaderMap::new();
		headers.insert("x-forwarded-for", "8.8.8.8".parse().unwrap());
		headers.insert("user-agent", "Mozilla/5.0".parse().unwrap());

		let info = ClientInfo::from_headers(&headers, None);
		assert_eq!(info.ip_address, Some("8.8.8.8".to_string()));
		assert_eq!(info.user_agent, Some("Mozilla/5.0".to_string()));
		assert_eq!(info.geo_city, None);
		assert_eq!(info.geo_region, None);
		assert_eq!(info.geo_country, None);
	}
}
