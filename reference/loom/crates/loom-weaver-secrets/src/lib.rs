// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! Client library for weavers to access secrets.
//!
//! This crate provides a simple API for weavers to:
//! 1. Obtain a Weaver SVID using K8s service account credentials
//! 2. Fetch secrets from the Loom secrets service
//!
//! # Example
//!
//! ```ignore
//! use loom_weaver_secrets::{SecretsClient, SecretScope};
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let client = SecretsClient::new()?;
//!     
//!     // Fetch an org-scoped secret
//!     let api_key = client.get_secret(SecretScope::Org, "STRIPE_API_KEY").await?;
//!     
//!     // Use the secret (it's a SecretString, so it auto-redacts in logs)
//!     println!("Got API key: {}", api_key); // Prints: Got API key: [REDACTED]
//!     
//!     // Access the actual value when needed
//!     let value = api_key.expose();
//!     
//!     Ok(())
//! }
//! ```

mod client;
mod error;

pub use client::{ClientConfig, SecretsClient};
pub use error::{SecretsClientError, SecretsClientResult};
pub use loom_common_secret::SecretString;

/// Secret scope for fetching secrets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecretScope {
	/// Organization-wide secret
	Org,
	/// Repository-specific secret
	Repo,
	/// Weaver-instance secret (ephemeral)
	Weaver,
}

impl SecretScope {
	/// Get the path segment for this scope.
	pub fn path_segment(&self) -> &'static str {
		match self {
			SecretScope::Org => "org",
			SecretScope::Repo => "repo",
			SecretScope::Weaver => "weaver",
		}
	}
}
