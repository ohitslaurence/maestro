// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! LLM proxy client for communicating with server-side LLM proxies.
//!
//! This crate provides a client implementation that talks to server proxy
//! endpoints instead of direct LLM provider APIs. The client does not need
//! API keys as authentication is handled server-side.
//!
//! # Example
//!
//! ```no_run
//! use loom_server_llm_proxy::{ProxyLlmClient, LlmProvider};
//! use loom_common_core::{LlmClient, LlmRequest, Message};
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! // Create a client for Anthropic
//! let client = ProxyLlmClient::anthropic("http://localhost:8080");
//!
//! // Or for OpenAI
//! let openai_client = ProxyLlmClient::openai("http://localhost:8080");
//!
//! let request = LlmRequest::new("claude-3-5-sonnet-20241022")
//!     .with_messages(vec![Message::user("Hello!")]);
//!
//! let response = client.complete(request).await?;
//! println!("Response: {}", response.message.content);
//! # Ok(())
//! # }
//! ```

pub mod client;
pub mod stream;
pub mod types;

pub use client::{LlmProvider, ProxyLlmClient};
pub use stream::ProxyLlmStream;
pub use types::{LlmProxyResponse, LlmStreamEvent};
