// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! Agent Client Protocol (ACP) integration for Loom.
//!
//! This crate provides an ACP `Agent` implementation that bridges ACP clients
//! (code editors like Zed) to Loom's existing agent infrastructure. It allows
//! Loom to be driven via the ACP protocol over stdio.
//!
//! # Architecture
//!
//! ```text
//! Editor (Client)  <--->  LoomAcpAgent  <--->  LLM/Tools/ThreadStore
//!      stdio              acp::Agent           existing Loom infra
//! ```
//!
//! The [`LoomAcpAgent`] implements the ACP `Agent` trait and:
//! - Maps ACP sessions to Loom threads
//! - Streams LLM responses as ACP session notifications
//! - Executes tools locally via the existing tool registry
//! - Persists conversations to the thread store

pub mod agent;
pub mod bridge;
pub mod error;
pub mod session;

pub use agent::LoomAcpAgent;
pub use error::AcpError;
pub use session::{SessionNotificationRequest, SessionState};
