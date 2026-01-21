// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! Weaver Secrets System
//!
//! This crate provides secure secret management for Loom weavers:
//!
//! - **Secret Storage**: Encrypted at-rest storage with envelope encryption
//! - **Weaver Identity**: SPIFFE-style SVIDs for weaver authentication
//! - **Access Control**: ABAC-based secret access policies
//! - **Audit Integration**: Full lifecycle audit logging
//!
//! # Security Design
//!
//! - All secret values use [`SecretString`] to prevent logging
//! - Envelope encryption: Master Key (KEK) encrypts per-secret DEKs
//! - Weaver SVIDs are short-lived JWTs (15 min default)
//! - No secrets in environment variables; pull-only API

pub mod config;
pub mod encryption;
pub mod error;
pub mod key_backend;
pub mod policy;
pub mod service;
pub mod store;
pub mod svid;
pub mod types;

pub use config::SecretsConfig;
pub use encryption::{generate_key, EncryptedData, KEY_SIZE, NONCE_SIZE};
pub use error::{SecretsError, SecretsResult};
pub use key_backend::{
	EncryptedDekData, JsonWebKey, JsonWebKeySet, KeyBackend, SoftwareKeyBackend,
};
pub use policy::{can_access_secret, WeaverPrincipal};
pub use service::{CreateSecretInput, SecretMetadata, SecretValue, SecretsService};
pub use store::{SecretStore, SqliteSecretStore};
pub use svid::{
	PodMetadata, SvidConfig, SvidIssuer, SvidRequest, ValidatedSaToken, WeaverClaims, WeaverSvid,
};
pub use types::{
	EncryptedDek, Secret, SecretId, SecretScope, SecretVersion, SecretVersionId, WeaverId,
};
