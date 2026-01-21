// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! WireGuard daemon for Loom weavers.
//!
//! This crate implements the weaver-side WireGuard daemon that runs inside weaver pods,
//! enabling secure network connectivity between user devices and weavers.
//!
//! # Overview
//!
//! The daemon:
//! 1. Generates an ephemeral WireGuard keypair (in-memory only, never persisted)
//! 2. Authenticates with loom-server using SVID from loom-weaver-secrets
//! 3. Registers its public key and receives an assigned IPv6 address
//! 4. Subscribes to a peer stream to receive notifications about connected clients
//! 5. Manages WireGuard peers dynamically as clients connect/disconnect
//!
//! # Example
//!
//! ```ignore
//! use loom_weaver_wgtunnel::{WeaverWgDaemon, WeaverWgConfig};
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let config = WeaverWgConfig::from_env()?;
//!     let mut daemon = WeaverWgDaemon::new(config).await?;
//!     
//!     // Run until shutdown signal
//!     daemon.run().await?;
//!     
//!     Ok(())
//! }
//! ```

pub mod config;
pub mod daemon;
pub mod error;
pub mod peer_handler;
pub mod registration;

pub use config::WeaverWgConfig;
pub use daemon::WeaverWgDaemon;
pub use error::{ConfigError, DaemonError, PeerError, RegistrationError, Result};
pub use peer_handler::{PeerEvent, PeerHandler};
pub use registration::{Registration, RegistrationResponse};
