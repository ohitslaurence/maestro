//! Post-tool hook runner.
//!
//! This module implements the hook execution pipeline that runs after mutating tool batches.
//! See specs/agent-state-machine.md sections 2, 3, 5, and 6 for the full specification.

use std::path::PathBuf;
use std::process::Stdio;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::time::timeout;

use crate::agent_state::{
    HookConfig, HookFailurePolicy, HookRunRecord, HookRunStatus, HookToolFilter, HooksConfig,
    ToolRunRecord,
};

// ============================================================================
// Hook Configuration Loading (§2)
// ============================================================================

/// Load hook configuration from the app data directory.
///
/// Per spec §2:
/// - Path: `app_data_dir()/hooks.json`
/// - If missing: hooks disabled (non-fatal, returns empty config)
/// - If invalid: returns error with code `hook_config_invalid`
pub fn load_hooks_config(app_data_dir: &PathBuf) -> Result<HooksConfig, HookConfigError> {
    let path = app_data_dir.join("hooks.json");

    if !path.exists() {
        // Missing file is not an error; hooks are simply disabled
        return Ok(HooksConfig::default());
    }

    let content = std::fs::read_to_string(&path).map_err(|e| HookConfigError {
        code: "hook_config_read_failed".to_string(),
        message: format!("Failed to read hooks.json: {}", e),
    })?;

    serde_json::from_str(&content).map_err(|e| HookConfigError {
        code: "hook_config_invalid".to_string(),
        message: format!("Failed to parse hooks.json: {}", e),
    })
}

/// Error loading hook configuration.
#[derive(Debug, Clone)]
pub struct HookConfigError {
    pub code: String,
    pub message: String,
}

// ============================================================================
// Hook Filtering (§3, §5)
// ============================================================================

/// Filter hooks that should run based on the completed tool runs.
///
/// Per spec §5.4 (Post-Tool Hook Flow):
/// - Filter hooks by `HookToolFilter`
/// - Execute hooks in configuration order
pub fn filter_hooks_for_batch<'a>(
    hooks: &'a [HookConfig],
    tool_runs: &[ToolRunRecord],
) -> Vec<&'a HookConfig> {
    hooks
        .iter()
        .filter(|hook| hook_matches_filter(hook, tool_runs))
        .collect()
}

/// Check if a hook's filter matches the given tool runs.
fn hook_matches_filter(hook: &HookConfig, tool_runs: &[ToolRunRecord]) -> bool {
    match &hook.tool_filter {
        HookToolFilter::AnyMutating => {
            // Run if any tool in the batch is mutating
            tool_runs.iter().any(|r| r.mutating)
        }
        HookToolFilter::ToolNames(names) => {
            // Run if any tool in the batch matches one of the specified names
            tool_runs
                .iter()
                .any(|r| names.contains(&r.tool_name))
        }
    }
}

/// Get tool run IDs that triggered a hook (for HookRunRecord.tool_run_ids).
pub fn get_triggering_tool_run_ids(hook: &HookConfig, tool_runs: &[ToolRunRecord]) -> Vec<String> {
    match &hook.tool_filter {
        HookToolFilter::AnyMutating => tool_runs
            .iter()
            .filter(|r| r.mutating)
            .map(|r| r.run_id.clone())
            .collect(),
        HookToolFilter::ToolNames(names) => tool_runs
            .iter()
            .filter(|r| names.contains(&r.tool_name))
            .map(|r| r.run_id.clone())
            .collect(),
    }
}

// ============================================================================
// Hook Execution (§5, §6)
// ============================================================================

/// Result of executing a single hook.
#[derive(Debug, Clone)]
pub struct HookExecutionResult {
    pub status: HookRunStatus,
    pub output: String,
    pub error: Option<String>,
    pub started_at_ms: u64,
    pub finished_at_ms: u64,
}

/// Execute a single hook command.
///
/// Per spec §2.3 (Hook Pipeline):
/// - Hook execution runs in the session workspace root
/// - Hook environment is restricted to allowlisted keys
/// - Hook output is captured (no streaming to LLM)
/// - Timeout enforcement per `HookConfig.timeout_ms`
pub async fn execute_hook(
    config: &HookConfig,
    workspace_root: &PathBuf,
) -> HookExecutionResult {
    let started_at_ms = current_time_ms();

    if config.command.is_empty() {
        return HookExecutionResult {
            status: HookRunStatus::Failed,
            output: String::new(),
            error: Some("Hook command is empty".to_string()),
            started_at_ms,
            finished_at_ms: current_time_ms(),
        };
    }

    let (program, args) = (&config.command[0], &config.command[1..]);

    // Build command with restricted environment
    let mut cmd = Command::new(program);
    cmd.args(args)
        .current_dir(workspace_root)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env_clear(); // Per spec: restricted environment

    // Add allowlisted environment variables
    for (key, value) in allowed_env_vars() {
        cmd.env(key, value);
    }

    // Spawn the process
    let child = match cmd.spawn() {
        Ok(child) => child,
        Err(e) => {
            return HookExecutionResult {
                status: HookRunStatus::Failed,
                output: String::new(),
                error: Some(format!("Failed to spawn hook process: {}", e)),
                started_at_ms,
                finished_at_ms: current_time_ms(),
            };
        }
    };

    // Wait for completion with timeout
    let timeout_duration = Duration::from_millis(config.timeout_ms);
    let result = timeout(timeout_duration, wait_for_hook(child)).await;

    match result {
        Ok(Ok((output, exit_code))) => {
            let status = if exit_code == 0 {
                HookRunStatus::Succeeded
            } else {
                HookRunStatus::Failed
            };
            let error = if exit_code != 0 {
                Some(format!("Hook exited with code {}", exit_code))
            } else {
                None
            };

            HookExecutionResult {
                status,
                output,
                error,
                started_at_ms,
                finished_at_ms: current_time_ms(),
            }
        }
        Ok(Err(e)) => HookExecutionResult {
            status: HookRunStatus::Failed,
            output: String::new(),
            error: Some(format!("Hook execution error: {}", e)),
            started_at_ms,
            finished_at_ms: current_time_ms(),
        },
        Err(_) => {
            // Timeout elapsed
            HookExecutionResult {
                status: HookRunStatus::Failed,
                output: String::new(),
                error: Some(format!(
                    "Hook timed out after {} ms",
                    config.timeout_ms
                )),
                started_at_ms,
                finished_at_ms: current_time_ms(),
            }
        }
    }
}

/// Wait for hook process to complete and capture output.
async fn wait_for_hook(
    mut child: tokio::process::Child,
) -> Result<(String, i32), String> {
    let stdout = child
        .stdout
        .take()
        .ok_or("Failed to capture stdout")?;
    let stderr = child
        .stderr
        .take()
        .ok_or("Failed to capture stderr")?;

    // Capture stdout and stderr concurrently
    let stdout_handle = tokio::spawn(async move {
        let mut reader = BufReader::new(stdout).lines();
        let mut lines = Vec::new();
        while let Ok(Some(line)) = reader.next_line().await {
            lines.push(line);
        }
        lines
    });

    let stderr_handle = tokio::spawn(async move {
        let mut reader = BufReader::new(stderr).lines();
        let mut lines = Vec::new();
        while let Ok(Some(line)) = reader.next_line().await {
            lines.push(format!("[stderr] {}", line));
        }
        lines
    });

    // Wait for both readers to complete
    let stdout_lines = stdout_handle
        .await
        .map_err(|e| format!("Failed to join stdout task: {}", e))?;
    let stderr_lines = stderr_handle
        .await
        .map_err(|e| format!("Failed to join stderr task: {}", e))?;

    // Combine output
    let mut output = stdout_lines.join("\n");
    if !stderr_lines.is_empty() {
        if !output.is_empty() {
            output.push('\n');
        }
        output.push_str(&stderr_lines.join("\n"));
    }

    // Wait for process to exit
    let status = child
        .wait()
        .await
        .map_err(|e| format!("Failed to wait for process: {}", e))?;

    let exit_code = status.code().unwrap_or(-1);
    Ok((output, exit_code))
}

/// Get allowlisted environment variables for hook execution.
///
/// Per spec §8.2: Hook environment variables filtered via allowlist.
fn allowed_env_vars() -> Vec<(String, String)> {
    let allowed_keys = [
        "PATH",
        "HOME",
        "USER",
        "LANG",
        "LC_ALL",
        "TERM",
        "SHELL",
        "XDG_CONFIG_HOME",
        "XDG_DATA_HOME",
        "XDG_CACHE_HOME",
        // Git-specific
        "GIT_AUTHOR_NAME",
        "GIT_AUTHOR_EMAIL",
        "GIT_COMMITTER_NAME",
        "GIT_COMMITTER_EMAIL",
    ];

    allowed_keys
        .iter()
        .filter_map(|key| std::env::var(key).ok().map(|v| (key.to_string(), v)))
        .collect()
}

// ============================================================================
// Hook Run Record Creation (§3)
// ============================================================================

/// Generate a hook run ID per spec §3: `hookrun_<uuid>`
pub fn generate_hook_run_id() -> String {
    format!("hookrun_{}", uuid::Uuid::new_v4())
}

/// Create a HookRunRecord for a hook that is about to be executed.
pub fn create_hook_run_record(
    config: &HookConfig,
    tool_runs: &[ToolRunRecord],
) -> HookRunRecord {
    HookRunRecord {
        run_id: generate_hook_run_id(),
        hook_name: config.name.clone(),
        tool_run_ids: get_triggering_tool_run_ids(config, tool_runs),
        status: HookRunStatus::Queued,
        started_at_ms: 0,
        finished_at_ms: None,
        attempt: 1,
        error: None,
    }
}

/// Update a HookRunRecord to Running status.
pub fn mark_hook_started(record: &mut HookRunRecord) {
    record.status = HookRunStatus::Running;
    record.started_at_ms = current_time_ms();
}

/// Update a HookRunRecord with execution result.
pub fn mark_hook_completed(record: &mut HookRunRecord, result: &HookExecutionResult) {
    record.status = result.status;
    record.finished_at_ms = Some(result.finished_at_ms);
    record.error = result.error.clone();
}

// ============================================================================
// Failure Policy Handling (§6)
// ============================================================================

/// Determine what to do after a hook failure based on its policy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FailureAction {
    /// Transition to Error state and stop hook pipeline
    FailSession,
    /// Log warning and continue to next hook
    WarnContinue,
    /// Retry the hook after a delay
    Retry { delay_ms: u64 },
}

/// Get the action to take for a failed hook based on its policy and attempt count.
pub fn get_failure_action(config: &HookConfig, attempt: u32) -> FailureAction {
    match &config.failure_policy {
        HookFailurePolicy::FailSession => FailureAction::FailSession,
        HookFailurePolicy::WarnContinue => FailureAction::WarnContinue,
        HookFailurePolicy::Retry {
            max_attempts,
            delay_ms,
        } => {
            if attempt < *max_attempts {
                FailureAction::Retry { delay_ms: *delay_ms }
            } else {
                // Exhausted retries, fall back to FailSession
                FailureAction::FailSession
            }
        }
    }
}

// ============================================================================
// Utilities
// ============================================================================

/// Get current time in milliseconds since Unix epoch.
fn current_time_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_state::ToolRunStatus;

    fn make_tool_run(run_id: &str, tool_name: &str, mutating: bool) -> ToolRunRecord {
        ToolRunRecord {
            run_id: run_id.to_string(),
            call_id: format!("call_{}", run_id),
            tool_name: tool_name.to_string(),
            mutating,
            status: ToolRunStatus::Succeeded,
            started_at_ms: 1000,
            finished_at_ms: Some(2000),
            attempt: 1,
            error: None,
        }
    }

    fn make_hook_config(name: &str, filter: HookToolFilter) -> HookConfig {
        HookConfig {
            name: name.to_string(),
            command: vec!["echo".to_string(), "test".to_string()],
            timeout_ms: 5000,
            failure_policy: HookFailurePolicy::default(),
            tool_filter: filter,
        }
    }

    // ========================================================================
    // Filter Tests
    // ========================================================================

    #[test]
    fn filter_any_mutating_matches_mutating_tools() {
        let hook = make_hook_config("test", HookToolFilter::AnyMutating);
        let tool_runs = vec![
            make_tool_run("1", "read_file", false),
            make_tool_run("2", "edit_file", true),
        ];

        assert!(hook_matches_filter(&hook, &tool_runs));
    }

    #[test]
    fn filter_any_mutating_no_match_when_no_mutating() {
        let hook = make_hook_config("test", HookToolFilter::AnyMutating);
        let tool_runs = vec![
            make_tool_run("1", "read_file", false),
            make_tool_run("2", "grep", false),
        ];

        assert!(!hook_matches_filter(&hook, &tool_runs));
    }

    #[test]
    fn filter_tool_names_matches_specific_tools() {
        let hook = make_hook_config(
            "test",
            HookToolFilter::ToolNames(vec!["edit_file".to_string(), "bash".to_string()]),
        );
        let tool_runs = vec![
            make_tool_run("1", "read_file", false),
            make_tool_run("2", "bash", true),
        ];

        assert!(hook_matches_filter(&hook, &tool_runs));
    }

    #[test]
    fn filter_tool_names_no_match_when_no_matching_tools() {
        let hook = make_hook_config(
            "test",
            HookToolFilter::ToolNames(vec!["git_commit".to_string()]),
        );
        let tool_runs = vec![
            make_tool_run("1", "read_file", false),
            make_tool_run("2", "edit_file", true),
        ];

        assert!(!hook_matches_filter(&hook, &tool_runs));
    }

    #[test]
    fn get_triggering_ids_for_any_mutating() {
        let hook = make_hook_config("test", HookToolFilter::AnyMutating);
        let tool_runs = vec![
            make_tool_run("1", "read_file", false),
            make_tool_run("2", "edit_file", true),
            make_tool_run("3", "bash", true),
        ];

        let ids = get_triggering_tool_run_ids(&hook, &tool_runs);
        assert_eq!(ids.len(), 2);
        assert!(ids.contains(&"2".to_string()));
        assert!(ids.contains(&"3".to_string()));
    }

    #[test]
    fn get_triggering_ids_for_tool_names() {
        let hook = make_hook_config(
            "test",
            HookToolFilter::ToolNames(vec!["bash".to_string()]),
        );
        let tool_runs = vec![
            make_tool_run("1", "edit_file", true),
            make_tool_run("2", "bash", true),
        ];

        let ids = get_triggering_tool_run_ids(&hook, &tool_runs);
        assert_eq!(ids.len(), 1);
        assert!(ids.contains(&"2".to_string()));
    }

    #[test]
    fn filter_hooks_for_batch_returns_matching_only() {
        let hooks = vec![
            make_hook_config("mutating_hook", HookToolFilter::AnyMutating),
            make_hook_config(
                "bash_hook",
                HookToolFilter::ToolNames(vec!["bash".to_string()]),
            ),
            make_hook_config(
                "git_hook",
                HookToolFilter::ToolNames(vec!["git_commit".to_string()]),
            ),
        ];

        let tool_runs = vec![
            make_tool_run("1", "read_file", false),
            make_tool_run("2", "edit_file", true),
        ];

        let filtered = filter_hooks_for_batch(&hooks, &tool_runs);
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].name, "mutating_hook");
    }

    // ========================================================================
    // Failure Policy Tests
    // ========================================================================

    #[test]
    fn failure_action_fail_session() {
        let config = HookConfig {
            name: "test".to_string(),
            command: vec!["echo".to_string()],
            timeout_ms: 5000,
            failure_policy: HookFailurePolicy::FailSession,
            tool_filter: HookToolFilter::AnyMutating,
        };

        assert_eq!(get_failure_action(&config, 1), FailureAction::FailSession);
    }

    #[test]
    fn failure_action_warn_continue() {
        let config = HookConfig {
            name: "test".to_string(),
            command: vec!["echo".to_string()],
            timeout_ms: 5000,
            failure_policy: HookFailurePolicy::WarnContinue,
            tool_filter: HookToolFilter::AnyMutating,
        };

        assert_eq!(get_failure_action(&config, 1), FailureAction::WarnContinue);
    }

    #[test]
    fn failure_action_retry_under_max() {
        let config = HookConfig {
            name: "test".to_string(),
            command: vec!["echo".to_string()],
            timeout_ms: 5000,
            failure_policy: HookFailurePolicy::Retry {
                max_attempts: 3,
                delay_ms: 500,
            },
            tool_filter: HookToolFilter::AnyMutating,
        };

        assert_eq!(
            get_failure_action(&config, 1),
            FailureAction::Retry { delay_ms: 500 }
        );
        assert_eq!(
            get_failure_action(&config, 2),
            FailureAction::Retry { delay_ms: 500 }
        );
    }

    #[test]
    fn failure_action_retry_exhausted_falls_back() {
        let config = HookConfig {
            name: "test".to_string(),
            command: vec!["echo".to_string()],
            timeout_ms: 5000,
            failure_policy: HookFailurePolicy::Retry {
                max_attempts: 3,
                delay_ms: 500,
            },
            tool_filter: HookToolFilter::AnyMutating,
        };

        // Attempt 3 = max_attempts, should fall back to FailSession
        assert_eq!(get_failure_action(&config, 3), FailureAction::FailSession);
    }

    // ========================================================================
    // Hook Run Record Tests
    // ========================================================================

    #[test]
    fn generate_hook_run_id_format() {
        let id = generate_hook_run_id();
        assert!(id.starts_with("hookrun_"));
        assert!(id.len() > 8); // "hookrun_" + uuid
    }

    #[test]
    fn create_hook_run_record_initializes_correctly() {
        let config = make_hook_config("test_hook", HookToolFilter::AnyMutating);
        let tool_runs = vec![make_tool_run("1", "edit_file", true)];

        let record = create_hook_run_record(&config, &tool_runs);

        assert!(record.run_id.starts_with("hookrun_"));
        assert_eq!(record.hook_name, "test_hook");
        assert_eq!(record.tool_run_ids, vec!["1".to_string()]);
        assert_eq!(record.status, HookRunStatus::Queued);
        assert_eq!(record.started_at_ms, 0);
        assert!(record.finished_at_ms.is_none());
        assert_eq!(record.attempt, 1);
        assert!(record.error.is_none());
    }

    #[test]
    fn mark_hook_started_updates_status() {
        let config = make_hook_config("test", HookToolFilter::AnyMutating);
        let tool_runs = vec![make_tool_run("1", "edit_file", true)];
        let mut record = create_hook_run_record(&config, &tool_runs);

        mark_hook_started(&mut record);

        assert_eq!(record.status, HookRunStatus::Running);
        assert!(record.started_at_ms > 0);
    }

    #[test]
    fn mark_hook_completed_updates_status() {
        let config = make_hook_config("test", HookToolFilter::AnyMutating);
        let tool_runs = vec![make_tool_run("1", "edit_file", true)];
        let mut record = create_hook_run_record(&config, &tool_runs);

        let result = HookExecutionResult {
            status: HookRunStatus::Succeeded,
            output: "done".to_string(),
            error: None,
            started_at_ms: 1000,
            finished_at_ms: 2000,
        };

        mark_hook_completed(&mut record, &result);

        assert_eq!(record.status, HookRunStatus::Succeeded);
        assert_eq!(record.finished_at_ms, Some(2000));
        assert!(record.error.is_none());
    }

    // ========================================================================
    // Config Loading Tests
    // ========================================================================

    #[test]
    fn load_hooks_config_returns_empty_when_missing() {
        let temp_dir = std::env::temp_dir().join("maestro_test_hooks_missing");
        let _ = std::fs::create_dir_all(&temp_dir);

        let result = load_hooks_config(&temp_dir);
        assert!(result.is_ok());
        assert!(result.unwrap().hooks.is_empty());

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn load_hooks_config_parses_valid_json() {
        let temp_dir = std::env::temp_dir().join("maestro_test_hooks_valid");
        let _ = std::fs::create_dir_all(&temp_dir);

        let config_json = r#"{
            "hooks": [
                {
                    "name": "auto_commit",
                    "command": ["git", "commit", "-am", "Auto-commit"],
                    "timeout_ms": 60000,
                    "failure_policy": { "type": "fail_session" },
                    "tool_filter": { "type": "any_mutating" }
                }
            ]
        }"#;

        std::fs::write(temp_dir.join("hooks.json"), config_json).unwrap();

        let result = load_hooks_config(&temp_dir);
        assert!(result.is_ok());
        let config = result.unwrap();
        assert_eq!(config.hooks.len(), 1);
        assert_eq!(config.hooks[0].name, "auto_commit");

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn load_hooks_config_returns_error_for_invalid_json() {
        let temp_dir = std::env::temp_dir().join("maestro_test_hooks_invalid");
        let _ = std::fs::create_dir_all(&temp_dir);

        std::fs::write(temp_dir.join("hooks.json"), "{ invalid json }").unwrap();

        let result = load_hooks_config(&temp_dir);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.code, "hook_config_invalid");

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    // ========================================================================
    // Hook Execution Tests (async)
    // ========================================================================

    #[tokio::test]
    async fn execute_hook_runs_simple_command() {
        let config = HookConfig {
            name: "echo_test".to_string(),
            command: vec!["echo".to_string(), "hello".to_string()],
            timeout_ms: 5000,
            failure_policy: HookFailurePolicy::default(),
            tool_filter: HookToolFilter::AnyMutating,
        };

        let workspace = std::env::temp_dir();
        let result = execute_hook(&config, &workspace).await;

        assert_eq!(result.status, HookRunStatus::Succeeded);
        assert!(result.output.contains("hello"));
        assert!(result.error.is_none());
        assert!(result.started_at_ms > 0);
        assert!(result.finished_at_ms >= result.started_at_ms);
    }

    #[tokio::test]
    async fn execute_hook_captures_failure_exit_code() {
        let config = HookConfig {
            name: "fail_test".to_string(),
            command: vec!["sh".to_string(), "-c".to_string(), "exit 1".to_string()],
            timeout_ms: 5000,
            failure_policy: HookFailurePolicy::default(),
            tool_filter: HookToolFilter::AnyMutating,
        };

        let workspace = std::env::temp_dir();
        let result = execute_hook(&config, &workspace).await;

        assert_eq!(result.status, HookRunStatus::Failed);
        assert!(result.error.is_some());
        assert!(result.error.unwrap().contains("exit"));
    }

    #[tokio::test]
    async fn execute_hook_fails_on_empty_command() {
        let config = HookConfig {
            name: "empty".to_string(),
            command: vec![],
            timeout_ms: 5000,
            failure_policy: HookFailurePolicy::default(),
            tool_filter: HookToolFilter::AnyMutating,
        };

        let workspace = std::env::temp_dir();
        let result = execute_hook(&config, &workspace).await;

        assert_eq!(result.status, HookRunStatus::Failed);
        assert!(result.error.unwrap().contains("empty"));
    }

    #[tokio::test]
    async fn execute_hook_times_out_slow_command() {
        let config = HookConfig {
            name: "slow".to_string(),
            command: vec!["sleep".to_string(), "10".to_string()],
            timeout_ms: 100, // Very short timeout
            failure_policy: HookFailurePolicy::default(),
            tool_filter: HookToolFilter::AnyMutating,
        };

        let workspace = std::env::temp_dir();
        let result = execute_hook(&config, &workspace).await;

        assert_eq!(result.status, HookRunStatus::Failed);
        assert!(result.error.unwrap().contains("timed out"));
    }
}
