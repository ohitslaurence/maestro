// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! Shared build and version information for Loom (CLI + server).
//!
//! This crate provides a single source of truth for version, git SHA,
//! build timestamp, and platform information across all Loom binaries.

shadow_rs::shadow!(build);

#[cfg(feature = "serde")]
use serde::Serialize;
#[cfg(feature = "utoipa")]
use utoipa::ToSchema;

/// Platform string in `{os}-{arch}` format, e.g. "linux-x86_64".
///
/// Derived at compile time from target configuration.
pub const PLATFORM: &str = env!("LOOM_PLATFORM");

/// Core build information used across CLI, server, and headers.
#[derive(Debug, Clone, Copy)]
pub struct BuildInfo {
	pub version: &'static str,
	pub git_sha: &'static str,
	pub build_timestamp: &'static str,
	pub platform: &'static str,
}

impl BuildInfo {
	/// Get the current build information (compile-time constants).
	#[allow(clippy::const_is_empty)]
	pub const fn current() -> Self {
		Self {
			version: build::PKG_VERSION,
			git_sha: if build::SHORT_COMMIT.is_empty() {
				"unknown"
			} else {
				build::SHORT_COMMIT
			},
			build_timestamp: build::BUILD_TIME,
			platform: PLATFORM,
		}
	}
}

/// Version info shape used for health checks (matches health-check spec).
///
/// Contains only the git SHA for build identification.
#[cfg_attr(feature = "serde", derive(Serialize))]
#[cfg_attr(feature = "utoipa", derive(ToSchema))]
#[derive(Debug, Clone, Copy)]
pub struct HealthVersionInfo {
	pub git_sha: &'static str,
}

impl HealthVersionInfo {
	/// Get version info for health check responses.
	#[allow(clippy::const_is_empty)]
	pub const fn current() -> Self {
		let info = BuildInfo::current();
		Self {
			git_sha: if info.git_sha.is_empty()
				|| info.git_sha.as_bytes()[0] == b'u'
					&& info.git_sha.len() == 7
					&& info.git_sha.as_bytes()[1] == b'n'
			{
				"unknown"
			} else {
				info.git_sha
			},
		}
	}
}

/// HTTP header names for version information.
pub mod headers {
	pub const VERSION: &str = "X-Loom-Version";
	pub const GIT_SHA: &str = "X-Loom-Git-Sha";
	pub const BUILD_TIMESTAMP: &str = "X-Loom-Build-Timestamp";
	pub const PLATFORM: &str = "X-Loom-Platform";
}

/// Get the Loom version string for thread metadata.
pub const fn loom_version() -> &'static str {
	build::PKG_VERSION
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn build_info_has_version() {
		let info = BuildInfo::current();
		assert!(!info.version.is_empty());
	}

	#[test]
	fn platform_format_is_valid() {
		assert!(PLATFORM.contains('-'));
		let parts: Vec<&str> = PLATFORM.split('-').collect();
		assert_eq!(parts.len(), 2);
	}

	#[test]
	fn health_version_info_has_git_sha() {
		let info = HealthVersionInfo::current();
		assert!(!info.git_sha.is_empty());
	}

	#[test]
	fn loom_version_matches_build_info() {
		assert_eq!(loom_version(), BuildInfo::current().version);
	}
}
