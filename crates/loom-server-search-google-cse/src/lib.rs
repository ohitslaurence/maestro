// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights
// reserved. SPDX-License-Identifier: Proprietary

//! Google Custom Search Engine client for Loom.
//!
//! This crate provides a typed Rust client for the Google CSE API,
//! encapsulating HTTP communication and response parsing.

pub mod client;
pub mod error;
pub mod types;

pub use client::CseClient;
pub use error::CseError;
pub use loom_common_http::RetryConfig;
pub use types::{CseRequest, CseResponse, CseResultItem};
