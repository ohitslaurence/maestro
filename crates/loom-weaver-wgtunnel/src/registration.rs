// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use crate::error::RegistrationError;
use loom_common_secret::SecretString;
use loom_weaver_secrets::SecretsClient;
use loom_wgtunnel_common::WgPublicKey;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::net::Ipv6Addr;
use tracing::{debug, info, instrument};
use url::Url;

pub struct Registration {
	server_url: Url,
	http_client: Client,
	weaver_id: String,
	svid: Option<SecretString>,
	assigned_ip: Option<Ipv6Addr>,
}

#[derive(Debug, Serialize)]
struct RegisterRequest {
	pub public_key: String,
	pub derp_home_region: Option<u16>,
}

#[derive(Debug, Deserialize)]
pub struct RegistrationResponse {
	pub assigned_ip: String,
	pub derp_map: serde_json::Value,
}

impl Registration {
	pub fn new(server_url: Url, weaver_id: String) -> Result<Self, RegistrationError> {
		let http_client = loom_common_http::new_client();

		Ok(Self {
			server_url,
			http_client,
			weaver_id,
			svid: None,
			assigned_ip: None,
		})
	}

	#[instrument(skip(self))]
	pub async fn get_svid(&mut self) -> Result<SecretString, RegistrationError> {
		let secrets_client = SecretsClient::new()?;
		let svid = secrets_client.get_svid().await?;
		self.svid = Some(svid.clone());
		Ok(svid)
	}

	#[instrument(skip(self, public_key), fields(weaver_id = %self.weaver_id))]
	pub async fn register(
		&mut self,
		public_key: &WgPublicKey,
		derp_region: Option<u16>,
	) -> Result<RegistrationResponse, RegistrationError> {
		let svid = self.svid.as_ref().ok_or(RegistrationError::NoSvid)?;

		let url = self.server_url.join("/internal/wg/weavers")?;

		debug!(%url, "registering weaver WG key with server");

		let resp = self
			.http_client
			.post(url)
			.header("Authorization", format!("Bearer {}", svid.expose()))
			.json(&RegisterRequest {
				public_key: public_key.to_base64(),
				derp_home_region: derp_region,
			})
			.send()
			.await?
			.error_for_status()?
			.json::<RegistrationResponse>()
			.await?;

		self.assigned_ip = Some(resp.assigned_ip.parse()?);

		info!(assigned_ip = %resp.assigned_ip, "weaver registered with server");

		Ok(resp)
	}

	#[instrument(skip(self), fields(weaver_id = %self.weaver_id))]
	pub async fn unregister(&self) -> Result<(), RegistrationError> {
		let svid = self.svid.as_ref().ok_or(RegistrationError::NoSvid)?;

		let url = self
			.server_url
			.join(&format!("/internal/wg/weavers/{}", self.weaver_id))?;

		debug!(%url, "unregistering weaver from server");

		self
			.http_client
			.delete(url)
			.header("Authorization", format!("Bearer {}", svid.expose()))
			.send()
			.await?
			.error_for_status()?;

		info!("weaver unregistered from server");

		Ok(())
	}

	#[instrument(skip(self), fields(weaver_id = %self.weaver_id))]
	pub async fn heartbeat(&self) -> Result<(), RegistrationError> {
		let svid = self.svid.as_ref().ok_or(RegistrationError::NoSvid)?;

		let url = self.server_url.join(&format!(
			"/internal/wg/weavers/{}/heartbeat",
			self.weaver_id
		))?;

		self
			.http_client
			.post(url)
			.header("Authorization", format!("Bearer {}", svid.expose()))
			.send()
			.await?
			.error_for_status()?;

		debug!("heartbeat sent");

		Ok(())
	}

	pub fn assigned_ip(&self) -> Option<Ipv6Addr> {
		self.assigned_ip
	}

	pub fn weaver_id(&self) -> &str {
		&self.weaver_id
	}

	pub fn svid(&self) -> Option<&SecretString> {
		self.svid.as_ref()
	}
}

impl std::fmt::Debug for Registration {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.debug_struct("Registration")
			.field("server_url", &self.server_url)
			.field("weaver_id", &self.weaver_id)
			.field("has_svid", &self.svid.is_some())
			.field("assigned_ip", &self.assigned_ip)
			.finish()
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_registration_new() {
		let reg = Registration::new(
			"https://loom.example.com".parse().unwrap(),
			"weaver-123".to_string(),
		)
		.unwrap();
		assert_eq!(reg.weaver_id(), "weaver-123");
		assert!(reg.svid().is_none());
		assert!(reg.assigned_ip().is_none());
	}

	#[test]
	fn test_registration_debug_does_not_leak() {
		let reg = Registration::new(
			"https://loom.example.com".parse().unwrap(),
			"weaver-123".to_string(),
		)
		.unwrap();
		let debug = format!("{:?}", reg);
		assert!(debug.contains("has_svid"));
		assert!(!debug.contains("Bearer"));
	}
}
