// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use loom_common_http::new_client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::{Ipv4Addr, Ipv6Addr};
use thiserror::Error;
use tracing::instrument;

pub const DEFAULT_DERP_MAP_URL: &str = "https://controlplane.tailscale.com/derpmap/default";

#[derive(Error, Debug)]
pub enum DerpMapError {
	#[error("failed to fetch DERP map: {0}")]
	Fetch(#[from] reqwest::Error),

	#[error("failed to parse DERP map: {0}")]
	Parse(#[from] serde_json::Error),

	#[error("failed to read overlay file: {0}")]
	ReadOverlay(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, DerpMapError>;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "PascalCase")]
pub struct DerpMap {
	#[serde(default, deserialize_with = "deserialize_string_key_map")]
	pub regions: HashMap<u16, DerpRegion>,
}

fn deserialize_string_key_map<'de, D>(
	deserializer: D,
) -> std::result::Result<HashMap<u16, DerpRegion>, D::Error>
where
	D: serde::Deserializer<'de>,
{
	use serde::de::Error;
	let string_map: HashMap<String, DerpRegion> = HashMap::deserialize(deserializer)?;
	string_map
		.into_iter()
		.map(|(k, v)| {
			k.parse::<u16>()
				.map(|id| (id, v))
				.map_err(|_| D::Error::custom(format!("invalid region id: {}", k)))
		})
		.collect()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct DerpRegion {
	#[serde(rename = "RegionID")]
	pub region_id: u16,
	pub region_code: String,
	pub region_name: String,
	#[serde(default)]
	pub latitude: f64,
	#[serde(default)]
	pub longitude: f64,
	#[serde(default)]
	pub nodes: Vec<DerpNode>,
	#[serde(default)]
	pub avoid: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct DerpNode {
	pub name: String,
	#[serde(rename = "RegionID")]
	pub region_id: u16,
	pub host_name: String,
	#[serde(default, rename = "IPv4")]
	pub ipv4: Option<Ipv4Addr>,
	#[serde(default, rename = "IPv6")]
	pub ipv6: Option<Ipv6Addr>,
	#[serde(default, rename = "DERPPort")]
	pub derp_port: u16,
	#[serde(default, rename = "STUNPort")]
	pub stun_port: u16,
	#[serde(default)]
	pub stun_only: bool,
	#[serde(default)]
	pub can_port80: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub struct DerpOverlay {
	#[serde(default)]
	pub disable_regions: Vec<u16>,
	#[serde(default)]
	pub custom_regions: HashMap<u16, DerpRegion>,
	#[serde(default)]
	pub omit_default_regions: bool,
}

impl DerpMap {
	pub fn new() -> Self {
		Self::default()
	}

	pub fn get_region(&self, id: u16) -> Option<&DerpRegion> {
		self.regions.get(&id)
	}

	pub fn region_ids(&self) -> Vec<u16> {
		let mut ids: Vec<_> = self.regions.keys().copied().collect();
		ids.sort();
		ids
	}
}

#[instrument(skip_all, fields(url = %url))]
pub async fn fetch_derp_map(url: &str) -> Result<DerpMap> {
	let client = new_client();
	let response = client.get(url).send().await?;
	let map: DerpMap = response.json().await?;
	Ok(map)
}

pub async fn fetch_default_derp_map() -> Result<DerpMap> {
	fetch_derp_map(DEFAULT_DERP_MAP_URL).await
}

pub fn apply_overlay(map: &DerpMap, overlay: &DerpOverlay) -> DerpMap {
	let mut result = if overlay.omit_default_regions {
		DerpMap::new()
	} else {
		map.clone()
	};

	for region_id in &overlay.disable_regions {
		result.regions.remove(region_id);
	}

	for (id, region) in &overlay.custom_regions {
		result.regions.insert(*id, region.clone());
	}

	result
}

pub async fn load_overlay_file(path: &std::path::Path) -> Result<DerpOverlay> {
	let content = tokio::fs::read_to_string(path).await?;
	let overlay: DerpOverlay = serde_json::from_str(&content)?;
	Ok(overlay)
}

#[cfg(test)]
mod tests {
	use super::*;

	fn sample_derp_map() -> DerpMap {
		let mut regions = HashMap::new();
		regions.insert(
			1,
			DerpRegion {
				region_id: 1,
				region_code: "nyc".to_string(),
				region_name: "New York City".to_string(),
				latitude: 40.7128,
				longitude: -74.0060,
				nodes: vec![DerpNode {
					name: "nyc1".to_string(),
					region_id: 1,
					host_name: "derp1.tailscale.com".to_string(),
					ipv4: Some("1.2.3.4".parse().unwrap()),
					ipv6: None,
					derp_port: 443,
					stun_port: 3478,
					stun_only: false,
					can_port80: true,
				}],
				avoid: false,
			},
		);
		regions.insert(
			2,
			DerpRegion {
				region_id: 2,
				region_code: "sfo".to_string(),
				region_name: "San Francisco".to_string(),
				latitude: 37.7749,
				longitude: -122.4194,
				nodes: vec![],
				avoid: false,
			},
		);
		DerpMap { regions }
	}

	#[test]
	fn get_region() {
		let map = sample_derp_map();
		let nyc = map.get_region(1).unwrap();
		assert_eq!(nyc.region_code, "nyc");
	}

	#[test]
	fn region_ids_sorted() {
		let map = sample_derp_map();
		let ids = map.region_ids();
		assert_eq!(ids, vec![1, 2]);
	}

	#[test]
	fn apply_overlay_disable_regions() {
		let map = sample_derp_map();
		let overlay = DerpOverlay {
			disable_regions: vec![1],
			..Default::default()
		};

		let result = apply_overlay(&map, &overlay);
		assert!(result.get_region(1).is_none());
		assert!(result.get_region(2).is_some());
	}

	#[test]
	fn apply_overlay_custom_regions() {
		let map = sample_derp_map();
		let custom_region = DerpRegion {
			region_id: 900,
			region_code: "loom".to_string(),
			region_name: "Loom Private".to_string(),
			latitude: 0.0,
			longitude: 0.0,
			nodes: vec![],
			avoid: false,
		};

		let mut custom_regions = HashMap::new();
		custom_regions.insert(900, custom_region);

		let overlay = DerpOverlay {
			custom_regions,
			..Default::default()
		};

		let result = apply_overlay(&map, &overlay);
		assert!(result.get_region(900).is_some());
		assert!(result.get_region(1).is_some());
	}

	#[test]
	fn apply_overlay_omit_default() {
		let map = sample_derp_map();
		let custom_region = DerpRegion {
			region_id: 900,
			region_code: "loom".to_string(),
			region_name: "Loom Private".to_string(),
			latitude: 0.0,
			longitude: 0.0,
			nodes: vec![],
			avoid: false,
		};

		let mut custom_regions = HashMap::new();
		custom_regions.insert(900, custom_region);

		let overlay = DerpOverlay {
			custom_regions,
			omit_default_regions: true,
			..Default::default()
		};

		let result = apply_overlay(&map, &overlay);
		assert!(result.get_region(900).is_some());
		assert!(result.get_region(1).is_none());
		assert!(result.get_region(2).is_none());
	}
}
