// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights
// reserved. SPDX-License-Identifier: Proprietary

//! GeoIP lookup service for Loom.
//!
//! This crate provides IP-to-location lookups using MaxMind GeoLite2 databases.
//! The database path is configured via the `LOOM_SERVER_GEOIP_DATABASE_PATH` environment
//! variable.
//!
//! # Usage
//!
//! ```ignore
//! use loom_server_geoip::GeoIpService;
//! use std::net::IpAddr;
//!
//! let service = GeoIpService::from_env()?;
//! let location = service.lookup("8.8.8.8".parse()?)?;
//! println!("City: {:?}, Country: {:?}", location.city, location.country);
//! ```

use std::net::IpAddr;
use std::path::Path;
use std::sync::Arc;

use maxminddb::{geoip2, Reader};
use serde::Serialize;

pub const GEOIP_DATABASE_PATH_ENV: &str = "LOOM_SERVER_GEOIP_DATABASE_PATH";

#[derive(Debug, thiserror::Error)]
pub enum GeoIpError {
	#[error("GeoIP database not configured (set {GEOIP_DATABASE_PATH_ENV})")]
	NotConfigured,

	#[error("GeoIP database not found at path: {0}")]
	DatabaseNotFound(String),

	#[error("Failed to open GeoIP database: {0}")]
	DatabaseOpen(#[source] maxminddb::MaxMindDBError),

	#[error("Failed to lookup IP address: {0}")]
	Lookup(#[source] maxminddb::MaxMindDBError),

	#[error("Invalid IP address: {0}")]
	InvalidIp(String),
}

pub type Result<T> = std::result::Result<T, GeoIpError>;

#[derive(Debug, Clone, Default, Serialize, PartialEq)]
pub struct GeoLocation {
	pub city: Option<String>,
	pub region: Option<String>,
	pub region_code: Option<String>,
	pub country: Option<String>,
	pub country_code: Option<String>,
	pub continent: Option<String>,
	pub latitude: Option<f64>,
	pub longitude: Option<f64>,
	pub timezone: Option<String>,
}

impl GeoLocation {
	pub fn display_string(&self) -> Option<String> {
		match (&self.city, &self.country) {
			(Some(city), Some(country)) => Some(format!("{city}, {country}")),
			(None, Some(country)) => Some(country.clone()),
			(Some(city), None) => Some(city.clone()),
			(None, None) => None,
		}
	}
}

pub struct GeoIpService {
	reader: Arc<Reader<Vec<u8>>>,
	database_path: String,
}

impl std::fmt::Debug for GeoIpService {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.debug_struct("GeoIpService")
			.field("database_path", &self.database_path)
			.finish()
	}
}

impl GeoIpService {
	#[tracing::instrument(level = "info", skip(database_path), fields(path))]
	pub fn new<P: AsRef<Path>>(database_path: P) -> Result<Self> {
		let path = database_path.as_ref();
		let path_str = path.display().to_string();
		tracing::Span::current().record("path", &path_str);

		if !path.exists() {
			return Err(GeoIpError::DatabaseNotFound(path_str));
		}

		let reader = Reader::open_readfile(path).map_err(GeoIpError::DatabaseOpen)?;

		tracing::info!("GeoIP database loaded");

		Ok(Self {
			reader: Arc::new(reader),
			database_path: path_str,
		})
	}

	#[tracing::instrument(level = "debug")]
	pub fn from_env() -> Result<Self> {
		let path = std::env::var(GEOIP_DATABASE_PATH_ENV).map_err(|_| GeoIpError::NotConfigured)?;

		if path.is_empty() {
			return Err(GeoIpError::NotConfigured);
		}

		Self::new(&path)
	}

	pub fn try_from_env() -> Option<Self> {
		match Self::from_env() {
			Ok(service) => Some(service),
			Err(e) => {
				tracing::debug!(error = %e, "GeoIP service not available");
				None
			}
		}
	}

	pub fn database_path(&self) -> &str {
		&self.database_path
	}

	#[tracing::instrument(level = "trace", skip(self), fields(ip = %ip))]
	pub fn lookup(&self, ip: IpAddr) -> Result<GeoLocation> {
		let city: geoip2::City = self.reader.lookup(ip).map_err(GeoIpError::Lookup)?;

		// Extract region from subdivisions (first subdivision is typically the state/province)
		let (region, region_code) = city
			.subdivisions
			.as_ref()
			.and_then(|subs| subs.first())
			.map(|sub| {
				let name = sub
					.names
					.as_ref()
					.and_then(|n| n.get("en").copied())
					.map(String::from);
				let code = sub.iso_code.map(String::from);
				(name, code)
			})
			.unwrap_or((None, None));

		let location = GeoLocation {
			city: city
				.city
				.and_then(|c| c.names)
				.and_then(|n| n.get("en").copied())
				.map(String::from),
			region,
			region_code,
			country: city
				.country
				.as_ref()
				.and_then(|c| c.names.as_ref())
				.and_then(|n| n.get("en").copied())
				.map(String::from),
			country_code: city
				.country
				.as_ref()
				.and_then(|c| c.iso_code)
				.map(String::from),
			continent: city
				.continent
				.and_then(|c| c.names)
				.and_then(|n| n.get("en").copied())
				.map(String::from),
			latitude: city.location.as_ref().and_then(|l| l.latitude),
			longitude: city.location.as_ref().and_then(|l| l.longitude),
			timezone: city
				.location
				.as_ref()
				.and_then(|l| l.time_zone)
				.map(String::from),
		};

		Ok(location)
	}

	#[tracing::instrument(level = "trace", skip(self), fields(ip = %ip_str))]
	pub fn lookup_str(&self, ip_str: &str) -> Result<GeoLocation> {
		let ip: IpAddr = ip_str
			.parse()
			.map_err(|_| GeoIpError::InvalidIp(ip_str.to_string()))?;
		self.lookup(ip)
	}

	pub fn is_healthy(&self) -> bool {
		let test_ip: IpAddr = "8.8.8.8".parse().unwrap();
		self.reader.lookup::<geoip2::City>(test_ip).is_ok()
	}

	pub fn database_metadata(&self) -> DatabaseMetadata {
		let metadata = &self.reader.metadata;
		DatabaseMetadata {
			database_type: metadata.database_type.clone(),
			build_epoch: metadata.build_epoch,
			node_count: metadata.node_count,
			ip_version: metadata.ip_version,
		}
	}
}

#[derive(Debug, Clone, Serialize)]
pub struct DatabaseMetadata {
	pub database_type: String,
	pub build_epoch: u64,
	pub node_count: u32,
	pub ip_version: u16,
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_geo_location_display_string() {
		let loc = GeoLocation {
			city: Some("Mountain View".to_string()),
			country: Some("United States".to_string()),
			..Default::default()
		};
		assert_eq!(
			loc.display_string(),
			Some("Mountain View, United States".to_string())
		);

		let loc = GeoLocation {
			city: None,
			country: Some("United States".to_string()),
			..Default::default()
		};
		assert_eq!(loc.display_string(), Some("United States".to_string()));

		let loc = GeoLocation {
			city: Some("Mountain View".to_string()),
			country: None,
			..Default::default()
		};
		assert_eq!(loc.display_string(), Some("Mountain View".to_string()));

		let loc = GeoLocation::default();
		assert_eq!(loc.display_string(), None);
	}

	#[test]
	fn test_from_env_not_configured() {
		std::env::remove_var(GEOIP_DATABASE_PATH_ENV);
		let result = GeoIpService::from_env();
		assert!(matches!(result, Err(GeoIpError::NotConfigured)));
	}

	#[test]
	fn test_database_not_found() {
		let result = GeoIpService::new("/nonexistent/path/to/database.mmdb");
		assert!(matches!(result, Err(GeoIpError::DatabaseNotFound(_))));
	}

	#[test]
	fn test_invalid_ip() {
		std::env::set_var(GEOIP_DATABASE_PATH_ENV, "/tmp/test.mmdb");
		let service = GeoIpService::try_from_env();
		// Clean up env var to avoid interfering with other tests
		std::env::remove_var(GEOIP_DATABASE_PATH_ENV);
		if let Some(svc) = service {
			let result = svc.lookup_str("not-an-ip");
			assert!(matches!(result, Err(GeoIpError::InvalidIp(_))));
		}
	}
}

#[cfg(test)]
mod proptests {
	use super::*;
	use proptest::prelude::*;

	proptest! {
		#[test]
		fn test_geo_location_display_does_not_panic(
			city in proptest::option::of("[a-zA-Z ]{1,50}"),
			region in proptest::option::of("[a-zA-Z ]{1,50}"),
			country in proptest::option::of("[a-zA-Z ]{1,50}")
		) {
			let loc = GeoLocation {
				city,
				region,
				country,
				..Default::default()
			};
			let _ = loc.display_string();
		}

		/// Property: GeoLocation with region and country displays properly
		#[test]
		fn test_geo_location_with_all_fields(
			city in "[a-zA-Z ]{1,30}",
			region in "[a-zA-Z ]{1,30}",
			region_code in "[A-Z]{2}",
			country in "[a-zA-Z ]{1,30}",
			country_code in "[A-Z]{2}",
			lat in -90.0f64..90.0,
			lon in -180.0f64..180.0,
		) {
			let loc = GeoLocation {
				city: Some(city.clone()),
				region: Some(region),
				region_code: Some(region_code),
				country: Some(country.clone()),
				country_code: Some(country_code),
				continent: Some("Test Continent".to_string()),
				latitude: Some(lat),
				longitude: Some(lon),
				timezone: Some("UTC".to_string()),
			};

			// Should display as "City, Country"
			let display = loc.display_string();
			prop_assert!(display.is_some());
			let display_str = display.unwrap();
			prop_assert!(display_str.contains(&city));
			prop_assert!(display_str.contains(&country));
		}

		/// Property: Default GeoLocation has no display string
		#[test]
		fn test_default_geo_location_no_display(_dummy in 0..1) {
			let loc = GeoLocation::default();
			prop_assert!(loc.display_string().is_none());
			prop_assert!(loc.city.is_none());
			prop_assert!(loc.region.is_none());
			prop_assert!(loc.region_code.is_none());
			prop_assert!(loc.country.is_none());
			prop_assert!(loc.country_code.is_none());
		}
	}
}
