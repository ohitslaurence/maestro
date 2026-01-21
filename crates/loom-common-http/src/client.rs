// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights
// reserved. SPDX-License-Identifier: Proprietary

//! Shared HTTP client with consistent User-Agent header.

use loom_common_version::BuildInfo;
use reqwest::{Client, ClientBuilder};
use std::time::Duration;

/// Creates a new HTTP client with the standard Loom User-Agent header.
///
/// The User-Agent format is: `loom/{platform}/{git_sha}`
/// Example: `loom/linux-x86_64/abc1234`
pub fn new_client() -> Client {
	builder().build().expect("failed to build HTTP client")
}

/// Creates a new HTTP client builder with the standard Loom User-Agent header.
///
/// Use this when you need to customize the client (e.g., set timeout).
///
/// # Example
/// ```ignore
/// let client = loom_common_http::builder()
///     .timeout(Duration::from_secs(30))
///     .build()?;
/// ```
pub fn builder() -> ClientBuilder {
	Client::builder().user_agent(user_agent())
}

/// Creates a new HTTP client builder with a custom User-Agent header.
///
/// Use this when you need to impersonate a specific client (e.g., Claude CLI).
///
/// # Example
/// ```ignore
/// let client = loom_common_http::builder_with_user_agent("claude-cli/2.0.76 (external, sdk-cli)")
///     .timeout(Duration::from_secs(30))
///     .build()?;
/// ```
pub fn builder_with_user_agent(user_agent: impl Into<String>) -> ClientBuilder {
	Client::builder().user_agent(user_agent.into())
}

/// Creates a new HTTP client with a custom timeout and the standard User-Agent.
pub fn new_client_with_timeout(timeout: Duration) -> Client {
	builder()
		.timeout(timeout)
		.build()
		.expect("failed to build HTTP client")
}

/// Creates a new HTTP client with a custom User-Agent and timeout.
pub fn new_client_with_user_agent(user_agent: impl Into<String>) -> Client {
	builder_with_user_agent(user_agent)
		.build()
		.expect("failed to build HTTP client")
}

/// Returns the standard Loom User-Agent string.
///
/// Format: `loom/{platform}/{git_sha}`
pub fn user_agent() -> String {
	let info = BuildInfo::current();
	format!("loom/{}/{}", info.platform, info.git_sha)
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn user_agent_has_correct_format() {
		let ua = user_agent();
		assert!(ua.starts_with("loom/"));
		let parts: Vec<&str> = ua.split('/').collect();
		assert_eq!(parts.len(), 3);
		assert_eq!(parts[0], "loom");
	}

	#[test]
	fn builder_with_custom_user_agent() {
		let custom_ua = "my-custom-agent/1.0";
		let client = builder_with_user_agent(custom_ua).build();
		assert!(client.is_ok());
	}
}
