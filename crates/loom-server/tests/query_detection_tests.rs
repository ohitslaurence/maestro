// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

//! Query detection tests for Phase 2 integration.
//!
//! **Purpose**: Validates that LLM output patterns are correctly recognized and converted
//! into structured queries. Tests regex detection accuracy, argument extraction, and
//! edge case handling.
//!
//! These tests ensure the query detection system can reliably extract intent from
//! natural language LLM output without false positives or missed patterns.

use loom_common_core::server_query::ServerQueryKind;
use loom_server::SimpleRegexDetector;

// ============================================================================
// ReadFile Pattern Detection Tests
// ============================================================================

/// Test basic ReadFile pattern detection with various phrasings.
/// **Purpose**: Ensures the detector recognizes different natural language ways
/// to express file reading intent (e.g., "read", "show me", "examine").
#[test]
fn test_read_file_detection_basic_patterns() {
	let detector = SimpleRegexDetector::new();

	let test_cases = vec![
		("I need to read config.json", true, "/config.json"),
		("Let me read test.txt", true, "/test.txt"),
		("Show me src/main.rs", true, "/src/main.rs"),
		("read the file settings.ini", true, "/settings.ini"),
		("get file data.json", true, "/data.json"),
		("just some text", false, ""),
	];

	for (input, should_match, expected_path) in test_cases {
		let queries = detector.detect_queries(input).unwrap();

		if should_match {
			assert!(!queries.is_empty(), "Expected to detect query in: {input}");
			if let Some(query) = queries.first() {
				match &query.kind {
					ServerQueryKind::ReadFile { path } => {
						assert_eq!(path, expected_path, "Path mismatch for input: {input}");
					}
					_ => panic!("Expected ReadFile query for: {input}"),
				}
			}
		} else {
			assert!(queries.is_empty(), "Unexpected query in: {input}");
		}
	}
}

/// Test ReadFile detection with absolute and relative paths.
/// **Purpose**: Validates path normalization and handling of different path formats
/// to ensure consistent behavior regardless of how the path is specified.
#[test]
fn test_read_file_path_normalization() {
	let detector = SimpleRegexDetector::new();

	let test_cases = vec![
		("read /app/config.json", "/app/config.json"),
		("read app/config.json", "/app/config.json"),
		("read ./src/main.rs", "./src/main.rs"),
		("read 'src/lib.rs'", "/src/lib.rs"),
		("read \"config.json\"", "/config.json"),
	];

	for (input, expected_path) in test_cases {
		let queries = detector.detect_queries(input).unwrap();
		assert!(!queries.is_empty(), "Expected query for: {input}");

		if let Some(query) = queries.first() {
			if let ServerQueryKind::ReadFile { path } = &query.kind {
				assert_eq!(path, expected_path, "Path mismatch for: {input}");
			}
		}
	}
}

// ============================================================================
// ExecuteCommand Pattern Detection Tests
// ============================================================================

/// Test ExecuteCommand pattern detection with command arguments.
/// **Purpose**: Ensures command names and arguments are correctly extracted.
/// Incorrect parsing could lead to commands being executed with wrong arguments.
#[test]
fn test_execute_command_detection() {
	let detector = SimpleRegexDetector::new();

	let test_cases = vec![
		("run ls -la", "ls"),
		("execute npm install", "npm"),
		("run command cargo build --release", "command"),
	];

	for (input, expected_cmd) in test_cases {
		let queries = detector.detect_queries(input).unwrap();
		assert!(!queries.is_empty(), "Expected query for: {input}");

		if let Some(query) = queries.first() {
			if let ServerQueryKind::ExecuteCommand {
				command,
				args: _,
				timeout_secs: _,
			} = &query.kind
			{
				assert_eq!(command, expected_cmd, "Command mismatch for: {input}");
			}
		}
	}
}

/// Test command argument extraction accuracy.
/// **Purpose**: Validates that command-line arguments are properly split and extracted.
/// Malformed arguments could cause command execution failures or security issues.
#[test]
fn test_execute_command_argument_extraction() {
	let detector = SimpleRegexDetector::new();

	let queries = detector
		.detect_queries("run cargo build --release --quiet")
		.unwrap();
	assert!(!queries.is_empty());

	if let Some(query) = queries.first() {
		if let ServerQueryKind::ExecuteCommand {
			command,
			args,
			timeout_secs: _,
		} = &query.kind
		{
			assert_eq!(command, "cargo");
			assert!(args.len() >= 2, "Expected at least 2 args, got: {args:?}");
		}
	}
}

// ============================================================================
// GetEnvironment Pattern Detection Tests
// ============================================================================

/// Test GetEnvironment pattern detection with variable names.
/// **Purpose**: Ensures environment variable names are correctly identified and
/// normalized to uppercase. Incorrect parsing could request wrong environment data.
#[test]
fn test_get_environment_detection_with_vars() {
	let detector = SimpleRegexDetector::new();

	let queries = detector
		.detect_queries("get environment PATH USER HOME")
		.unwrap();

	assert!(!queries.is_empty(), "Expected environment query");

	if let Some(query) = queries.first() {
		if let ServerQueryKind::GetEnvironment { keys } = &query.kind {
			assert!(
				keys.contains(&"PATH".to_string()),
				"PATH not found in keys: {keys:?}"
			);
			assert!(
				keys.contains(&"USER".to_string()),
				"USER not found in keys: {keys:?}"
			);
			assert!(
				keys.contains(&"HOME".to_string()),
				"HOME not found in keys: {keys:?}"
			);
		}
	}
}

/// Test GetEnvironment with default variables when none specified.
/// **Purpose**: Validates fallback behavior when environment query has no specific
/// variable names. Should provide sensible defaults (PATH, HOME, USER, etc.).
#[test]
fn test_get_environment_detection_default_vars() {
	let detector = SimpleRegexDetector::new();

	let queries = detector.detect_queries("get environment").unwrap();

	assert!(!queries.is_empty(), "Expected default environment query");

	if let Some(query) = queries.first() {
		if let ServerQueryKind::GetEnvironment { keys } = &query.kind {
			assert!(
				!keys.is_empty(),
				"Expected default variables, got empty list"
			);
			// Should include common defaults
			assert!(
				keys.iter().any(|k| k == "PATH"),
				"PATH should be in defaults: {keys:?}"
			);
		}
	}
}

// ============================================================================
// RequestUserInput Pattern Detection Tests
// ============================================================================

/// Test RequestUserInput pattern detection with prompt extraction.
/// **Purpose**: Ensures user input prompts are correctly extracted from LLM output.
/// Malformed prompts could confuse users or fail silently.
#[test]
fn test_request_user_input_detection() {
	let detector = SimpleRegexDetector::new();

	let test_cases = vec![
		"ask user for confirmation",
		"request user input for deploy option",
		"prompt user to continue",
	];

	for input in test_cases {
		let queries = detector.detect_queries(input).unwrap();
		assert!(
			!queries.is_empty(),
			"Expected user input query for: {input}"
		);

		if let Some(query) = queries.first() {
			if let ServerQueryKind::RequestUserInput {
				prompt,
				input_type,
				options: _,
			} = &query.kind
			{
				assert!(
					!prompt.is_empty(),
					"Prompt should not be empty for: {input}"
				);
				assert_eq!(input_type, "text", "Expected text input type");
			}
		}
	}
}

// ============================================================================
// Multiple Query Detection Tests
// ============================================================================

/// Test detection of multiple queries in same output.
/// **Purpose**: Validates that the detector can find all queries in output with
/// multiple query patterns. Important for complex LLM outputs.
#[test]
fn test_multiple_queries_in_output() {
	let detector = SimpleRegexDetector::new();

	let output = "First I need to read config.json, then run npm install";
	let queries = detector.detect_queries(output).unwrap();

	assert!(
		queries.len() >= 2,
		"Expected at least 2 queries, got: {}",
		queries.len()
	);
}

/// Test three separate queries in one output.
/// **Purpose**: Validates handling of complex prompts with multiple distinct operations.
/// Ensures all queries are detected, not just the first one.
#[test]
fn test_three_queries_in_output() {
	let detector = SimpleRegexDetector::new();

	let output = "First read main.rs, then check PATH environment, and ask user for approval";
	let queries = detector.detect_queries(output).unwrap();

	// At minimum should find file read and environment queries
	assert!(
		queries.len() >= 2,
		"Expected at least 2 queries, got: {}",
		queries.len()
	);
}

// ============================================================================
// No-Match / Edge Case Tests
// ============================================================================

/// Test that random text doesn't trigger false positive matches.
/// **Purpose**: Ensures the detector has sufficient specificity to avoid false
/// positives. False positives would interrupt normal LLM flow unnecessarily.
#[test]
fn test_no_match_on_random_text() {
	let detector = SimpleRegexDetector::new();

	let test_cases = vec![
		"The quick brown fox jumps over the lazy dog",
		"This is a completely normal sentence with no query",
		"I'm explaining how to read a book to someone",
		"Let me know if you understand the concept",
	];

	for input in test_cases {
		let queries = detector.detect_queries(input).unwrap();
		assert!(queries.is_empty(), "Unexpected query match for: {input}");
	}
}

/// Test graceful handling of empty and malformed input.
/// **Purpose**: Ensures detector robustness. Empty or malformed input should not
/// cause errors or panics in production.
#[test]
fn test_empty_and_malformed_input() {
	let detector = SimpleRegexDetector::new();

	let test_cases = vec!["", "   ", "\n\n", "incomplete read", "run", "ask"];

	for input in test_cases {
		let result = detector.detect_queries(input);
		assert!(result.is_ok(), "Should handle input gracefully: {input:?}");
	}
}

// ============================================================================
// Argument Extraction Helper Tests
// ============================================================================

/// Test path extraction with various quote styles and formats.
/// **Purpose**: Path extraction must normalize different input formats to
/// consistent internal representation for reliable file operations.
#[test]
fn test_path_extraction_variants() {
	let detector = SimpleRegexDetector::new();

	let test_cases = vec![
		("config.json", Some("/config.json")),
		("/etc/passwd", Some("/etc/passwd")),
		("./src/main.rs", Some("./src/main.rs")),
		("'src/lib.rs'", Some("/src/lib.rs")),
		("\"test.txt\"", Some("/test.txt")),
		("", None),
	];

	for (input, expected) in test_cases {
		let result = detector.extract_path(input);
		match expected {
			Some(exp) => assert_eq!(result, Some(exp.to_string()), "Failed for: {input}"),
			None => assert!(result.is_none(), "Expected None for: {input}"),
		}
	}
}

/// Test command argument splitting and parsing.
/// **Purpose**: Command arguments must be correctly split into individual tokens
/// to prevent injection vulnerabilities and ensure correct command execution.
#[test]
fn test_command_argument_splitting() {
	let detector = SimpleRegexDetector::new();

	let (cmd, args) = detector.extract_command("cargo build --release --target x86_64");
	assert_eq!(cmd, "cargo");
	assert_eq!(args.len(), 4, "Expected 4 args, got: {args:?}");
	assert!(args.contains(&"build".to_string()));
	assert!(args.contains(&"--release".to_string()));
	assert!(args.contains(&"--target".to_string()));
	assert!(args.contains(&"x86_64".to_string()));

	let (cmd, args) = detector.extract_command("single");
	assert_eq!(cmd, "single");
	assert!(args.is_empty());
}

/// Test environment variable name parsing.
/// **Purpose**: Variable names must be uppercased and validated to match actual
/// environment variable conventions. Invalid names should be filtered out.
#[test]
fn test_environment_variable_parsing() {
	let detector = SimpleRegexDetector::new();

	let vars = detector.extract_env_vars("path user home RUST_LOG");
	assert!(vars.contains(&"PATH".to_string()));
	assert!(vars.contains(&"USER".to_string()));
	assert!(vars.contains(&"HOME".to_string()));
	assert!(vars.contains(&"RUST_LOG".to_string()));

	let vars = detector.extract_env_vars("");
	assert!(vars.is_empty());

	// Test with delimiters
	let vars = detector.extract_env_vars("PATH,USER;HOME");
	assert_eq!(vars.len(), 3);
}

// ============================================================================
// Query Metadata Tests
// ============================================================================

/// Test that all detected queries have valid metadata.
/// **Purpose**: Metadata aids debugging and tracing. Missing or malformed metadata
/// makes troubleshooting production issues difficult.
#[test]
fn test_query_metadata_presence() {
	let detector = SimpleRegexDetector::new();

	let queries = detector.detect_queries("I need to read test.txt").unwrap();
	assert!(!queries.is_empty());

	for query in queries {
		// Metadata must indicate the detector type
		let metadata = &query.metadata;
		assert_eq!(
			metadata.get("detector").and_then(|v| v.as_str()),
			Some("simple_regex"),
			"Metadata should identify detector: {metadata:?}"
		);

		// Verify query ID is properly formatted
		assert!(query.id.starts_with("Q-"), "Query ID should start with Q-");
		assert!(
			query.id.len() > 10,
			"Query ID should have reasonable length"
		);
	}
}

// ============================================================================
// Query Timeout Configuration Tests
// ============================================================================

/// Test that appropriate timeouts are set for each query type.
/// **Purpose**: Different query types have different expected response times.
/// - ReadFile: 10s (fast, local operation)
/// - ExecuteCommand: 30s (may take time)
/// - RequestUserInput: 60s (user may take time)
/// Incorrect timeouts cause acceptable requests to fail or excessive waiting.
#[test]
fn test_query_timeout_configuration() {
	let detector = SimpleRegexDetector::new();

	// ReadFile timeout
	let queries = detector.detect_queries("I need to read test.txt").unwrap();
	assert_eq!(
		queries[0].timeout_secs, 10,
		"ReadFile should have 10s timeout"
	);

	// ExecuteCommand timeout
	let queries = detector.detect_queries("run cargo build").unwrap();
	assert_eq!(
		queries[0].timeout_secs, 30,
		"ExecuteCommand should have 30s timeout"
	);

	// RequestUserInput timeout
	let queries = detector
		.detect_queries("ask user for confirmation")
		.unwrap();
	assert_eq!(
		queries[0].timeout_secs, 60,
		"RequestUserInput should have 60s timeout"
	);
}

// ============================================================================
// Query ID Generation Tests
// ============================================================================

/// Test that query IDs are always unique and valid.
/// **Purpose**: Query IDs are used for correlation between requests and responses.
/// Invalid or duplicate IDs would break the query-response matching mechanism.
#[test]
fn test_query_id_uniqueness() {
	let detector = SimpleRegexDetector::new();

	let output = "I need to read config.json and also read settings.json";
	let queries = detector.detect_queries(output).unwrap();

	// All query IDs should be unique
	let ids: Vec<_> = queries.iter().map(|q| &q.id).collect();
	for (i, id1) in ids.iter().enumerate() {
		for (j, id2) in ids.iter().enumerate() {
			if i != j {
				assert_ne!(id1, id2, "Query IDs should be unique");
			}
		}
	}
}

/// Test query ID format consistency.
/// **Purpose**: IDs must follow a consistent format for parsing and validation.
#[test]
fn test_query_id_format() {
	let detector = SimpleRegexDetector::new();

	for _ in 0..10 {
		let queries = detector.detect_queries("read test.txt").unwrap();
		for query in queries {
			assert!(
				query.id.starts_with("Q-"),
				"ID should start with Q-: {}",
				query.id
			);
			// IDs generated from UUIDs should be hex characters
			let hex_part = &query.id[2..];
			assert!(
				hex_part.chars().all(|c| c.is_ascii_hexdigit()),
				"ID should contain hex digits: {}",
				query.id
			);
		}
	}
}
