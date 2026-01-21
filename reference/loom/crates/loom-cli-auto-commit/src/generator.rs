// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights
// reserved. SPDX-License-Identifier: Proprietary

use std::sync::Arc;

use loom_common_core::llm::{LlmClient, LlmRequest};
use loom_common_core::message::Message;
use tracing::{debug, info, warn};

use loom_cli_git::GitDiff;

use crate::config::AutoCommitConfig;
use crate::error::AutoCommitError;

const SYSTEM_PROMPT: &str = r#"You are an expert software engineer generating git commit messages.

Rules:
1. Use conventional commit format: <type>(<scope>): <description>
2. Types: feat, fix, refactor, docs, style, test, chore
3. Keep the first line under 72 characters
4. Be specific about what changed, not why
5. Use imperative mood ("add" not "added")
6. If multiple unrelated changes, summarize the primary one
7. Output ONLY the commit message, nothing else

Examples:
- feat(auth): add JWT token validation
- fix(api): handle null response from upstream
- refactor(tools): extract path validation helper
- chore(deps): update tokio to 1.35"#;

const FALLBACK_MESSAGE: &str =
	"chore: auto-commit from loom\n\n[Auto-generated fallback: LLM unavailable]";

pub struct CommitMessageGenerator<L: LlmClient> {
	llm: Arc<L>,
	config: AutoCommitConfig,
}

impl<L: LlmClient> CommitMessageGenerator<L> {
	pub fn new(llm: Arc<L>, config: AutoCommitConfig) -> Self {
		Self { llm, config }
	}

	pub async fn generate(&self, diff: &GitDiff) -> Result<String, AutoCommitError> {
		let truncated_diff = self.truncate_diff(&diff.content);
		let truncation_notice = if truncated_diff.len() < diff.content.len() {
			format!(
				"\n\n[TRUNCATED: {} bytes, showing first {}]",
				diff.content.len(),
				truncated_diff.len()
			)
		} else {
			String::new()
		};

		let user_content = format!(
			"Generate a commit message for these changes:\n\n<diff>\n{truncated_diff}\n</diff>{truncation_notice}"
		);

		debug!(
			diff_bytes = diff.content.len(),
			truncated_bytes = truncated_diff.len(),
			files_changed = diff.files_changed.len(),
			"generating commit message"
		);

		let request = LlmRequest {
			model: self.config.model.clone(),
			messages: vec![Message::system(SYSTEM_PROMPT), Message::user(user_content)],
			tools: vec![],
			max_tokens: Some(256),
			temperature: Some(0.3),
		};

		match self.llm.complete(request).await {
			Ok(response) => {
				let message = response.message.content.trim().to_string();
				info!(message_len = message.len(), "generated commit message");
				Ok(message)
			}
			Err(e) => {
				warn!(error = %e, "LLM failed, using fallback message");
				Ok(FALLBACK_MESSAGE.to_string())
			}
		}
	}

	fn truncate_diff(&self, content: &str) -> String {
		if content.len() <= self.config.max_diff_bytes {
			content.to_string()
		} else {
			// Find a valid UTF-8 boundary at or before max_diff_bytes
			let mut end = self.config.max_diff_bytes;
			while end > 0 && !content.is_char_boundary(end) {
				end -= 1;
			}
			let truncated = &content[..end];
			// Try to truncate at a newline boundary for cleaner output
			if let Some(last_newline) = truncated.rfind('\n') {
				truncated[..last_newline].to_string()
			} else {
				truncated.to_string()
			}
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use proptest::prelude::*;

	proptest! {
			/// Validates that truncation never exceeds max_diff_bytes.
			/// This is critical to prevent OOM when processing large diffs
			/// and ensures predictable memory usage during commit message generation.
			#[test]
			fn truncation_never_exceeds_max_bytes(
					content in ".{0,100000}",
					max_bytes in 1usize..50000,
			) {
					let config = AutoCommitConfig::default().with_max_diff_bytes(max_bytes);

					struct DummyLlm;
					#[async_trait::async_trait]
					impl LlmClient for DummyLlm {
							async fn complete(&self, _: LlmRequest) -> Result<loom_common_core::llm::LlmResponse, loom_common_core::error::LlmError> {
									unimplemented!()
							}
							async fn complete_streaming(&self, _: LlmRequest) -> Result<loom_common_core::llm::LlmStream, loom_common_core::error::LlmError> {
									unimplemented!()
							}
					}

					let generator = CommitMessageGenerator::new(Arc::new(DummyLlm), config);
					let truncated = generator.truncate_diff(&content);

					prop_assert!(truncated.len() <= max_bytes);
			}

			/// Validates that small diffs are not truncated.
			/// This ensures we don't lose information when the diff fits within limits.
			#[test]
			fn small_diffs_are_not_truncated(
					content in ".{0,100}",
			) {
					let config = AutoCommitConfig::default().with_max_diff_bytes(1000);

					struct DummyLlm;
					#[async_trait::async_trait]
					impl LlmClient for DummyLlm {
							async fn complete(&self, _: LlmRequest) -> Result<loom_common_core::llm::LlmResponse, loom_common_core::error::LlmError> {
									unimplemented!()
							}
							async fn complete_streaming(&self, _: LlmRequest) -> Result<loom_common_core::llm::LlmStream, loom_common_core::error::LlmError> {
									unimplemented!()
							}
					}

					let generator = CommitMessageGenerator::new(Arc::new(DummyLlm), config);
					let truncated = generator.truncate_diff(&content);

					prop_assert_eq!(truncated, content);
			}
	}
}
