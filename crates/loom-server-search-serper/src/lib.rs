// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights
// reserved. SPDX-License-Identifier: Proprietary

//! Serper.dev Google Search API client for Loom.
//!
//! This crate provides a typed Rust client for the Serper.dev API,
//! encapsulating HTTP communication and response parsing.

pub mod client;
pub mod error;
pub mod types;

pub use client::SerperClient;
pub use error::SerperError;
pub use loom_common_http::RetryConfig;
pub use types::{SerperRequest, SerperResponse, SerperResultItem};
