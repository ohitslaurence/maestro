// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights
// reserved. SPDX-License-Identifier: Proprietary

//! Build information and version utilities for loom-server.

pub use loom_common_version::BuildInfo;

/// Get the current build information.
#[allow(dead_code)]
pub fn build_info() -> BuildInfo {
	BuildInfo::current()
}

/// Format version info for display.
pub fn format_version_info() -> String {
	use chrono::{DateTime, Utc};

	let info = BuildInfo::current();

	let mut output = format!(
		"loom-server version: {}\n\
         Git SHA:             {}\n\
         Built at:            {}\n\
         Platform:            {}",
		info.version, info.git_sha, info.build_timestamp, info.platform,
	);

	// Try to parse build time and calculate age
	if let Ok(built_at) = DateTime::parse_from_rfc3339(info.build_timestamp)
		.or_else(|_| DateTime::parse_from_str(info.build_timestamp, "%Y-%m-%d %H:%M:%S"))
	{
		let built_at_utc: DateTime<Utc> = built_at.into();
		let now = Utc::now();
		let age = now.signed_duration_since(built_at_utc);

		if let Ok(std_duration) = age.to_std() {
			output.push_str(&format!(
				"\nBuild age:         {} ({} seconds)",
				humantime::format_duration(std_duration),
				std_duration.as_secs()
			));
		}
	}

	output
}
