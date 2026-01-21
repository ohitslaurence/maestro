// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use std::io::{self, Write};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use anyhow::{anyhow, Context, Result};
use serde::Deserialize;
use tracing::{debug, info, instrument, warn};

use loom_cli_credentials::{CredentialStore, CredentialValue, KeyringThenFileStore};
use loom_common_secret::SecretString;

use crate::locale::get_locale;

#[derive(Deserialize)]
struct DeviceStartResponse {
	device_code: String,
	user_code: String,
	verification_url: String,
	expires_in: i64,
}

#[derive(Deserialize)]
#[serde(tag = "status")]
enum DevicePollResponse {
	#[serde(rename = "pending")]
	Pending,
	#[serde(rename = "completed")]
	Completed { access_token: String },
	#[serde(rename = "expired")]
	Expired,
}

pub fn credential_store_path() -> PathBuf {
	dirs::config_dir()
		.unwrap_or_else(|| PathBuf::from("."))
		.join("loom")
		.join("credentials.json")
}

fn normalize_base(server_url: &str) -> String {
	server_url.trim_end_matches('/').to_string()
}

fn sanitize_server_key(server_url: &str) -> String {
	server_url
		.chars()
		.map(|c| if c.is_alphanumeric() { c } else { '_' })
		.collect()
}

fn get_credential_store() -> KeyringThenFileStore {
	KeyringThenFileStore::new("loom", credential_store_path())
}

#[instrument(skip_all, fields(server_url = %server_url))]
pub async fn login(server_url: &str) -> Result<()> {
	let client = loom_common_http::new_client();
	let base = normalize_base(server_url);
	let start_url = format!("{base}/auth/device/start");

	debug!("initiating device code flow");
	let resp = client
		.post(&start_url)
		.send()
		.await
		.context("failed to start device auth")?;

	if !resp.status().is_success() {
		let status = resp.status();
		let body = resp.text().await.unwrap_or_default();
		warn!(status = %status, "device start failed");
		return Err(anyhow!("device start failed: {status} - {body}"));
	}

	let start: DeviceStartResponse = resp.json().await.context("invalid device start response")?;
	debug!(
		verification_url = %start.verification_url,
		user_code = %start.user_code,
		expires_in = start.expires_in,
		"device code flow started"
	);

	eprintln!(
		"\n{}\n",
		loom_common_i18n::t_fmt(
			get_locale(),
			"client.auth.visit_url",
			&[("url", &start.verification_url), ("code", &start.user_code)]
		)
	);

	if let Err(e) = webbrowser::open(&start.verification_url) {
		debug!(error = %e, "failed to open browser");
		eprintln!(
			"{}",
			loom_common_i18n::t(get_locale(), "client.auth.browser_failed")
		);
	}

	let poll_url = format!("{base}/auth/device/poll");
	let timeout = Duration::from_secs(start.expires_in.max(0) as u64);
	let poll_interval = Duration::from_secs(1);
	let started = Instant::now();

	eprint!(
		"{}",
		loom_common_i18n::t(get_locale(), "client.auth.waiting")
	);
	io::stderr().flush().ok();

	loop {
		if started.elapsed() > timeout {
			eprintln!(
				"\n{}",
				loom_common_i18n::t(get_locale(), "client.auth.timed_out")
			);
			return Err(anyhow!("device code expired"));
		}

		tokio::select! {
			_ = tokio::signal::ctrl_c() => {
				eprintln!("\n{}", loom_common_i18n::t(get_locale(), "client.auth.cancelled"));
				return Err(anyhow!("login cancelled by user"));
			}
			_ = tokio::time::sleep(poll_interval) => {}
		}

		let resp = client
			.post(&poll_url)
			.json(&serde_json::json!({ "device_code": start.device_code }))
			.send()
			.await
			.context("failed to poll device auth")?;

		if !resp.status().is_success() {
			let status = resp.status();
			if status.is_client_error() && status != reqwest::StatusCode::TOO_MANY_REQUESTS {
				let body = resp.text().await.unwrap_or_default();
				eprintln!();
				warn!(status = %status, "device auth failed");
				return Err(anyhow!("device auth failed: {status} - {body}"));
			}
			eprint!(".");
			io::stderr().flush().ok();
			continue;
		}

		let poll: DevicePollResponse = match resp.json().await {
			Ok(p) => p,
			Err(_) => {
				eprint!(".");
				io::stderr().flush().ok();
				continue;
			}
		};

		match poll {
			DevicePollResponse::Pending => {
				eprint!(".");
				io::stderr().flush().ok();
			}
			DevicePollResponse::Completed { access_token } => {
				eprintln!();
				let store = get_credential_store();
				let key = sanitize_server_key(server_url);
				let creds = CredentialValue::ApiKey {
					key: SecretString::new(access_token),
				};
				store
					.save(&key, &creds)
					.await
					.context("failed to save credentials")?;
				info!("login successful");
				eprintln!(
					"{}",
					loom_common_i18n::t_fmt(
						get_locale(),
						"client.auth.login_success",
						&[("server", server_url)]
					)
				);
				return Ok(());
			}
			DevicePollResponse::Expired => {
				eprintln!(
					"\n{}",
					loom_common_i18n::t(get_locale(), "client.auth.device_expired")
				);
				return Err(anyhow!("device code expired"));
			}
		}
	}
}

#[instrument(skip_all, fields(server_url = %server_url))]
pub async fn logout(server_url: &str) -> Result<()> {
	let store = get_credential_store();
	let key = sanitize_server_key(server_url);
	let base = normalize_base(server_url);

	if let Some(CredentialValue::ApiKey { key: token }) = store
		.load(&key)
		.await
		.context("failed to load credentials")?
	{
		let client = loom_common_http::new_client();
		let logout_url = format!("{base}/auth/logout");
		debug!("calling logout endpoint");
		let _ = client
			.post(&logout_url)
			.bearer_auth(token.expose())
			.send()
			.await;
	}

	store
		.delete(&sanitize_server_key(server_url))
		.await
		.context("failed to delete credentials")?;
	info!("logout complete");
	eprintln!(
		"{}",
		loom_common_i18n::t_fmt(
			get_locale(),
			"client.auth.logged_out",
			&[("server", server_url)]
		)
	);
	Ok(())
}

pub async fn load_token(server_url: &str) -> Option<SecretString> {
	let store = get_credential_store();
	let key = sanitize_server_key(server_url);

	match store.load(&key).await {
		Ok(Some(CredentialValue::ApiKey { key })) => Some(key),
		Ok(Some(CredentialValue::OAuth { access, .. })) => Some(access),
		Ok(None) => None,
		Err(e) => {
			warn!(server_url = %server_url, error = %e, "failed to load credentials");
			None
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_sanitize_server_key_alphanumeric() {
		assert_eq!(sanitize_server_key("abc123"), "abc123");
	}

	#[test]
	fn test_sanitize_server_key_special_chars() {
		assert_eq!(
			sanitize_server_key("https://example.com:8080/path"),
			"https___example_com_8080_path"
		);
	}

	#[test]
	fn test_sanitize_server_key_preserves_uniqueness() {
		let key1 = sanitize_server_key("https://prod.loom.io");
		let key2 = sanitize_server_key("https://staging.loom.io");
		assert_ne!(key1, key2);
	}

	#[test]
	fn test_normalize_base_strips_trailing_slash() {
		assert_eq!(
			normalize_base("https://example.com/"),
			"https://example.com"
		);
		assert_eq!(normalize_base("https://example.com"), "https://example.com");
	}

	#[test]
	fn test_normalize_base_strips_multiple_slashes() {
		assert_eq!(
			normalize_base("https://example.com///"),
			"https://example.com"
		);
	}

	#[test]
	fn test_credential_store_path_has_loom_dir() {
		let path = credential_store_path();
		assert!(path.ends_with("loom/credentials.json") || path.ends_with("loom\\credentials.json"));
	}
}

#[cfg(test)]
mod proptests {
	use super::*;
	use proptest::prelude::*;

	proptest! {
		#[test]
		fn sanitize_key_never_empty_for_nonempty_input(s in ".+") {
			let key = sanitize_server_key(&s);
			prop_assert!(!key.is_empty());
		}

		#[test]
		fn sanitize_key_only_contains_alphanumeric_or_underscore(s in ".*") {
			let key = sanitize_server_key(&s);
			prop_assert!(key.chars().all(|c| c.is_alphanumeric() || c == '_'));
		}

		#[test]
		fn normalize_base_never_ends_with_slash(s in "https?://[a-z]+\\.[a-z]+/*") {
			let normalized = normalize_base(&s);
			prop_assert!(!normalized.ends_with('/'));
		}
	}
}
