// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct RegisterDeviceRequest {
	#[schema(example = "aaaabbbbccccddddeeeeffffgggghhhh12345678")]
	pub public_key: String,
	#[schema(example = "MacBook Pro")]
	pub name: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct DeviceResponse {
	pub id: String,
	pub public_key: String,
	pub name: Option<String>,
	pub created_at: String,
	pub last_seen_at: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct CreateSessionRequest {
	pub weaver_id: String,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct SessionResponse {
	pub session_id: String,
	pub client_ip: String,
	pub weaver_ip: String,
	pub weaver_public_key: String,
	pub derp_map: serde_json::Value,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct SessionListItem {
	pub session_id: String,
	pub device_id: String,
	pub weaver_id: String,
	pub client_ip: String,
	pub created_at: String,
	pub last_handshake_at: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct RegisterWeaverRequest {
	pub weaver_id: String,
	#[schema(example = "aaaabbbbccccddddeeeeffffgggghhhh12345678")]
	pub public_key: String,
	#[schema(example = 1)]
	pub derp_home_region: Option<u16>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct RegisterWeaverResponse {
	pub assigned_ip: String,
	pub derp_map_url: String,
	pub peers_stream_url: String,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct WeaverResponse {
	pub weaver_id: String,
	pub public_key: String,
	pub assigned_ip: String,
	pub derp_home_region: Option<u16>,
	pub endpoint: Option<String>,
	pub registered_at: String,
	pub last_seen_at: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct UpdateEndpointRequest {
	pub endpoint: String,
}
