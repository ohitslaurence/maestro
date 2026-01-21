// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use loom_wgtunnel_common::WgKeyPair;
use std::path::Path;
use tracing::instrument;

pub const KEY_FILENAME: &str = "wg-key";

#[instrument(skip(config_dir))]
pub async fn get_or_create_device_key(config_dir: &Path) -> anyhow::Result<WgKeyPair> {
	let key = loom_wgtunnel_common::get_or_create_device_key(config_dir).await?;
	Ok(key)
}

#[instrument(skip_all, fields(path = %path.as_ref().display()))]
pub async fn load_key_file(path: impl AsRef<Path>) -> anyhow::Result<WgKeyPair> {
	let key = loom_wgtunnel_common::load_wg_key_from_file(path).await?;
	Ok(key)
}

#[instrument(skip(keypair), fields(path = %path.as_ref().display()))]
pub async fn save_key_file(keypair: &WgKeyPair, path: impl AsRef<Path>) -> anyhow::Result<()> {
	loom_wgtunnel_common::save_wg_key_to_file(keypair, path).await?;
	Ok(())
}

pub fn key_exists(config_dir: &Path) -> bool {
	config_dir.join(KEY_FILENAME).exists()
}

#[cfg(test)]
mod tests {
	use super::*;
	use tempfile::TempDir;

	#[tokio::test]
	async fn test_get_or_create_device_key() {
		let temp_dir = TempDir::new().unwrap();

		let key1 = get_or_create_device_key(temp_dir.path()).await.unwrap();
		let key2 = get_or_create_device_key(temp_dir.path()).await.unwrap();

		assert_eq!(key1.public_key(), key2.public_key());
	}

	#[test]
	fn test_key_exists() {
		let temp_dir = TempDir::new().unwrap();

		assert!(!key_exists(temp_dir.path()));

		std::fs::write(temp_dir.path().join(KEY_FILENAME), "test").unwrap();
		assert!(key_exists(temp_dir.path()));
	}
}
