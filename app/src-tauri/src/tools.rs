//! Tool execution and classification.
//!
//! This module provides the ToolRunner for normalizing tool calls and
//! marking mutating tools. See specs/agent-state-machine.md §3 for the full spec.

use crate::agent_state::{ToolCall, ToolRunRecord, ToolRunStatus};

// ============================================================================
// Mutating Tool Classification (§3)
// ============================================================================

/// Initial mutating tool list per spec §3.
///
/// Mutating tools are those that modify the filesystem, execute commands,
/// or perform git operations. Post-tool hooks run only if any tool in the
/// batch is mutating.
const MUTATING_TOOLS: &[&str] = &[
    // File edits
    "edit_file",
    "write_file",
    "apply_patch",
    // Shell execution
    "bash",
    "run_command",
    // Git operations (git_* prefix)
    "git_commit",
    "git_checkout",
    "git_apply",
    "git_add",
    "git_reset",
    "git_stash",
    "git_push",
    "git_pull",
    "git_merge",
    "git_rebase",
];

/// Non-mutating tools per spec §3.
/// These tools are read-only and do not trigger post-tool hooks.
#[allow(dead_code)]
const NON_MUTATING_TOOLS: &[&str] = &["read_file", "list_files", "grep", "search"];

/// Classify whether a tool is mutating based on its name.
///
/// Per spec §3:
/// - Default: tools are non-mutating unless marked otherwise.
/// - Mutating tool categories: file edits, shell execution, git operations.
/// - Harness adapters may override classification when tools are unknown.
pub fn is_mutating_tool(tool_name: &str) -> bool {
    // Check exact match in mutating list
    if MUTATING_TOOLS.contains(&tool_name) {
        return true;
    }

    // Check git_* prefix for any git operation
    if tool_name.starts_with("git_") {
        return true;
    }

    // Default: non-mutating
    false
}

// ============================================================================
// Tool Normalization (§3)
// ============================================================================

/// Normalize a raw tool call by setting the mutating flag based on classification.
///
/// This ensures all tool calls have their mutating flag properly set before
/// execution, regardless of what the harness reported.
pub fn normalize_tool_call(mut call: ToolCall) -> ToolCall {
    call.mutating = is_mutating_tool(&call.name);
    call
}

/// Normalize a batch of tool calls.
pub fn normalize_tool_calls(calls: Vec<ToolCall>) -> Vec<ToolCall> {
    calls.into_iter().map(normalize_tool_call).collect()
}

// ============================================================================
// Tool Run Record Creation (§3)
// ============================================================================

/// Create a new ToolRunRecord for a tool call.
///
/// Per spec §3, tool run IDs follow the format `toolrun_<uuid>`.
/// Records start in Queued status with attempt 1.
pub fn create_tool_run_record(call: &ToolCall) -> ToolRunRecord {
    ToolRunRecord {
        run_id: format!("toolrun_{}", uuid::Uuid::new_v4()),
        call_id: call.call_id.clone(),
        tool_name: call.name.clone(),
        mutating: call.mutating,
        status: ToolRunStatus::Queued,
        started_at_ms: 0,
        finished_at_ms: None,
        attempt: 1,
        error: None,
    }
}

/// Create tool run records for a batch of tool calls.
///
/// Returns the normalized calls and their corresponding run records.
pub fn prepare_tool_batch(calls: Vec<ToolCall>) -> (Vec<ToolCall>, Vec<ToolRunRecord>) {
    let normalized = normalize_tool_calls(calls);
    let records: Vec<ToolRunRecord> = normalized.iter().map(create_tool_run_record).collect();
    (normalized, records)
}

/// Check if a batch of tool runs contains any mutating tools.
///
/// Per spec §5, post-tool hooks run only if any tool in the batch is mutating.
pub fn batch_has_mutating_tools(records: &[ToolRunRecord]) -> bool {
    records.iter().any(|r| r.mutating)
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mutating_tools_classified_correctly() {
        // File edits
        assert!(is_mutating_tool("edit_file"));
        assert!(is_mutating_tool("write_file"));
        assert!(is_mutating_tool("apply_patch"));

        // Shell execution
        assert!(is_mutating_tool("bash"));
        assert!(is_mutating_tool("run_command"));

        // Git operations
        assert!(is_mutating_tool("git_commit"));
        assert!(is_mutating_tool("git_checkout"));
        assert!(is_mutating_tool("git_apply"));
    }

    #[test]
    fn non_mutating_tools_classified_correctly() {
        assert!(!is_mutating_tool("read_file"));
        assert!(!is_mutating_tool("list_files"));
        assert!(!is_mutating_tool("grep"));
        assert!(!is_mutating_tool("search"));
    }

    #[test]
    fn unknown_git_prefix_is_mutating() {
        // Any git_* tool should be considered mutating
        assert!(is_mutating_tool("git_unknown_command"));
        assert!(is_mutating_tool("git_branch"));
        assert!(is_mutating_tool("git_tag"));
    }

    #[test]
    fn unknown_tools_default_to_non_mutating() {
        assert!(!is_mutating_tool("unknown_tool"));
        assert!(!is_mutating_tool("fetch_url"));
        assert!(!is_mutating_tool("analyze_code"));
    }

    #[test]
    fn normalize_tool_call_sets_mutating_flag() {
        let call = ToolCall {
            call_id: "call_1".to_string(),
            name: "edit_file".to_string(),
            arguments: serde_json::json!({"path": "foo.rs"}),
            mutating: false, // Harness may report incorrectly
        };

        let normalized = normalize_tool_call(call);
        assert!(normalized.mutating);
    }

    #[test]
    fn normalize_tool_call_clears_mutating_for_read_only() {
        let call = ToolCall {
            call_id: "call_1".to_string(),
            name: "read_file".to_string(),
            arguments: serde_json::json!({"path": "foo.rs"}),
            mutating: true, // Harness may report incorrectly
        };

        let normalized = normalize_tool_call(call);
        assert!(!normalized.mutating);
    }

    #[test]
    fn create_tool_run_record_generates_valid_id() {
        let call = ToolCall {
            call_id: "call_123".to_string(),
            name: "bash".to_string(),
            arguments: serde_json::json!({"cmd": "ls"}),
            mutating: true,
        };

        let record = create_tool_run_record(&call);

        assert!(record.run_id.starts_with("toolrun_"));
        assert_eq!(record.call_id, "call_123");
        assert_eq!(record.tool_name, "bash");
        assert!(record.mutating);
        assert_eq!(record.status, ToolRunStatus::Queued);
        assert_eq!(record.attempt, 1);
    }

    #[test]
    fn prepare_tool_batch_normalizes_and_creates_records() {
        let calls = vec![
            ToolCall {
                call_id: "call_1".to_string(),
                name: "read_file".to_string(),
                arguments: serde_json::json!({}),
                mutating: true, // Wrong
            },
            ToolCall {
                call_id: "call_2".to_string(),
                name: "edit_file".to_string(),
                arguments: serde_json::json!({}),
                mutating: false, // Wrong
            },
        ];

        let (normalized, records) = prepare_tool_batch(calls);

        assert_eq!(normalized.len(), 2);
        assert!(!normalized[0].mutating); // read_file corrected
        assert!(normalized[1].mutating); // edit_file corrected

        assert_eq!(records.len(), 2);
        assert!(!records[0].mutating);
        assert!(records[1].mutating);
    }

    #[test]
    fn batch_has_mutating_tools_detects_mutating() {
        let records = vec![
            ToolRunRecord {
                run_id: "toolrun_1".to_string(),
                call_id: "call_1".to_string(),
                tool_name: "read_file".to_string(),
                mutating: false,
                status: ToolRunStatus::Queued,
                started_at_ms: 0,
                finished_at_ms: None,
                attempt: 1,
                error: None,
            },
            ToolRunRecord {
                run_id: "toolrun_2".to_string(),
                call_id: "call_2".to_string(),
                tool_name: "edit_file".to_string(),
                mutating: true,
                status: ToolRunStatus::Queued,
                started_at_ms: 0,
                finished_at_ms: None,
                attempt: 1,
                error: None,
            },
        ];

        assert!(batch_has_mutating_tools(&records));
    }

    #[test]
    fn batch_has_mutating_tools_returns_false_for_read_only() {
        let records = vec![ToolRunRecord {
            run_id: "toolrun_1".to_string(),
            call_id: "call_1".to_string(),
            tool_name: "read_file".to_string(),
            mutating: false,
            status: ToolRunStatus::Queued,
            started_at_ms: 0,
            finished_at_ms: None,
            attempt: 1,
            error: None,
        }];

        assert!(!batch_has_mutating_tools(&records));
    }
}
