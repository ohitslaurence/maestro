// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use std::collections::HashMap;

/// Calculate Shannon entropy of a string.
/// Returns bits per character (0.0 to ~log2(charset_size)).
/// Higher entropy indicates more randomness (likely a secret).
pub fn shannon_entropy(s: &str) -> f32 {
	if s.is_empty() {
		return 0.0;
	}

	let bytes = s.as_bytes();
	let len = bytes.len() as f32;

	let mut freq: HashMap<u8, usize> = HashMap::new();
	for &byte in bytes {
		*freq.entry(byte).or_insert(0) += 1;
	}

	let mut entropy: f32 = 0.0;
	for &count in freq.values() {
		let p = count as f32 / len;
		entropy -= p * p.log2();
	}

	entropy
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_empty_string() {
		assert_eq!(shannon_entropy(""), 0.0);
	}

	#[test]
	fn test_single_character_repeated() {
		assert_eq!(shannon_entropy("aaaaaaaaaa"), 0.0);
		assert_eq!(shannon_entropy("XXXXXXXXXX"), 0.0);
	}

	#[test]
	fn test_two_equal_characters() {
		let entropy = shannon_entropy("ab");
		assert!(
			(entropy - 1.0).abs() < 0.001,
			"Expected ~1.0, got {}",
			entropy
		);
	}

	#[test]
	fn test_four_equal_characters() {
		let entropy = shannon_entropy("abcd");
		assert!(
			(entropy - 2.0).abs() < 0.001,
			"Expected ~2.0, got {}",
			entropy
		);
	}

	#[test]
	fn test_hex_string_high_entropy() {
		let entropy = shannon_entropy("a1b2c3d4e5f6");
		assert!(
			entropy > 3.0,
			"Hex string should have high entropy, got {}",
			entropy
		);
	}

	#[test]
	fn test_base64_like_high_entropy() {
		let entropy = shannon_entropy("aGVsbG8gd29ybGQh");
		assert!(
			entropy > 3.5,
			"Base64-like string should have high entropy, got {}",
			entropy
		);
	}

	#[test]
	fn test_random_api_key() {
		let entropy = shannon_entropy("REDACTED");
		assert!(
			entropy > 4.0,
			"API key should have high entropy, got {}",
			entropy
		);
	}

	#[test]
	fn test_english_text_lower_entropy() {
		let entropy = shannon_entropy("the quick brown fox jumps over the lazy dog");
		assert!(
			entropy > 3.0 && entropy < 4.5,
			"English text should have moderate entropy, got {}",
			entropy
		);
	}

	#[test]
	fn test_simple_password_moderate_entropy() {
		let entropy = shannon_entropy("password123");
		assert!(
			entropy > 2.5 && entropy < 4.0,
			"Simple password should have moderate entropy, got {}",
			entropy
		);
	}

	#[test]
	fn test_uuid_high_entropy() {
		let entropy = shannon_entropy("550e8400-e29b-41d4-a716-446655440000");
		assert!(
			entropy > 3.0,
			"UUID should have high entropy, got {}",
			entropy
		);
	}

	#[test]
	fn test_known_vector_uniform_distribution() {
		let entropy = shannon_entropy("0123456789abcdef");
		assert!(
			(entropy - 4.0).abs() < 0.001,
			"16 unique chars should give ~4.0 bits, got {}",
			entropy
		);
	}
}
