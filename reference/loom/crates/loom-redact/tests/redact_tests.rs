// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

use loom_redact::{contains_secrets, detect, redact, redact_in_place};
use std::borrow::Cow;

fn github_pat() -> String {
	format!("ghp_{}", "A1b2C3d4E5f6G7h8I9j0K1l2M3n4O5p6Q7r8")
}

fn aws_key() -> String {
	format!("AKIA{}", "Z7VRSQ5TJN2XMPLQ")
}

#[test]
fn test_no_secrets_unchanged() {
	let input = "This is just normal text without any secrets.";
	let result = redact(input);
	assert!(matches!(result, Cow::Borrowed(_)));
	assert_eq!(result, input);
}

#[test]
fn test_aws_access_token() {
	let input = format!("AWS_ACCESS_KEY_ID={}", aws_key());
	let result = redact(&input);
	assert!(
		result.contains("REDACTED:aws-access-token"),
		"Result: {}",
		result
	);
}

#[test]
fn test_github_pat() {
	let input = format!("export GITHUB_TOKEN={}", github_pat());
	let result = redact(&input);
	assert!(result.contains("REDACTED:github-pat"), "Result: {}", result);
}

#[test]
fn test_github_fine_grained_pat() {
	let token = format!(
		"github_pat_{}",
		"11ABCDEFG0123456789_abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789abc"
	);
	let input = format!("token: {}", token);
	let result = redact(&input);
	assert!(
		result.contains("REDACTED:github-fine-grained-pat"),
		"Result: {}",
		result
	);
}

#[test]
fn test_stripe_secret_key() {
	let key = format!("sk_live_{}", "A1b2C3d4E5f6G7h8I9j0");
	let input = format!(r#"stripe_key = "{}""#, key);
	let result = redact(&input);
	assert!(result.contains("REDACTED:stripe"), "Result: {}", result);
}

#[test]
fn test_multiple_secrets() {
	let input = format!("AWS: {} GitHub: {}", aws_key(), github_pat());
	let result = redact(&input);
	assert!(
		result.contains("REDACTED:aws-access-token"),
		"Result: {}",
		result
	);
	assert!(result.contains("REDACTED:github-pat"), "Result: {}", result);
}

#[test]
fn test_idempotence() {
	let input = format!("My key is {}", aws_key());
	let first = redact(&input);
	let second = redact(&first);
	assert_eq!(first, second);
}

#[test]
fn test_idempotence_multiple() {
	let input = format!("Keys: {} and {}", aws_key(), github_pat());
	let first = redact(&input);
	let second = redact(&first);
	let third = redact(&second);
	assert_eq!(first, second);
	assert_eq!(second, third);
}

#[test]
fn test_redact_in_place() {
	let mut input = github_pat();
	let changed = redact_in_place(&mut input);
	assert!(changed);
	assert!(input.contains("REDACTED:github-pat"));
}

#[test]
fn test_redact_in_place_no_change() {
	let mut input = "No secrets here".to_string();
	let changed = redact_in_place(&mut input);
	assert!(!changed);
	assert_eq!(input, "No secrets here");
}

#[test]
fn test_contains_secrets_true() {
	let input = format!("Token: {}", github_pat());
	assert!(contains_secrets(&input));
}

#[test]
fn test_contains_secrets_false() {
	let input = "Just normal text";
	assert!(!contains_secrets(input));
}

#[test]
fn test_detect_returns_positions() {
	let key = aws_key();
	let input = format!("Key: {} here", key);
	let detections = detect(&input);
	assert!(!detections.is_empty());
	let d = &detections[0];
	assert!(
		d.rule_id == "aws-access-token" || d.rule_id == "generic-api-key",
		"Expected aws-access-token or generic-api-key, got {}",
		d.rule_id
	);
	assert!(input[d.start..d.end].contains("AKIA"));
}

#[test]
fn test_entropy_filtering_low_entropy() {
	let input = "AKIAAAAAAAAAAAAAAAAA";
	let result = redact(input);
	assert!(
		!result.contains("REDACTED:"),
		"Low entropy should not match: {}",
		result
	);
}

#[test]
fn test_jwt_detection() {
	let jwt = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiIxMjM0NTY3ODkwIiwibmFtZSI6IkpvaG4iLCJpYXQiOjE1MTYyMzkwMjJ9.SflKxwRJSMeKKF2QT4fwpMeJf36POk6yJV_adQssw5c";
	let input = format!("Bearer {}", jwt);
	let result = redact(&input);
	assert!(result.contains("REDACTED:jwt"), "Result: {}", result);
}

#[test]
fn test_slack_token() {
	let token = format!(
		"xoxb-{}-{}-{}",
		"1234567890123", "1234567890123", "AbCdEfGhIjKlMnOpQrStUvWx"
	);
	let input = format!("SLACK_TOKEN={}", token);
	let result = redact(&input);
	assert!(result.contains("REDACTED:slack"), "Result: {}", result);
}

#[test]
fn test_private_key_detection() {
	let input = "-----BEGIN RSA PRIVATE KEY-----\nMIIEpAIBAAKCAQEA2Z3qX2BTLS4e0rEZfghij1234567890abcdefghijklmnopqrst\nuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789abcdefghijklmnopqrstuvwx\n-----END RSA PRIVATE KEY-----";
	let result = redact(input);
	assert!(
		result.contains("REDACTED:private-key"),
		"Result: {}",
		result
	);
}

#[test]
fn test_preserves_surrounding_text() {
	let input = format!("Before {} after", github_pat());
	let result = redact(&input);
	assert!(result.starts_with("Before "));
	assert!(result.ends_with(" after"));
}

#[test]
fn test_newlines_preserved() {
	let input = format!("Line 1\n{}\nLine 3", github_pat());
	let result = redact(&input);
	assert!(result.contains("Line 1\n"));
	assert!(result.contains("\nLine 3"));
	assert!(result.contains("REDACTED:github-pat"));
}

#[test]
fn test_unicode_text_unchanged() {
	let input = "日本語テキスト emoji and special chars: àéïõü";
	let result = redact(input);
	assert!(matches!(result, Cow::Borrowed(_)));
	assert_eq!(result, input);
}

#[test]
fn test_empty_string() {
	let result = redact("");
	assert!(matches!(result, Cow::Borrowed(_)));
	assert_eq!(result, "");
}

#[test]
fn test_huggingface_token() {
	let token = format!("hf_{}", "abcdefghijklmnopqrstuvwxyzabcdefgh");
	let input = format!("HF_TOKEN={}", token);
	let result = redact(&input);
	assert!(
		result.contains("REDACTED:huggingface"),
		"Result: {}",
		result
	);
}

#[test]
fn test_linear_api_key() {
	let token = format!("lin_api_{}", "abcdefghij1234567890abcdefghij1234567890");
	let input = format!("LINEAR_API_KEY={}", token);
	let result = redact(&input);
	assert!(
		result.contains("REDACTED:linear-api-key"),
		"Result: {}",
		result
	);
}

#[test]
fn test_sendgrid_api_key() {
	let token = format!(
		"SG.{}",
		"abcdefghij1234567890abcdefghij1234567890abcdefghij1234567890abcdef"
	);
	let input = format!("SENDGRID_API_KEY={}", token);
	let result = redact(&input);
	assert!(result.contains("REDACTED:sendgrid"), "Result: {}", result);
}
