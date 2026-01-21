// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! Real-time secret detection and redaction using gitleaks patterns.
//!
//! This crate scans arbitrary text for secrets (API keys, tokens, passwords)
//! and replaces them with `[REDACTED:<rule-id>]` placeholders.

mod entropy;
mod rule;

include!(concat!(env!("OUT_DIR"), "/generated_rules.rs"));

use once_cell::sync::Lazy;
use regex::{Regex, RegexBuilder};
use std::borrow::Cow;

const REGEX_SIZE_LIMIT: usize = 50 * 1024 * 1024; // 50MB for large patterns

pub use entropy::shannon_entropy;
pub use rule::CompiledRule;

static RULES: Lazy<Vec<CompiledRule>> = Lazy::new(|| {
	GENERATED_RULES
		.iter()
		.filter_map(|g| CompiledRule::from_generated(g).ok())
		.collect()
});

#[derive(Debug, Clone)]
pub struct Detection {
	pub rule_id: &'static str,
	pub start: usize,
	pub end: usize,
}

#[derive(Debug, thiserror::Error)]
pub enum RedactError {
	#[error("regex compilation failed: {0}")]
	RegexError(#[from] regex::Error),
}

struct SecretMatch {
	start: usize,
	end: usize,
	rule_id: &'static str,
}

fn find_matches(input: &str) -> Vec<SecretMatch> {
	let input_lower = input.to_lowercase();
	let mut matches: Vec<SecretMatch> = Vec::new();

	for rule in RULES.iter() {
		if !rule.should_check(&input_lower) {
			continue;
		}

		for cap in rule.regex.captures_iter(input) {
			let secret_match = if rule.secret_group > 0 {
				cap.get(rule.secret_group as usize)
			} else {
				cap.get(0)
			};

			let Some(m) = secret_match else {
				continue;
			};

			let secret_text = m.as_str();

			if let Some(threshold) = rule.entropy {
				if shannon_entropy(secret_text) < threshold {
					continue;
				}
			}

			if rule.is_allowed(secret_text) {
				continue;
			}

			matches.push(SecretMatch {
				start: m.start(),
				end: m.end(),
				rule_id: rule.id,
			});
		}
	}

	matches.sort_by_key(|m| m.start);

	let mut deduped: Vec<SecretMatch> = Vec::new();
	for m in matches {
		if let Some(last) = deduped.last() {
			if m.start < last.end {
				continue;
			}
		}
		deduped.push(m);
	}

	deduped
}

pub fn redact(input: &str) -> Cow<'_, str> {
	let matches = find_matches(input);

	if matches.is_empty() {
		return Cow::Borrowed(input);
	}

	let mut result = String::with_capacity(input.len());
	let mut last_end = 0;

	for m in matches {
		result.push_str(&input[last_end..m.start]);
		result.push('[');
		result.push_str("REDACTED:");
		result.push_str(m.rule_id);
		result.push(']');
		last_end = m.end;
	}

	result.push_str(&input[last_end..]);
	Cow::Owned(result)
}

pub fn redact_in_place(input: &mut String) -> bool {
	let result = redact(input);
	match result {
		Cow::Borrowed(_) => false,
		Cow::Owned(redacted) => {
			*input = redacted;
			true
		}
	}
}

pub fn contains_secrets(input: &str) -> bool {
	let input_lower = input.to_lowercase();

	for rule in RULES.iter() {
		if !rule.should_check(&input_lower) {
			continue;
		}

		for cap in rule.regex.captures_iter(input) {
			let secret_match = if rule.secret_group > 0 {
				cap.get(rule.secret_group as usize)
			} else {
				cap.get(0)
			};

			let Some(m) = secret_match else {
				continue;
			};

			let secret_text = m.as_str();

			if let Some(threshold) = rule.entropy {
				if shannon_entropy(secret_text) < threshold {
					continue;
				}
			}

			if rule.is_allowed(secret_text) {
				continue;
			}

			return true;
		}
	}

	false
}

pub fn detect(input: &str) -> Vec<Detection> {
	find_matches(input)
		.into_iter()
		.map(|m| Detection {
			rule_id: m.rule_id,
			start: m.start,
			end: m.end,
		})
		.collect()
}

fn build_regex(pattern: &str) -> Result<Regex, regex::Error> {
	RegexBuilder::new(pattern)
		.size_limit(REGEX_SIZE_LIMIT)
		.build()
}

impl CompiledRule {
	pub fn from_generated(g: &GeneratedRule) -> Result<Self, regex::Error> {
		let regex = build_regex(g.regex)?;
		let allowlist_patterns: Vec<Regex> = g
			.allowlist_patterns
			.iter()
			.filter_map(|p| build_regex(p).ok())
			.collect();

		Ok(CompiledRule {
			id: g.id,
			regex,
			secret_group: g.secret_group,
			entropy: g.entropy,
			keywords: g.keywords,
			allowlist_patterns,
			allowlist_stopwords: g.allowlist_stopwords,
		})
	}
}
