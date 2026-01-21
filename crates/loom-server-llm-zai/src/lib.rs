// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights
// reserved. SPDX-License-Identifier: Proprietary

//! Z.ai (智谱AI/ZhipuAI) LLM client implementation for Loom.

mod client;
mod stream;
mod types;

pub use client::ZaiClient;
pub use types::*;
