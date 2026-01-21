// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! XDG Base Directory compliant path resolution.

use std::path::PathBuf;

use crate::ConfigError;

/// Resolved XDG paths for Loom.
#[derive(Debug, Clone)]
pub struct PathsConfig {
	/// User config file: ~/.config/loom/config.toml
	pub user_config_file: PathBuf,
	/// System config file: /etc/loom/config.toml
	pub system_config_file: PathBuf,
	/// Data directory: ~/.local/share/loom/
	pub data_dir: PathBuf,
	/// Cache directory: ~/.cache/loom/
	pub cache_dir: PathBuf,
	/// State directory: ~/.local/state/loom/
	pub state_dir: PathBuf,
}

impl PathsConfig {
	/// Get the config directory (parent of user_config_file)
	pub fn config_dir(&self) -> PathBuf {
		self
			.user_config_file
			.parent()
			.map(|p| p.to_path_buf())
			.unwrap_or_else(|| self.user_config_file.clone())
	}
}

impl Default for PathsConfig {
	fn default() -> Self {
		Self {
			user_config_file: PathBuf::from("~/.config/loom/config.toml"),
			system_config_file: PathBuf::from("/etc/loom/config.toml"),
			data_dir: PathBuf::from("~/.local/share/loom"),
			cache_dir: PathBuf::from("~/.cache/loom"),
			state_dir: PathBuf::from("~/.local/state/loom"),
		}
	}
}

/// Resolve XDG paths according to the Base Directory Specification.
///
/// Uses environment variables if set, otherwise falls back to defaults:
/// - XDG_CONFIG_HOME or ~/.config
/// - XDG_DATA_HOME or ~/.local/share
/// - XDG_CACHE_HOME or ~/.cache
/// - XDG_STATE_HOME or ~/.local/state
pub fn resolve_xdg_paths() -> Result<PathsConfig, ConfigError> {
	let home = dirs::home_dir().ok_or(ConfigError::HomeDirNotFound)?;

	let config_home = std::env::var_os("XDG_CONFIG_HOME")
		.map(PathBuf::from)
		.unwrap_or_else(|| home.join(".config"));

	let data_home = std::env::var_os("XDG_DATA_HOME")
		.map(PathBuf::from)
		.unwrap_or_else(|| home.join(".local/share"));

	let cache_home = std::env::var_os("XDG_CACHE_HOME")
		.map(PathBuf::from)
		.unwrap_or_else(|| home.join(".cache"));

	let state_home = std::env::var_os("XDG_STATE_HOME")
		.map(PathBuf::from)
		.unwrap_or_else(|| home.join(".local/state"));

	tracing::debug!(
			config_home = %config_home.display(),
			data_home = %data_home.display(),
			cache_home = %cache_home.display(),
			state_home = %state_home.display(),
			"resolved XDG paths"
	);

	Ok(PathsConfig {
		user_config_file: config_home.join("loom/config.toml"),
		system_config_file: PathBuf::from("/etc/loom/config.toml"),
		data_dir: data_home.join("loom"),
		cache_dir: cache_home.join("loom"),
		state_dir: state_home.join("loom"),
	})
}

/// Get the workspace config file path from current directory.
pub fn workspace_config_path() -> Result<PathBuf, ConfigError> {
	let cwd = std::env::current_dir()?;
	Ok(cwd.join(".loom/config.toml"))
}

#[cfg(test)]
mod tests {
	use super::*;

	/// Test that XDG paths can be resolved without panicking.
	/// This test verifies the path resolution logic works in the test environment.
	#[test]
	fn test_resolve_xdg_paths_succeeds() {
		let result = resolve_xdg_paths();
		assert!(result.is_ok());

		let paths = result.unwrap();
		assert!(paths.user_config_file.to_string_lossy().contains("loom"));
		assert!(paths.data_dir.to_string_lossy().contains("loom"));
		assert!(paths.cache_dir.to_string_lossy().contains("loom"));
		assert!(paths.state_dir.to_string_lossy().contains("loom"));
	}

	/// Test that system config path is always /etc/loom/config.toml.
	#[test]
	fn test_system_config_is_etc() {
		let paths = resolve_xdg_paths().unwrap();
		assert_eq!(
			paths.system_config_file,
			PathBuf::from("/etc/loom/config.toml")
		);
	}

	/// Test config_dir() returns parent of config file.
	#[test]
	fn test_config_dir_returns_parent() {
		let paths = resolve_xdg_paths().unwrap();
		let config_dir = paths.config_dir();
		assert!(config_dir.to_string_lossy().ends_with("loom"));
	}

	/// Test workspace config path is relative to cwd.
	#[test]
	fn test_workspace_config_path() {
		let result = workspace_config_path();
		assert!(result.is_ok());
		assert!(result
			.unwrap()
			.to_string_lossy()
			.contains(".loom/config.toml"));
	}
}
