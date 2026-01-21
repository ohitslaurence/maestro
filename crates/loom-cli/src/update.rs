// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use anyhow::{Context, Result};
use sha2::{Digest, Sha256};
use tracing::{debug, info, instrument, warn};
use url::Url;

use crate::locale::get_locale;
use crate::version;

pub fn get_update_base_url() -> Result<Url> {
	if let Ok(raw) = std::env::var("LOOM_UPDATE_BASE_URL") {
		debug!(url = %raw, "using LOOM_UPDATE_BASE_URL");
		return Url::parse(&raw).context("invalid LOOM_UPDATE_BASE_URL");
	}
	if let Ok(sync_url) = std::env::var("LOOM_THREAD_SYNC_URL") {
		debug!(url = %sync_url, "deriving base URL from LOOM_THREAD_SYNC_URL");
		let mut base = Url::parse(&sync_url).context("invalid LOOM_THREAD_SYNC_URL")?;
		base.set_path("");
		return Ok(base);
	}
	anyhow::bail!(
		"{}",
		loom_common_i18n::t(get_locale(), "client.update.error.env_not_set")
	)
}

pub fn compute_sha256(data: &[u8]) -> String {
	hex::encode(Sha256::digest(data))
}

pub fn build_sha_url(base_url: &Url, platform: &str) -> Result<Url> {
	base_url
		.join(&format!("bin/{platform}.sha256"))
		.context("failed to construct SHA URL")
}

pub fn build_bin_url(base_url: &Url, platform: &str) -> Result<Url> {
	base_url
		.join(&format!("bin/{platform}"))
		.context("failed to construct binary URL")
}

pub fn normalize_remote_sha(raw: &str) -> String {
	raw.trim().to_lowercase()
}

pub fn needs_update(current_sha: &str, remote_sha: &str) -> bool {
	current_sha != remote_sha
}

pub fn verify_download(downloaded_bytes: &[u8], expected_sha: &str) -> Result<()> {
	if downloaded_bytes.is_empty() {
		anyhow::bail!(
			"{}",
			loom_common_i18n::t(get_locale(), "client.update.error.empty_binary")
		);
	}

	let downloaded_sha = compute_sha256(downloaded_bytes);
	if downloaded_sha != expected_sha {
		anyhow::bail!(
			"{}",
			loom_common_i18n::t_fmt(
				get_locale(),
				"client.update.error.sha_mismatch",
				&[
					("expected", &expected_sha[..12.min(expected_sha.len())]),
					("actual", &downloaded_sha[..12])
				]
			)
		);
	}

	Ok(())
}

#[instrument(skip_all, fields(platform))]
pub async fn run_update() -> Result<()> {
	let build_info = version::build_info();
	let base_url = get_update_base_url()?;

	let platform = &build_info.platform;
	tracing::Span::current().record("platform", platform);

	let sha_url = build_sha_url(&base_url, platform)?;
	let bin_url = build_bin_url(&base_url, platform)?;

	println!(
		"{}",
		loom_common_i18n::t_fmt(
			get_locale(),
			"client.update.current_version",
			&[("version", build_info.version)]
		)
	);
	println!(
		"{}",
		loom_common_i18n::t_fmt(
			get_locale(),
			"client.update.platform",
			&[("platform", platform)]
		)
	);
	println!(
		"{}",
		loom_common_i18n::t(get_locale(), "client.update.checking")
	);

	let current_exe = std::env::current_exe().context("failed to get current executable path")?;
	debug!(path = %current_exe.display(), "current executable");

	let current_bytes = std::fs::read(&current_exe).context("failed to read current executable")?;
	let current_sha = compute_sha256(&current_bytes);
	debug!(sha = %current_sha, "current executable SHA");

	let http_client = loom_common_http::new_client();

	info!(url = %sha_url, "fetching remote SHA");
	let sha_response = http_client
		.get(sha_url.clone())
		.send()
		.await
		.context("failed to fetch remote SHA")?;

	if !sha_response.status().is_success() {
		let status = sha_response.status();
		warn!(status = %status, "failed to check for updates");
		anyhow::bail!(
			"{}",
			loom_common_i18n::t_fmt(
				get_locale(),
				"client.update.error.check_failed",
				&[("status", &status.to_string())]
			)
		);
	}

	let remote_sha = normalize_remote_sha(
		&sha_response
			.text()
			.await
			.context("failed to read remote SHA")?,
	);
	debug!(sha = %remote_sha, "remote SHA");

	if !needs_update(&current_sha, &remote_sha) {
		info!(sha = %current_sha, "already up to date");
		println!(
			"{}",
			loom_common_i18n::t_fmt(
				get_locale(),
				"client.update.up_to_date",
				&[("sha", &current_sha[..12])]
			)
		);
		return Ok(());
	}

	info!(
		current_sha = %&current_sha[..12],
		remote_sha = %&remote_sha[..12],
		"update available"
	);
	println!(
		"{}",
		loom_common_i18n::t_fmt(
			get_locale(),
			"client.update.available",
			&[
				("current", &current_sha[..12]),
				("remote", &remote_sha[..12])
			]
		)
	);
	println!(
		"{}",
		loom_common_i18n::t_fmt(
			get_locale(),
			"client.update.downloading",
			&[("url", bin_url.as_str())]
		)
	);

	info!(url = %bin_url, "downloading update");
	let response = http_client
		.get(bin_url.clone())
		.send()
		.await
		.context("failed to download update")?;

	if !response.status().is_success() {
		let status = response.status();
		warn!(status = %status, "download failed");
		anyhow::bail!(
			"Update server returned error: {} - {}",
			status,
			response.text().await.unwrap_or_default()
		);
	}

	let bytes = response
		.bytes()
		.await
		.context("failed to read update binary")?;

	verify_download(&bytes, &remote_sha)?;

	info!(bytes = bytes.len(), "download verified");
	println!(
		"{}",
		loom_common_i18n::t_fmt(
			get_locale(),
			"client.update.downloaded",
			&[("bytes", &bytes.len().to_string())]
		)
	);

	let tmp_path = current_exe.with_extension("new");
	debug!(path = %tmp_path.display(), "writing temporary binary");
	tokio::fs::write(&tmp_path, &bytes)
		.await
		.context("failed to write temporary binary")?;

	#[cfg(unix)]
	{
		use std::os::unix::fs::PermissionsExt;
		let mut perms = std::fs::metadata(&tmp_path)?.permissions();
		perms.set_mode(0o755);
		std::fs::set_permissions(&tmp_path, perms)?;
	}

	info!("replacing binary");
	self_replace::self_replace(&tmp_path).context("failed to replace binary")?;

	let _ = std::fs::remove_file(&tmp_path);

	info!("update complete");
	println!(
		"{}",
		loom_common_i18n::t(get_locale(), "client.update.complete")
	);

	Ok(())
}

#[cfg(test)]
mod tests {
	use super::*;
	use proptest::prelude::*;

	proptest! {
		#[test]
		fn prop_sha256_deterministic(data: Vec<u8>) {
			let sha1 = compute_sha256(&data);
			let sha2 = compute_sha256(&data);
			prop_assert_eq!(sha1, sha2);
		}

		#[test]
		fn prop_sha256_length(data: Vec<u8>) {
			let sha = compute_sha256(&data);
			prop_assert_eq!(sha.len(), 64);
		}

		#[test]
		fn prop_sha256_hex_chars(data: Vec<u8>) {
			let sha = compute_sha256(&data);
			prop_assert!(sha.chars().all(|c| c.is_ascii_hexdigit()));
		}

		#[test]
		fn prop_normalize_sha_idempotent(s in "[a-fA-F0-9 \n\t]{0,100}") {
			let normalized = normalize_remote_sha(&s);
			let twice = normalize_remote_sha(&normalized);
			prop_assert_eq!(normalized, twice);
		}

		#[test]
		fn prop_verify_download_correct_sha(data in proptest::collection::vec(any::<u8>(), 1..1000)) {
			let sha = compute_sha256(&data);
			prop_assert!(verify_download(&data, &sha).is_ok());
		}
	}

	#[test]
	fn test_compute_sha256() {
		let data = b"hello world";
		let sha = compute_sha256(data);
		assert_eq!(
			sha,
			"b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9"
		);
	}

	#[test]
	fn test_compute_sha256_empty() {
		let sha = compute_sha256(b"");
		assert_eq!(
			sha,
			"e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
		);
	}

	#[test]
	fn test_normalize_remote_sha() {
		assert_eq!(normalize_remote_sha("  ABC123DEF  \n"), "abc123def");
		assert_eq!(normalize_remote_sha("abc123def"), "abc123def");
	}

	#[test]
	fn test_needs_update_same() {
		assert!(!needs_update("abc123", "abc123"));
	}

	#[test]
	fn test_needs_update_different() {
		assert!(needs_update("abc123", "def456"));
	}

	#[test]
	fn test_build_sha_url() {
		let base = Url::parse("https://example.com/").unwrap();
		let url = build_sha_url(&base, "linux-x86_64").unwrap();
		assert_eq!(url.as_str(), "https://example.com/bin/linux-x86_64.sha256");
	}

	#[test]
	fn test_build_bin_url() {
		let base = Url::parse("https://example.com/").unwrap();
		let url = build_bin_url(&base, "macos-aarch64").unwrap();
		assert_eq!(url.as_str(), "https://example.com/bin/macos-aarch64");
	}

	#[test]
	fn test_verify_download_success() {
		let data = b"test binary content";
		let sha = compute_sha256(data);
		assert!(verify_download(data, &sha).is_ok());
	}

	#[test]
	fn test_verify_download_empty() {
		let result = verify_download(b"", "abc123");
		assert!(result.is_err());
	}

	#[test]
	fn test_verify_download_mismatch() {
		let data = b"test binary content";
		let result = verify_download(
			data,
			"0000000000000000000000000000000000000000000000000000000000000000",
		);
		assert!(result.is_err());
	}
}
