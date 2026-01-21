// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! Google Vertex AI (Gemini) LLM client implementation for Loom.
//!
//! This crate provides an implementation of the `LlmClient` trait for Google's
//! Gemini models via the Vertex AI API.
//!
//! # Authentication
//!
//! This client uses Google Application Default Credentials (ADC) for authentication.
//! Set up credentials via one of:
//! - `GOOGLE_APPLICATION_CREDENTIALS` environment variable pointing to a service account JSON
//! - Default service account on GCE/GKE
//! - `gcloud auth application-default login` for local development
//!
//! # Example
//!
//! ```no_run
//! use loom_server_llm_vertex::{VertexClient, VertexConfig};
//! use loom_common_core::{LlmClient, LlmRequest, Message};
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let config = VertexConfig::new("my-gcp-project", "us-central1")
//!     .with_model("gemini-1.5-pro");
//!
//! let client = VertexClient::new(config)?;
//!
//! let request = LlmRequest::new("gemini-1.5-pro")
//!     .with_messages(vec![Message::user("Hello!")]);
//!
//! let response = client.complete(request).await?;
//! println!("Response: {}", response.message.content);
//! # Ok(())
//! # }
//! ```

mod client;
mod stream;
mod types;

pub use client::VertexClient;
pub use types::VertexConfig;
