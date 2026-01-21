// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights
// reserved. SPDX-License-Identifier: Proprietary

//! Shared HTTP utilities for Loom.
//!
//! This crate provides:
//! - A pre-configured HTTP client with consistent User-Agent header
//! - Retry logic with exponential backoff for transient failures

mod client;
mod retry;

pub use client::{
	builder, builder_with_user_agent, new_client, new_client_with_timeout,
	new_client_with_user_agent, user_agent,
};
pub use retry::{retry, RetryConfig, RetryableError};
