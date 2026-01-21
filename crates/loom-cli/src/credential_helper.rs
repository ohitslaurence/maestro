// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! Git credential helper for Loom SCM authentication.
//!
//! This module implements a git credential helper that provides stored session
//! tokens from `loom login` to git operations.
//!
//! # Setup
//!
//! Configure git to use this helper for Loom repositories:
//!
//! ```bash
//! git config --global credential.https://loom.ghuntley.com.helper 'loom credential-helper'
//! ```
//!
//! # Protocol
//!
//! Git calls the helper with one of three operations:
//! - `get`: Return credentials for the given host
//! - `store`: Store credentials (no-op, we use `loom login`)
//! - `erase`: Erase credentials (no-op)
//!
//! Input format (on stdin):
//! ```text
//! protocol=https
//! host=loom.ghuntley.com
//! ```
//!
//! Output format (on stdout for `get`):
//! ```text
//! username=oauth2
//! password={session_token}
//! ```

use std::collections::HashMap;
use std::io::{self, BufRead, Write};

use anyhow::{anyhow, Result};
use tracing::{debug, instrument};

use crate::auth;

/// Git credential helper arguments
#[derive(Debug, Clone, clap::Args)]
pub struct CredentialHelperArgs {
	/// The operation: get, store, or erase
	pub operation: String,

	/// Server URL to match credentials against
	#[arg(
		long,
		env = "LOOM_SERVER_URL",
		default_value = "https://loom.ghuntley.com"
	)]
	pub server_url: String,
}

#[instrument(skip_all, fields(operation = %args.operation))]
pub async fn run(args: CredentialHelperArgs) -> Result<()> {
	match args.operation.as_str() {
		"get" => handle_get(&args.server_url).await,
		"store" | "erase" => {
			debug!(
				"ignoring {} operation (credentials managed via loom login)",
				args.operation
			);
			Ok(())
		}
		_ => Err(anyhow!("unknown operation: {}", args.operation)),
	}
}

async fn handle_get(server_url: &str) -> Result<()> {
	let input = read_stdin()?;
	let params = parse_credential_input(&input);

	let protocol = params.get("protocol").map(|s| s.as_str()).unwrap_or("");
	let host = params.get("host").map(|s| s.as_str()).unwrap_or("");

	debug!(protocol = %protocol, host = %host, "credential get request");

	if protocol != "https" {
		debug!("ignoring non-https protocol");
		return Ok(());
	}

	if !is_matching_host(server_url, host) {
		debug!(server_url = %server_url, host = %host, "host does not match server URL");
		return Ok(());
	}

	let token = auth::load_token(server_url).await;

	match token {
		Some(secret) => {
			debug!("returning stored credentials");
			let mut stdout = io::stdout().lock();
			writeln!(stdout, "username=oauth2")?;
			writeln!(stdout, "password={}", secret.expose())?;
			stdout.flush()?;
			Ok(())
		}
		None => {
			debug!("no credentials found, run 'loom login' first");
			Ok(())
		}
	}
}

fn read_stdin() -> Result<String> {
	let stdin = io::stdin();
	let mut input = String::new();
	for line in stdin.lock().lines() {
		let line = line?;
		if line.is_empty() {
			break;
		}
		input.push_str(&line);
		input.push('\n');
	}
	Ok(input)
}

fn parse_credential_input(input: &str) -> HashMap<String, String> {
	input
		.lines()
		.filter_map(|line| {
			let parts: Vec<&str> = line.splitn(2, '=').collect();
			if parts.len() == 2 {
				Some((parts[0].to_string(), parts[1].to_string()))
			} else {
				None
			}
		})
		.collect()
}

fn is_matching_host(server_url: &str, host: &str) -> bool {
	if let Ok(url) = url::Url::parse(server_url) {
		if let Some(server_host) = url.host_str() {
			return server_host == host;
		}
	}
	false
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_parse_credential_input_basic() {
		let input = "protocol=https\nhost=loom.ghuntley.com\n";
		let params = parse_credential_input(input);
		assert_eq!(params.get("protocol"), Some(&"https".to_string()));
		assert_eq!(params.get("host"), Some(&"loom.ghuntley.com".to_string()));
	}

	#[test]
	fn test_parse_credential_input_with_path() {
		let input = "protocol=https\nhost=loom.ghuntley.com\npath=git/owner/repo.git\n";
		let params = parse_credential_input(input);
		assert_eq!(params.get("path"), Some(&"git/owner/repo.git".to_string()));
	}

	#[test]
	fn test_parse_credential_input_empty() {
		let input = "";
		let params = parse_credential_input(input);
		assert!(params.is_empty());
	}

	#[test]
	fn test_parse_credential_input_malformed_lines() {
		let input = "protocol=https\ninvalid_line\nhost=example.com\n";
		let params = parse_credential_input(input);
		assert_eq!(params.len(), 2);
		assert_eq!(params.get("protocol"), Some(&"https".to_string()));
		assert_eq!(params.get("host"), Some(&"example.com".to_string()));
	}

	#[test]
	fn test_is_matching_host_exact_match() {
		assert!(is_matching_host(
			"https://loom.ghuntley.com",
			"loom.ghuntley.com"
		));
	}

	#[test]
	fn test_is_matching_host_with_port() {
		assert!(is_matching_host("https://localhost:8080", "localhost"));
	}

	#[test]
	fn test_is_matching_host_with_path() {
		assert!(is_matching_host(
			"https://loom.ghuntley.com/api",
			"loom.ghuntley.com"
		));
	}

	#[test]
	fn test_is_matching_host_no_match() {
		assert!(!is_matching_host("https://loom.ghuntley.com", "other.com"));
	}

	#[test]
	fn test_is_matching_host_invalid_url() {
		assert!(!is_matching_host("not a url", "example.com"));
	}
}

#[cfg(test)]
mod proptests {
	use super::*;
	use proptest::prelude::*;

	proptest! {
		#[test]
		fn parse_roundtrip_preserves_values(
			protocol in "[a-z]+",
			host in "[a-z0-9.-]+",
		) {
			let input = format!("protocol={protocol}\nhost={host}\n");
			let params = parse_credential_input(&input);
			prop_assert_eq!(params.get("protocol"), Some(&protocol));
			prop_assert_eq!(params.get("host"), Some(&host));
		}

		#[test]
		fn is_matching_host_consistent(
			host in "[a-z][a-z0-9-]*\\.[a-z]{2,4}",
		) {
			let url = format!("https://{host}");
			prop_assert!(is_matching_host(&url, &host));
		}
	}
}
