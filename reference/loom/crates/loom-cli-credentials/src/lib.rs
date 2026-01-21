// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! Provider-agnostic credential storage for Loom.
//!
//! This crate provides generic credential storage abstractions that can be used
//! by any LLM provider (Anthropic, OpenAI, etc.) for storing API keys, OAuth tokens,
//! and other authentication credentials.
//!
//! # Features
//!
//! - **CredentialStore trait**: Abstract interface for credential backends
//! - **FileCredentialStore**: JSON file-based storage with secure permissions
//! - **MemoryCredentialStore**: In-memory storage for testing
//!
//! # Example
//!
//! ```rust,no_run
//! use loom_cli_credentials::{CredentialStore, FileCredentialStore, CredentialValue};
//! use loom_common_secret::SecretString;
//!
//! # tokio_test::block_on(async {
//! let store = FileCredentialStore::new("~/.config/loom/credentials.json");
//!
//! // Store an API key
//! let cred = CredentialValue::ApiKey {
//!     key: SecretString::new("sk-test-key".to_string()),
//! };
//! store.save("anthropic", &cred).await.unwrap();
//!
//! // Load it back
//! let loaded = store.load("anthropic").await.unwrap();
//! # });
//! ```

mod error;
mod store;
#[cfg(feature = "keyring")]
mod store_fallback;
#[cfg(feature = "keyring")]
mod store_keyring;
mod value;

pub use error::CredentialError;
pub use store::{CredentialStore, FileCredentialStore, MemoryCredentialStore};
#[cfg(feature = "keyring")]
pub use store_fallback::KeyringThenFileStore;
#[cfg(feature = "keyring")]
pub use store_keyring::KeyringCredentialStore;
pub use value::{CredentialValue, PersistedCredentialValue};
