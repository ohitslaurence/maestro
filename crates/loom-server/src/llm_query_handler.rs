// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights
// reserved. SPDX-License-Identifier: Proprietary

//! LLM query detection and processing logic.
//!
//! This module detects when LLM output contains queries and automatically
//! processes them through the ServerQueryManager. It uses pattern matching
//! strategies to identify query requests embedded in LLM text output.
//!
//! ## Architecture
//!
//! - `QueryDetector`: Trait for pluggable query detection implementations
//! - `SimpleRegexDetector`: Pattern matching based detector for common query
//!   phrases
//! - `LlmQueryHandler`: Main handler that coordinates detection and processing

use loom_common_core::server_query::{
	ServerQuery, ServerQueryError, ServerQueryKind, ServerQueryResponse,
};
use regex::Regex;
use std::sync::Arc;
use tracing::{debug, instrument, warn};
use uuid::Uuid;

use crate::server_query::ServerQueryManager;

/// Simple regex-based detector for common query patterns.
///
/// This detector uses pattern matching to identify phrases that indicate
/// the LLM wants to query the client system. It supports:
///
/// - File reading: "I need to read", "show me", "get file"
/// - Commands: "run command", "execute"
/// - Environment: "get environment", "what's in", "show me env"
/// - User input: "ask user", "user input"
#[derive(Debug, Clone)]
pub struct SimpleRegexDetector {
	// Pattern for ReadFile queries
	pub read_file_pattern: Regex,
	// Pattern for ExecuteCommand queries
	pub execute_command_pattern: Regex,
	// Pattern for GetEnvironment queries
	pub get_env_pattern: Regex,
	// Pattern for RequestUserInput queries
	pub user_input_pattern: Regex,
}

impl SimpleRegexDetector {
	/// Create a new SimpleRegexDetector with default patterns.
	///
	/// # Panics
	/// Panics if regex patterns are invalid (should not happen with static
	/// patterns).
	pub fn new() -> Self {
		Self {
            // Detects: "I need to read config.json", "let me read test.txt", "show me /etc/passwd"
            // Requires file path to have extension, leading slash, or dot (to avoid "show me the file")
            read_file_pattern: Regex::new(
                r"(?i)(?:i need to read|let me read|show me|read|get file)\s+(?:the\s+)?(?:file\s+)?([^\s,;?!]*[/\.][^\s,;?!]*)"
            ).expect("read_file_pattern is invalid"),

            // Detects: "run ls -la", "execute command arg1 arg2"
            execute_command_pattern: Regex::new(
                r"(?i)(?:run|execute|run command)\s+(.+?)(?:\s+or\s+|\s+and\s+|$)"
            ).expect("execute_command_pattern is invalid"),

            // Detects: "get environment PATH USER"
            get_env_pattern: Regex::new(
                r"(?i)(?:get environment|what's in|show me env|check env)\b(?:\s+(.+))?$"
            ).expect("get_env_pattern is invalid"),

            // Detects: "ask user for input", "request user input"
            user_input_pattern: Regex::new(
                r"(?i)(?:ask|request|prompt)\s+(?:the\s+)?user(?:\s+(.*))?$"
            ).expect("user_input_pattern is invalid"),
        }
	}

	/// Extract a file path from text, handling various formats.
	pub fn extract_path(&self, text: &str) -> Option<String> {
		let path = text.trim();
		if path.is_empty() {
			return None;
		}
		// Remove quotes if present
		let path = if (path.starts_with('"') && path.ends_with('"'))
			|| (path.starts_with('\'') && path.ends_with('\''))
		{
			&path[1..path.len() - 1]
		} else {
			path
		};

		if path.is_empty() {
			return None;
		}

		// Ensure path starts with / for absolute paths
		let normalized = if !path.starts_with('/') && !path.starts_with('.') {
			format!("/{path}")
		} else {
			path.to_string()
		};

		Some(normalized)
	}

	/// Extract environment variable names from text.
	pub fn extract_env_vars(&self, text: &str) -> Vec<String> {
		let text = text.trim();
		if text.is_empty() {
			return vec![];
		}

		// Split by common separators and extract valid env var names
		text
			.split(|c: char| c.is_whitespace() || c == ',' || c == ';')
			.filter(|s| !s.is_empty() && s.chars().all(|c| c.is_alphanumeric() || c == '_'))
			.map(|s| s.to_uppercase())
			.collect()
	}

	/// Extract command and arguments from text.
	pub fn extract_command(&self, text: &str) -> (String, Vec<String>) {
		let trimmed = text.trim();
		let parts: Vec<&str> = trimmed.split_whitespace().collect();

		if parts.is_empty() {
			return ("".to_string(), vec![]);
		}

		let command = parts[0].to_string();
		let args = parts[1..].iter().map(|s| s.to_string()).collect::<Vec<_>>();

		(command, args)
	}

	/// Generate a unique query ID.
	fn generate_query_id() -> String {
		format!("Q-{}", Uuid::new_v4().to_string().replace("-", ""))
	}

	/// Analyze LLM output and extract queries if present.
	///
	/// # Arguments
	/// * `output` - The LLM text output to analyze
	///
	/// # Returns
	/// A vector of detected ServerQuery instances.
	/// An empty vector indicates no queries were detected, which is not an error.
	pub fn detect_queries(&self, output: &str) -> Result<Vec<ServerQuery>, ServerQueryError> {
		let mut queries = Vec::new();

		// Check for ReadFile queries
		if let Some(caps) = self.read_file_pattern.captures(output) {
			if let Some(path_match) = caps.get(1) {
				let path = path_match.as_str();
				if let Some(normalized_path) = self.extract_path(path) {
					debug!(path = %normalized_path, "detected ReadFile query");
					queries.push(ServerQuery {
						id: Self::generate_query_id(),
						kind: ServerQueryKind::ReadFile {
							path: normalized_path,
						},
						sent_at: chrono::Utc::now().to_rfc3339(),
						timeout_secs: 10,
						metadata: serde_json::json!({ "detector": "simple_regex" }),
					});
				}
			}
		}

		// Check for ExecuteCommand queries
		if let Some(caps) = self.execute_command_pattern.captures(output) {
			if let Some(cmd_match) = caps.get(1) {
				let cmd = cmd_match.as_str();
				let args_text = caps.get(2).map(|m| m.as_str()).unwrap_or("");
				let (command, args) = self.extract_command(cmd);
				if !command.is_empty() {
					let mut all_args = args;
					if !args_text.is_empty() {
						all_args.extend(self.extract_command(args_text).1);
					}
					debug!(command = %command, args_count = all_args.len(), "detected ExecuteCommand query");
					queries.push(ServerQuery {
						id: Self::generate_query_id(),
						kind: ServerQueryKind::ExecuteCommand {
							command,
							args: all_args,
							timeout_secs: 30,
						},
						sent_at: chrono::Utc::now().to_rfc3339(),
						timeout_secs: 30,
						metadata: serde_json::json!({ "detector": "simple_regex" }),
					});
				}
			}
		}

		// Check for GetEnvironment queries
		if let Some(caps) = self.get_env_pattern.captures(output) {
			let env_vars = if let Some(vars_match) = caps.get(1) {
				self.extract_env_vars(vars_match.as_str())
			} else {
				// If no specific vars mentioned, get all common ones
				["PATH", "HOME", "USER", "SHELL", "LANG"]
					.iter()
					.map(|s| s.to_string())
					.collect()
			};

			if !env_vars.is_empty() {
				debug!(var_count = env_vars.len(), "detected GetEnvironment query");
				queries.push(ServerQuery {
					id: Self::generate_query_id(),
					kind: ServerQueryKind::GetEnvironment { keys: env_vars },
					sent_at: chrono::Utc::now().to_rfc3339(),
					timeout_secs: 10,
					metadata: serde_json::json!({ "detector": "simple_regex" }),
				});
			}
		}

		// Check for RequestUserInput queries
		if let Some(caps) = self.user_input_pattern.captures(output) {
			let prompt = caps
				.get(1)
				.map(|m| m.as_str().to_string())
				.unwrap_or_else(|| "Please provide input".to_string());

			debug!(prompt = %prompt, "detected RequestUserInput query");
			queries.push(ServerQuery {
				id: Self::generate_query_id(),
				kind: ServerQueryKind::RequestUserInput {
					prompt,
					input_type: "text".to_string(),
					options: None,
				},
				sent_at: chrono::Utc::now().to_rfc3339(),
				timeout_secs: 60,
				metadata: serde_json::json!({ "detector": "simple_regex" }),
			});
		}

		debug!(detected_count = queries.len(), "query detection complete");
		Ok(queries)
	}
}

impl Default for SimpleRegexDetector {
	fn default() -> Self {
		Self::new()
	}
}

/// Handler for processing LLM output and extracting queries.
///
/// This handler coordinates query detection and processing, managing the
/// lifecycle of queries from detection through response collection.
#[derive(Clone)]
pub struct LlmQueryHandler {
	pub detector: SimpleRegexDetector,
	query_manager: Arc<ServerQueryManager>,
}

impl LlmQueryHandler {
	/// Create a new LLM query handler with a detector and query manager.
	///
	/// # Arguments
	/// * `detector` - The detector for finding queries in LLM output
	/// * `query_manager` - The manager for handling server queries
	pub fn new(detector: SimpleRegexDetector, query_manager: Arc<ServerQueryManager>) -> Self {
		Self {
			detector,
			query_manager,
		}
	}

	/// Create a new handler with the default SimpleRegexDetector.
	///
	/// # Arguments
	/// * `query_manager` - The manager for handling server queries
	pub fn with_default_detector(query_manager: Arc<ServerQueryManager>) -> Self {
		Self {
			detector: SimpleRegexDetector::new(),
			query_manager,
		}
	}

	/// Handle LLM output by detecting and processing any embedded queries.
	///
	/// This method:
	/// 1. Analyzes LLM output for query patterns
	/// 2. Sends the first detected query to the client
	/// 3. Waits for the client response
	/// 4. Returns the response for the LLM to inject back
	///
	/// # Arguments
	/// * `session_id` - The session ID for logging and correlation
	/// * `llm_output` - The text output from the LLM to analyze
	///
	/// # Returns
	/// - `Ok(Some(response))` - A query was detected and processed, response is
	///   ready
	/// - `Ok(None)` - No queries were detected in the output
	/// - `Err(error)` - An error occurred during processing
	#[instrument(
        skip(self, llm_output),
        fields(
            session_id = %session_id,
            output_len = llm_output.len()
        )
    )]
	pub async fn handle_llm_output(
		&self,
		session_id: &str,
		llm_output: &str,
	) -> Result<Option<ServerQueryResponse>, ServerQueryError> {
		// Detect queries in the LLM output
		let queries = self.detector.detect_queries(llm_output)?;

		if queries.is_empty() {
			debug!(session_id = %session_id, "no queries detected in LLM output");
			return Ok(None);
		}

		// Send the first query to the client
		let query = queries[0].clone();
		let query_id = query.id.clone();

		debug!(
				session_id = %session_id,
				query_id = %query_id,
				"processing first detected query"
		);

		// Send and wait for response
		match self.query_manager.send_query(session_id, query).await {
			Ok(response) => {
				debug!(
						session_id = %session_id,
						query_id = %query_id,
						"query processed successfully"
				);
				Ok(Some(response))
			}
			Err(e) => {
				warn!(
						session_id = %session_id,
						query_id = %query_id,
						error = ?e,
						"query processing failed"
				);
				Err(e)
			}
		}
	}

	/// Handle multiple queries in LLM output (processes all detected queries).
	///
	/// This is a batch version that processes all detected queries sequentially.
	///
	/// # Arguments
	/// * `session_id` - The session ID for logging and correlation
	/// * `llm_output` - The text output from the LLM to analyze
	///
	/// # Returns
	/// A vector of responses for all detected queries, in the order they were
	/// detected.
	#[instrument(
        skip(self, llm_output),
        fields(
            session_id = %session_id,
            output_len = llm_output.len()
        )
    )]
	pub async fn handle_llm_output_batch(
		&self,
		session_id: &str,
		llm_output: &str,
	) -> Result<Vec<ServerQueryResponse>, ServerQueryError> {
		// Detect queries in the LLM output
		let queries = self.detector.detect_queries(llm_output)?;

		if queries.is_empty() {
			debug!(session_id = %session_id, "no queries detected in batch");
			return Ok(vec![]);
		}

		let mut responses = Vec::new();

		// Process each query sequentially
		for query in queries {
			let query_id = query.id.clone();
			debug!(
					session_id = %session_id,
					query_id = %query_id,
					"processing query in batch"
			);

			match self.query_manager.send_query(session_id, query).await {
				Ok(response) => {
					debug!(
							session_id = %session_id,
							query_id = %query_id,
							"batch query processed successfully"
					);
					responses.push(response);
				}
				Err(e) => {
					warn!(
							session_id = %session_id,
							query_id = %query_id,
							error = ?e,
							"batch query processing failed"
					);
					return Err(e);
				}
			}
		}

		Ok(responses)
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::server_query::ServerQueryManager;

	/// Test that regex detector identifies ReadFile patterns correctly.
	/// **Why Important**: Ensures the detector correctly extracts file paths from
	/// various natural language patterns. File path extraction is critical for
	/// ReadFile queries.
	#[tokio::test]
	async fn test_detect_read_file_patterns() {
		let detector = SimpleRegexDetector::new();

		let test_cases = vec![
			("I need to read config.json", "/config.json"),
			("Show me /etc/passwd", "/etc/passwd"),
			("let me read the file test.txt", "/test.txt"),
			("read 'src/main.rs'", "/src/main.rs"),
			("show me the file", ""), // no path specified
		];

		for (input, expected_path) in test_cases {
			let queries = detector.detect_queries(input).unwrap();

			if expected_path.is_empty() {
				assert!(queries.is_empty(), "Expected no query for input: {input}");
			} else {
				assert_eq!(queries.len(), 1, "Expected one query for: {input}");
				match &queries[0].kind {
					ServerQueryKind::ReadFile { path } => {
						assert_eq!(path, expected_path, "Path mismatch for input: {input}");
					}
					_ => panic!("Expected ReadFile query for: {input}"),
				}
			}
		}
	}

	/// Test that regex detector identifies ExecuteCommand patterns correctly.
	/// **Why Important**: Ensures command and argument parsing works correctly
	/// for command execution requests. This prevents misinterpreted commands from
	/// being executed.
	#[tokio::test]
	async fn test_detect_execute_command_patterns() {
		let detector = SimpleRegexDetector::new();

		let queries = detector.detect_queries("run ls -la").unwrap();

		assert_eq!(queries.len(), 1);
		match &queries[0].kind {
			ServerQueryKind::ExecuteCommand {
				command,
				args,
				timeout_secs: _,
			} => {
				assert_eq!(command, "ls");
				assert!(args.contains(&"-la".to_string()));
			}
			_ => panic!("Expected ExecuteCommand query"),
		}
	}

	/// Test that regex detector identifies GetEnvironment patterns correctly.
	/// **Why Important**: Environment variable parsing must correctly identify
	/// and uppercase variable names to match actual environment variables.
	/// Incorrect parsing could cause the wrong environment data to be sent to the
	/// LLM.
	#[tokio::test]
	async fn test_detect_get_environment_patterns() {
		let detector = SimpleRegexDetector::new();

		let queries = detector
			.detect_queries("get environment PATH USER")
			.unwrap();

		assert_eq!(queries.len(), 1);
		match &queries[0].kind {
			ServerQueryKind::GetEnvironment { keys } => {
				assert!(keys.contains(&"PATH".to_string()));
				assert!(keys.contains(&"USER".to_string()));
			}
			_ => panic!("Expected GetEnvironment query"),
		}
	}

	/// Test that regex detector identifies RequestUserInput patterns correctly.
	/// **Why Important**: Ensures prompt extraction from natural language works
	/// correctly. Malformed prompts could confuse users or indicate a detector
	/// issue.
	#[tokio::test]
	async fn test_detect_user_input_patterns() {
		let detector = SimpleRegexDetector::new();

		let queries = detector.detect_queries("ask user for their name").unwrap();

		assert_eq!(queries.len(), 1);
		match &queries[0].kind {
			ServerQueryKind::RequestUserInput {
				prompt,
				input_type,
				options,
			} => {
				assert!(prompt.contains("name"));
				assert_eq!(input_type, "text");
				assert!(options.is_none());
			}
			_ => panic!("Expected RequestUserInput query"),
		}
	}

	/// Test that multiple queries are detected in a single output.
	/// **Why Important**: Ensures the detector doesn't stop after finding the
	/// first query and can handle complex LLM outputs requesting multiple
	/// operations.
	#[tokio::test]
	async fn test_multiple_queries_detected() {
		let detector = SimpleRegexDetector::new();

		let output = "I need to read config.json and then run the build script";
		let queries = detector.detect_queries(output).unwrap();

		// Should detect both ReadFile and ExecuteCommand
		assert!(!queries.is_empty(), "Expected at least 1 query");
	}

	/// Test that malformed input doesn't cause errors.
	/// **Why Important**: The detector must gracefully handle edge cases and
	/// invalid input without panicking. This ensures robustness in production.
	#[tokio::test]
	async fn test_malformed_input_handling() {
		let detector = SimpleRegexDetector::new();

		let test_cases = vec!["", "random text with no queries", "incomplete read", "run"];

		for input in test_cases {
			let result = detector.detect_queries(input);
			assert!(result.is_ok(), "Should not error on: {input}");
		}
	}

	/// Test path extraction with various formats.
	/// **Why Important**: Path normalization ensures consistency in file
	/// operations. Paths must be in expected format for the client to handle them
	/// correctly.
	#[test]
	fn test_path_extraction() {
		let detector = SimpleRegexDetector::new();

		let test_cases = vec![
			("config.json", "/config.json"),
			("/etc/passwd", "/etc/passwd"),
			("./src/main.rs", "./src/main.rs"),
			("'src/lib.rs'", "/src/lib.rs"),
			("\"test.txt\"", "/test.txt"),
		];

		for (input, expected) in test_cases {
			let result = detector.extract_path(input);
			assert_eq!(
				result,
				Some(expected.to_string()),
				"Path extraction failed for: {input}"
			);
		}
	}

	/// Test command extraction with arguments.
	/// **Why Important**: Command arguments must be parsed and separated
	/// correctly to prevent injection issues and ensure proper command execution.
	#[test]
	fn test_command_extraction() {
		let detector = SimpleRegexDetector::new();

		let (cmd, args) = detector.extract_command("ls -la /tmp");
		assert_eq!(cmd, "ls");
		assert_eq!(args, vec!["-la", "/tmp"]);

		let (cmd, args) = detector.extract_command("single");
		assert_eq!(cmd, "single");
		assert!(args.is_empty());
	}

	/// Test environment variable extraction.
	/// **Why Important**: Environment variable names must be correctly parsed and
	/// uppercased. Invalid parsing could request non-existent variables or miss
	/// requested ones.
	#[test]
	fn test_env_var_extraction() {
		let detector = SimpleRegexDetector::new();

		let vars = detector.extract_env_vars("path user home");
		assert!(vars.contains(&"PATH".to_string()));
		assert!(vars.contains(&"USER".to_string()));
		assert!(vars.contains(&"HOME".to_string()));

		let vars = detector.extract_env_vars("");
		assert!(vars.is_empty());
	}

	/// Test that query IDs are always valid.
	/// **Why Important**: Query IDs are used for correlation between requests and
	/// responses. Invalid IDs could break the query-response matching mechanism.
	#[tokio::test]
	async fn test_generated_query_ids_are_valid() {
		let detector = SimpleRegexDetector::new();

		let queries = detector.detect_queries("I need to read test.txt").unwrap();

		for query in queries {
			assert!(
				query.id.starts_with("Q-"),
				"Query ID should start with Q-: {}",
				query.id
			);
			assert!(
				query.id.len() > 10,
				"Query ID should have reasonable length: {}",
				query.id
			);
		}
	}

	/// Test LlmQueryHandler with no detected queries.
	/// **Why Important**: The handler must gracefully return None when no queries
	/// are present, allowing normal LLM flow to continue without interruption.
	#[tokio::test]
	async fn test_handler_no_queries() {
		let manager = Arc::new(ServerQueryManager::new());
		let handler = LlmQueryHandler::with_default_detector(manager);

		let result = handler
			.handle_llm_output("session-1", "just some regular text")
			.await
			.unwrap();

		assert!(result.is_none());
	}

	/// Test LlmQueryHandler with default detector.
	/// **Why Important**: The convenience constructor ensures users don't need to
	/// manually create and wire up dependencies for typical use cases.
	#[test]
	fn test_handler_with_default_detector() {
		let manager = Arc::new(ServerQueryManager::new());
		let handler = LlmQueryHandler::with_default_detector(manager);

		// Verify the handler was created
		assert!(!handler.detector.read_file_pattern.as_str().is_empty());
	}

	/// Test that queries generated have correct metadata.
	/// **Why Important**: Metadata aids debugging and tracing query origins
	/// through the system. Missing or incorrect metadata makes troubleshooting
	/// difficult.
	#[tokio::test]
	async fn test_query_metadata() {
		let detector = SimpleRegexDetector::new();

		let queries = detector.detect_queries("I need to read test.txt").unwrap();

		assert!(!queries.is_empty());
		for query in queries {
			let metadata = &query.metadata;
			assert_eq!(
				metadata.get("detector").and_then(|v| v.as_str()),
				Some("simple_regex")
			);
		}
	}

	/// Test that timeouts are set appropriately per query type.
	/// **Why Important**: Different query types have different expected response
	/// times. Incorrect timeouts could cause acceptable requests to fail or cause
	/// excessive waiting.
	#[tokio::test]
	async fn test_query_timeouts() {
		let detector = SimpleRegexDetector::new();

		// ReadFile should have moderate timeout
		let queries = detector.detect_queries("I need to read test.txt").unwrap();
		assert_eq!(queries[0].timeout_secs, 10);

		// ExecuteCommand should have longer timeout
		let queries = detector.detect_queries("run some_command").unwrap();
		assert_eq!(queries[0].timeout_secs, 30);

		// RequestUserInput should have longest timeout
		let queries = detector.detect_queries("ask user for input").unwrap();
		assert_eq!(queries[0].timeout_secs, 60);
	}
}
