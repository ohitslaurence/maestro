// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! User provisioning service for Loom.
//!
//! Provides a single code path for user creation across all authentication methods:
//! - OAuth (GitHub, Google, Okta)
//! - Magic link
//! - SCIM (enterprise IdP provisioning)
//!
//! When a user is provisioned, they automatically get a personal organization.

mod error;
mod request;
mod service;

pub use error::ProvisioningError;
pub use request::{ProvisioningRequest, ProvisioningSource};
pub use service::UserProvisioningService;
