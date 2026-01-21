// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! Rust SDK for Loom product analytics.
//!
//! This crate provides a client for capturing analytics events and identifying users
//! in the Loom analytics system. Events are batched and sent in the background for
//! optimal performance.
//!
//! # Quick Start
//!
//! ```ignore
//! use loom_analytics::{AnalyticsClient, Properties};
//! use std::time::Duration;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     // Initialize the client
//!     let client = AnalyticsClient::builder()
//!         .api_key("loom_analytics_write_xxx")
//!         .base_url("https://loom.example.com")
//!         .flush_interval(Duration::from_secs(10))
//!         .build()?;
//!
//!     // Capture an event
//!     client.capture("button_clicked", "user_123", Properties::new()
//!         .insert("button_name", "checkout")
//!         .insert("page", "/cart")
//!     ).await?;
//!
//!     // Identify a user (links anonymous to authenticated)
//!     client.identify("anon_abc123", "user@example.com", Properties::new()
//!         .insert("plan", "pro")
//!         .insert("company", "Acme Inc")
//!     ).await?;
//!
//!     // Set person properties
//!     client.set("user@example.com", Properties::new()
//!         .insert("last_login", chrono::Utc::now().to_rfc3339())
//!     ).await?;
//!
//!     // Shutdown gracefully (flushes pending events)
//!     client.shutdown().await?;
//!
//!     Ok(())
//! }
//! ```
//!
//! # Event Batching
//!
//! Events are queued locally and sent to the server in batches. This provides:
//!
//! - **Better performance**: Fewer HTTP requests
//! - **Resilience**: Events are queued if the server is temporarily unavailable
//! - **Non-blocking**: `capture()` returns immediately
//!
//! Configure batching behavior with the builder:
//!
//! ```ignore
//! let client = AnalyticsClient::builder()
//!     .api_key("loom_analytics_write_xxx")
//!     .base_url("https://loom.example.com")
//!     .flush_interval(Duration::from_secs(10))  // Flush every 10 seconds
//!     .max_batch_size(10)                       // Or when 10 events are queued
//!     .max_queue_size(1000)                     // Max events to queue
//!     .build()?;
//! ```
//!
//! # API Keys
//!
//! The SDK accepts two types of API keys:
//!
//! | Type | Prefix | Use Case |
//! |------|--------|----------|
//! | Write | `loom_analytics_write_` | Safe for client-side (capture only) |
//! | ReadWrite | `loom_analytics_rw_` | Server-side only (capture + query) |
//!
//! # Graceful Shutdown
//!
//! Always call `shutdown()` before your application exits to ensure all
//! pending events are sent:
//!
//! ```ignore
//! client.shutdown().await?;
//! ```
//!
//! # Error Handling
//!
//! The SDK uses retries with exponential backoff for transient failures.
//! Non-retryable errors (validation, auth) are returned immediately.
//!
//! ```ignore
//! use loom_analytics::AnalyticsError;
//!
//! match client.capture("event", "user_123", Properties::new()).await {
//!     Ok(()) => println!("Event queued"),
//!     Err(AnalyticsError::ValidationFailed(msg)) => {
//!         eprintln!("Invalid event: {}", msg);
//!     }
//!     Err(AnalyticsError::ClientShutdown) => {
//!         eprintln!("Client has been shut down");
//!     }
//!     Err(e) => {
//!         eprintln!("Unexpected error: {}", e);
//!     }
//! }
//! ```

pub mod batch;
pub mod client;
pub mod error;
pub mod properties;

pub use client::{AnalyticsClient, AnalyticsClientBuilder, ClientConfig};
pub use error::{AnalyticsError, Result};
pub use properties::Properties;

// Re-export types from loom-analytics-core that users may need
pub use loom_analytics_core::{
	AliasPayload, Event, EventId, IdentifyPayload, OrgId, Person, PersonId,
};
