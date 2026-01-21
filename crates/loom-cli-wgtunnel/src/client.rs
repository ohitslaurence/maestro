// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use crate::error::{CliError, Result};
use chrono::{DateTime, Utc};
use loom_common_secret::SecretString;
use loom_wgtunnel_common::{DerpMap, WgKeyPair, WgPublicKey};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tracing::instrument;
use url::Url;

pub struct WgTunnelClient {
	http: Client,
	base_url: Url,
	auth_token: SecretString,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceInfo {
	pub id: String,
	pub public_key: String,
	#[serde(default)]
	pub name: Option<String>,
	pub created_at: DateTime<Utc>,
	#[serde(default)]
	pub last_seen_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionInfo {
	pub session_id: String,
	pub client_ip: String,
	pub weaver_ip: String,
	pub weaver_public_key: String,
	#[serde(default)]
	pub weaver_derp_region: Option<u16>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateSessionResponse {
	pub session_id: String,
	pub client_ip: String,
	pub weaver: WeaverInfo,
	pub derp_map: DerpMap,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WeaverInfo {
	pub public_key: String,
	pub ip: String,
	pub derp_home_region: u16,
}

#[derive(Debug, Serialize)]
struct RegisterDeviceRequest<'a> {
	device_id: &'a str,
	public_key: &'a str,
	#[serde(skip_serializing_if = "Option::is_none")]
	name: Option<&'a str>,
}

#[derive(Debug, Serialize)]
struct CreateSessionRequest<'a> {
	weaver_id: &'a str,
	device_id: &'a str,
}

fn validate_https_url(url: &Url) -> Result<()> {
	if url.scheme() != "https" {
		return Err(CliError::Other("server URL must use https://".to_string()));
	}
	Ok(())
}

impl WgTunnelClient {
	pub fn new(base_url: Url, auth_token: SecretString) -> Result<Self> {
		validate_https_url(&base_url)?;
		let http = loom_common_http::new_client();
		Ok(Self {
			http,
			base_url,
			auth_token,
		})
	}

	#[cfg(test)]
	pub fn new_insecure(base_url: Url, auth_token: SecretString) -> Self {
		let http = loom_common_http::new_client();
		Self {
			http,
			base_url,
			auth_token,
		}
	}

	fn api_url(&self, path: &str) -> Result<Url> {
		Ok(self.base_url.join(path)?)
	}

	#[instrument(skip(self, public_key), fields(public_key = %public_key))]
	pub async fn register_device(
		&self,
		device_id: &str,
		public_key: &WgPublicKey,
		name: Option<&str>,
	) -> Result<DeviceInfo> {
		let url = self.api_url("/api/wg/devices")?;

		let request = RegisterDeviceRequest {
			device_id,
			public_key: &public_key.to_base64(),
			name,
		};

		let response = self
			.http
			.post(url)
			.bearer_auth(self.auth_token.expose())
			.json(&request)
			.send()
			.await?;

		if !response.status().is_success() {
			let status = response.status().as_u16();
			let message = response.text().await.unwrap_or_default();
			return Err(CliError::Api { status, message });
		}

		let device = response.json().await?;
		Ok(device)
	}

	#[instrument(skip(self, keypair))]
	pub async fn ensure_device_registered(&self, keypair: &WgKeyPair) -> Result<DeviceInfo> {
		let devices = self.list_devices().await?;
		let public_key_b64 = keypair.public_key().to_base64();

		if let Some(device) = devices.into_iter().find(|d| d.public_key == public_key_b64) {
			return Ok(device);
		}

		let device_id = uuid::Uuid::new_v4().to_string();
		self
			.register_device(&device_id, keypair.public_key(), None)
			.await
	}

	#[instrument(skip(self))]
	pub async fn list_devices(&self) -> Result<Vec<DeviceInfo>> {
		let url = self.api_url("/api/wg/devices")?;

		let response = self
			.http
			.get(url)
			.bearer_auth(self.auth_token.expose())
			.send()
			.await?;

		if !response.status().is_success() {
			let status = response.status().as_u16();
			let message = response.text().await.unwrap_or_default();
			return Err(CliError::Api { status, message });
		}

		let devices = response.json().await?;
		Ok(devices)
	}

	#[instrument(skip(self))]
	pub async fn revoke_device(&self, device_id: &str) -> Result<()> {
		let url = self.api_url(&format!("/api/wg/devices/{}", device_id))?;

		let response = self
			.http
			.delete(url)
			.bearer_auth(self.auth_token.expose())
			.send()
			.await?;

		if !response.status().is_success() {
			let status = response.status().as_u16();
			let message = response.text().await.unwrap_or_default();
			return Err(CliError::Api { status, message });
		}

		Ok(())
	}

	#[instrument(skip(self))]
	pub async fn create_session(
		&self,
		weaver_id: &str,
		device_id: &str,
	) -> Result<CreateSessionResponse> {
		let url = self.api_url("/api/wg/sessions")?;

		let request = CreateSessionRequest {
			weaver_id,
			device_id,
		};

		let response = self
			.http
			.post(url)
			.bearer_auth(self.auth_token.expose())
			.json(&request)
			.send()
			.await?;

		if !response.status().is_success() {
			let status = response.status().as_u16();
			let message = response.text().await.unwrap_or_default();
			return Err(CliError::Api { status, message });
		}

		let session = response.json().await?;
		Ok(session)
	}

	#[instrument(skip(self))]
	pub async fn list_sessions(&self) -> Result<Vec<SessionInfo>> {
		let url = self.api_url("/api/wg/sessions")?;

		let response = self
			.http
			.get(url)
			.bearer_auth(self.auth_token.expose())
			.send()
			.await?;

		if !response.status().is_success() {
			let status = response.status().as_u16();
			let message = response.text().await.unwrap_or_default();
			return Err(CliError::Api { status, message });
		}

		let sessions = response.json().await?;
		Ok(sessions)
	}

	#[instrument(skip(self))]
	pub async fn delete_session(&self, session_id: &str) -> Result<()> {
		let url = self.api_url(&format!("/api/wg/sessions/{}", session_id))?;

		let response = self
			.http
			.delete(url)
			.bearer_auth(self.auth_token.expose())
			.send()
			.await?;

		if !response.status().is_success() {
			let status = response.status().as_u16();
			let message = response.text().await.unwrap_or_default();
			return Err(CliError::Api { status, message });
		}

		Ok(())
	}

	#[instrument(skip(self))]
	pub async fn get_derp_map(&self) -> Result<DerpMap> {
		let url = self.api_url("/api/wg/derp-map")?;

		let response = self
			.http
			.get(url)
			.bearer_auth(self.auth_token.expose())
			.send()
			.await?;

		if !response.status().is_success() {
			let status = response.status().as_u16();
			let message = response.text().await.unwrap_or_default();
			return Err(CliError::Api { status, message });
		}

		let derp_map = response.json().await?;
		Ok(derp_map)
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_device_info_deserialize() {
		let json = r#"{
            "id": "550e8400-e29b-41d4-a716-446655440000",
            "public_key": "dGVzdHB1YmxpY2tleQ",
            "name": "MacBook Pro",
            "created_at": "2026-01-03T12:00:00Z"
        }"#;

		let device: DeviceInfo = serde_json::from_str(json).unwrap();
		assert_eq!(device.id, "550e8400-e29b-41d4-a716-446655440000");
		assert_eq!(device.name, Some("MacBook Pro".to_string()));
	}

	#[test]
	fn test_session_info_deserialize() {
		let json = r#"{
            "session_id": "sess-xyz789",
            "client_ip": "fd7a:115c:a1e0:2::1",
            "weaver_ip": "fd7a:115c:a1e0:1::1",
            "weaver_public_key": "dGVzdHB1YmxpY2tleQ"
        }"#;

		let session: SessionInfo = serde_json::from_str(json).unwrap();
		assert_eq!(session.session_id, "sess-xyz789");
		assert_eq!(session.client_ip, "fd7a:115c:a1e0:2::1");
	}
}
