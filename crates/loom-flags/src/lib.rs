// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! Feature Flags Rust SDK for Loom.
//!
//! This crate provides a client library for evaluating feature flags against the
//! Loom server. It supports real-time updates via SSE, local caching, and offline mode.
//!
//! # Features
//!
//! - **SDK Key Authentication**: Secure authentication using SDK keys
//! - **Real-time Updates**: SSE streaming for instant flag updates
//! - **Local Caching**: In-memory cache for fast evaluations
//! - **Offline Mode**: Fallback to cached values when disconnected
//! - **Type-safe Evaluation**: Methods for boolean, string, and JSON values
//!
//! # Example
//!
//! ```ignore
//! use loom_flags::{FlagsClient, EvaluationContext};
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     // Initialize client with SDK key
//!     let client = FlagsClient::builder()
//!         .sdk_key("loom_sdk_server_prod_xxx")
//!         .base_url("https://loom.example.com")
//!         .build()
//!         .await?;
//!
//!     // Build evaluation context
//!     let context = EvaluationContext::new("prod")
//!         .with_user_id("user123")
//!         .with_attribute("plan", serde_json::json!("enterprise"));
//!
//!     // Evaluate flags
//!     let enabled = client.get_bool("feature.new_flow", &context, false).await?;
//!     let theme = client.get_string("ui.theme", &context, "light").await?;
//!
//!     // Get all flags at once
//!     let all_flags = client.get_all(&context).await?;
//!
//!     Ok(())
//! }
//! ```

mod analytics;
mod cache;
mod client;
mod error;
mod sse;

pub use analytics::{AnalyticsHook, FlagExposure, NoOpAnalyticsHook, SharedAnalyticsHook};
pub use cache::FlagCache;
pub use client::{FlagsClient, FlagsClientBuilder};
pub use error::{FlagsError, Result};
pub use sse::SseConnection;

// Re-export core types for convenience
pub use loom_flags_core::{
	BulkEvaluationResult, EvaluationContext, EvaluationReason, EvaluationResult, FlagState,
	FlagStreamEvent, GeoContext, KillSwitchState, VariantValue,
};
