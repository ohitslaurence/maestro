// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use crate::keys::{WgKeyPair, WgPrivateKey};
use std::path::Path;
use thiserror::Error;
use tokio::fs;
use tracing::instrument;

#[derive(Error, Debug)]
pub enum KeyFileError {
	#[error("failed to read key file: {0}")]
	Read(#[from] std::io::Error),

	#[error("invalid key format: {0}")]
	InvalidFormat(#[from] crate::keys::KeyError),

	#[error("failed to get home directory")]
	NoHomeDir,
}

pub type Result<T> = std::result::Result<T, KeyFileError>;

#[instrument(skip_all, fields(path = %path.as_ref().display()))]
pub async fn load_wg_key_from_file(path: impl AsRef<Path>) -> Result<WgKeyPair> {
	let content = fs::read_to_string(path.as_ref()).await?;
	let trimmed = content.trim();
	let private = WgPrivateKey::from_base64(trimmed)?;
	Ok(WgKeyPair::from_private_key(private))
}

#[instrument(skip(var_name), fields(var = %var_name))]
pub fn load_wg_key_env(var_name: &str) -> Result<Option<WgKeyPair>> {
	if let Ok(value) = std::env::var(var_name) {
		let private = WgPrivateKey::from_base64(value.trim())?;
		return Ok(Some(WgKeyPair::from_private_key(private)));
	}

	let file_var = format!("{}_FILE", var_name);
	if let Ok(path) = std::env::var(&file_var) {
		let content = std::fs::read_to_string(&path)?;
		let private = WgPrivateKey::from_base64(content.trim())?;
		return Ok(Some(WgKeyPair::from_private_key(private)));
	}

	Ok(None)
}

#[instrument(skip(key), fields(path = %path.as_ref().display()))]
pub async fn save_wg_key_to_file(key: &WgKeyPair, path: impl AsRef<Path>) -> Result<()> {
	let path = path.as_ref();

	if let Some(parent) = path.parent() {
		fs::create_dir_all(parent).await?;
	}

	let private_b64 = key.private_key().to_base64();
	let content = format!("{}\n", private_b64.expose());

	#[cfg(unix)]
	{
		use tokio::fs::OpenOptions;
		use tokio::io::AsyncWriteExt;

		let mut file = OpenOptions::new()
			.write(true)
			.create(true)
			.truncate(true)
			.mode(0o600)
			.open(path)
			.await?;
		file.write_all(content.as_bytes()).await?;
	}

	#[cfg(not(unix))]
	{
		fs::write(path, content).await?;
	}

	Ok(())
}

#[instrument(skip(config_dir))]
pub async fn get_or_create_device_key(config_dir: impl AsRef<Path>) -> Result<WgKeyPair> {
	let key_path = config_dir.as_ref().join("wg-key");

	if key_path.exists() {
		return load_wg_key_from_file(&key_path).await;
	}

	let keypair = WgKeyPair::generate();
	save_wg_key_to_file(&keypair, &key_path).await?;
	Ok(keypair)
}

pub fn default_config_dir() -> Result<std::path::PathBuf> {
	dirs::home_dir()
		.map(|h| h.join(".loom"))
		.ok_or(KeyFileError::NoHomeDir)
}

#[cfg(test)]
mod tests {
	use super::*;
	use tempfile::TempDir;

	#[tokio::test]
	async fn save_and_load_key() {
		let temp_dir = TempDir::new().unwrap();
		let key_path = temp_dir.path().join("wg-key");

		let keypair = WgKeyPair::generate();
		save_wg_key_to_file(&keypair, &key_path).await.unwrap();

		let loaded = load_wg_key_from_file(&key_path).await.unwrap();
		assert_eq!(keypair.public_key(), loaded.public_key());
	}

	#[tokio::test]
	#[cfg(unix)]
	async fn save_key_sets_permissions() {
		use std::os::unix::fs::PermissionsExt;

		let temp_dir = TempDir::new().unwrap();
		let key_path = temp_dir.path().join("wg-key");

		let keypair = WgKeyPair::generate();
		save_wg_key_to_file(&keypair, &key_path).await.unwrap();

		let metadata = std::fs::metadata(&key_path).unwrap();
		let mode = metadata.permissions().mode() & 0o777;
		assert_eq!(mode, 0o600);
	}

	#[tokio::test]
	async fn get_or_create_generates_new_key() {
		let temp_dir = TempDir::new().unwrap();

		let keypair = get_or_create_device_key(temp_dir.path()).await.unwrap();
		let key_path = temp_dir.path().join("wg-key");
		assert!(key_path.exists());

		let loaded = load_wg_key_from_file(&key_path).await.unwrap();
		assert_eq!(keypair.public_key(), loaded.public_key());
	}

	#[tokio::test]
	async fn get_or_create_reuses_existing_key() {
		let temp_dir = TempDir::new().unwrap();

		let keypair1 = get_or_create_device_key(temp_dir.path()).await.unwrap();
		let keypair2 = get_or_create_device_key(temp_dir.path()).await.unwrap();

		assert_eq!(keypair1.public_key(), keypair2.public_key());
	}

	#[test]
	fn load_wg_key_env_returns_none_when_unset() {
		let result = load_wg_key_env("NONEXISTENT_WG_KEY_VAR").unwrap();
		assert!(result.is_none());
	}
}
