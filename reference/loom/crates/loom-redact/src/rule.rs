// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use regex::Regex;

pub struct CompiledRule {
	pub id: &'static str,
	pub regex: Regex,
	pub secret_group: u32,
	pub entropy: Option<f32>,
	pub keywords: &'static [&'static str],
	pub allowlist_patterns: Vec<Regex>,
	pub allowlist_stopwords: &'static [&'static str],
}

impl CompiledRule {
	pub fn should_check(&self, text_lower: &str) -> bool {
		if self.keywords.is_empty() {
			return true;
		}
		self.keywords.iter().any(|kw| text_lower.contains(kw))
	}

	pub fn is_allowed(&self, matched_text: &str) -> bool {
		let matched_lower = matched_text.to_lowercase();
		for stopword in self.allowlist_stopwords {
			if matched_lower.contains(stopword) {
				return true;
			}
		}
		for pattern in &self.allowlist_patterns {
			if pattern.is_match(matched_text) {
				return true;
			}
		}
		false
	}
}
