#!/usr/bin/env bash
# agent-loop-analyze.sh - Generate analysis prompt for a run

set -euo pipefail

LOG_DIR="logs/agent-loop"
RUN_ID=""

usage() {
  cat <<EOF
Usage: $(basename "$0") [run-id] [options]

Arguments:
  run-id              Run ID (defaults to latest run)

Options:
  --log-dir <path>    Log directory (default: logs/agent-loop)
  -h, --help          Show this help

Examples:
  $(basename "$0")
  $(basename "$0") 20260121-120045
  $(basename "$0") --log-dir /tmp/agent-loop
EOF
}

parse_args() {
  while [[ $# -gt 0 ]]; do
    case "$1" in
      --log-dir)
        LOG_DIR="$2"
        shift 2
        ;;
      -h|--help)
        usage
        exit 0
        ;;
      -* )
        printf 'Unknown option: %s\n' "$1" >&2
        usage >&2
        exit 1
        ;;
      *)
        if [[ -z "$RUN_ID" ]]; then
          RUN_ID="$1"
        else
          printf 'Unexpected argument: %s\n' "$1" >&2
          usage >&2
          exit 1
        fi
        shift
        ;;
    esac
  done
}

find_latest_run_id() {
  if ! compgen -G "$LOG_DIR/run-*-report.tsv" >/dev/null; then
    printf 'Error: No run reports found in %s\n' "$LOG_DIR" >&2
    exit 1
  fi

  local latest_report=""
  while IFS= read -r file; do
    latest_report="$file"
    break
  done < <(ls -t "$LOG_DIR"/run-*-report.tsv)

  if [[ -z "$latest_report" ]]; then
    printf 'Error: Unable to determine latest run\n' >&2
    exit 1
  fi

  local base
  base=$(basename "$latest_report")
  RUN_ID=${base#run-}
  RUN_ID=${RUN_ID%-report.tsv}
}

parse_report() {
  local report_path="$1"
  local last_iter_log=""
  local last_iter_tail=""
  local last_iter=""
  local completion_mode=""
  local completion_iter=""

  while IFS=$'\t' read -r timestamp kind iteration duration exit_code output_bytes output_lines output_path message; do
    if [[ "$kind" == "kind" ]]; then
      continue
    fi

    case "$kind" in
      ITERATION_END)
        last_iter_log="$output_path"
        last_iter="$iteration"
        ;;
      ITERATION_TAIL)
        last_iter_tail="$output_path"
        ;;
      COMPLETE_DETECTED)
        completion_iter="$iteration"
        completion_mode=${message#mode=}
        ;;
    esac
  done < "$report_path"

  printf '%s|%s|%s|%s|%s\n' "$last_iter" "$last_iter_log" "$last_iter_tail" "$completion_iter" "$completion_mode"
}

main() {
  parse_args "$@"

  if [[ -z "$RUN_ID" ]]; then
    find_latest_run_id
  fi

  local run_log="$LOG_DIR/run-$RUN_ID.log"
  local run_report="$LOG_DIR/run-$RUN_ID-report.tsv"
  local prompt_snapshot="$LOG_DIR/run-$RUN_ID-prompt.txt"
  local summary_json="$LOG_DIR/run-$RUN_ID-summary.json"
  local analysis_prompt_path="$LOG_DIR/run-$RUN_ID-analysis-prompt.txt"

  if [[ ! -f "$run_report" ]]; then
    printf 'Error: Run report not found: %s\n' "$run_report" >&2
    exit 1
  fi

  local parse_result
  parse_result=$(parse_report "$run_report")

  local last_iter last_iter_log last_iter_tail completion_iter completion_mode
  IFS='|' read -r last_iter last_iter_log last_iter_tail completion_iter completion_mode <<< "$parse_result"

  local completion_display="not detected"
  if [[ -n "$completion_iter" ]]; then
    completion_display="iteration $completion_iter"
    if [[ -n "$completion_mode" ]]; then
      completion_display+=" ($completion_mode)"
    fi
  fi

  local prompt
  prompt=$(cat <<EOF
Analyze this agent-loop run. Focus on end-of-task behavior, completion protocol compliance, and
actionable improvements to the spec templates and loop prompt.

Run metadata:
- Run ID: $RUN_ID
- Completion detected: $completion_display
- Last iteration observed: ${last_iter:-unknown}

Artifacts (read all that exist):
- Run report (TSV): $run_report
- Run log: $run_log
- Prompt snapshot: $prompt_snapshot
- Summary JSON: $summary_json
- Last iteration tail: ${last_iter_tail:-unknown}
- Last iteration log: ${last_iter_log:-unknown}

Return:
1) Short timeline summary + anomalies
2) End-of-task behavior (did it cleanly finish? protocol violations?)
3) Spec/template improvements (actionable)
4) Loop prompt improvements (actionable)
5) Loop UX/logging improvements (actionable)
EOF
)

  printf '%s\n' "$prompt" > "$analysis_prompt_path"
  printf '%s\n' "$prompt"
  printf 'Saved analysis prompt to: %s\n' "$analysis_prompt_path" >&2
}

main "$@"
