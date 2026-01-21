// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! Build information and version utilities.

pub use loom_common_version::BuildInfo;

/// Get the current build information.
pub fn build_info() -> BuildInfo {
	BuildInfo::current()
}

/// Format version info for display.
pub fn format_version_info() -> String {
	let info = BuildInfo::current();

	format!(
		"Git SHA:  {}\n\
         Platform: {}",
		info.git_sha, info.platform,
	)
}
