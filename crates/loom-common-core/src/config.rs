// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights
// reserved. SPDX-License-Identifier: Proprietary

use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Configuration for an agent.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AgentConfig {
	pub model_name: String,
	pub max_retries: u32,
	#[serde(with = "humantime_serde")]
	pub tool_timeout: Duration,
	#[serde(with = "humantime_serde")]
	pub llm_timeout: Duration,
	pub max_tokens: u32,
	pub temperature: Option<f32>,
}

impl Default for AgentConfig {
	fn default() -> Self {
		Self {
			model_name: "claude-opus-4-20250514".to_string(),
			max_retries: 3,
			tool_timeout: Duration::from_secs(30),
			llm_timeout: Duration::from_secs(120),
			max_tokens: 4096,
			temperature: None,
		}
	}
}

mod humantime_serde {
	use serde::{Deserialize, Deserializer, Serialize, Serializer};
	use std::time::Duration;

	pub fn serialize<S>(duration: &Duration, serializer: S) -> Result<S::Ok, S::Error>
	where
		S: Serializer,
	{
		duration.as_secs().serialize(serializer)
	}

	pub fn deserialize<'de, D>(deserializer: D) -> Result<Duration, D::Error>
	where
		D: Deserializer<'de>,
	{
		let secs = u64::deserialize(deserializer)?;
		Ok(Duration::from_secs(secs))
	}
}
