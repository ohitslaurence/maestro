#!/usr/bin/env bash
# agent-loop-ui.sh - Gum UI helpers for agent-loop.sh
# See: specs/agent-loop-terminal-ux.md §2, §4.2

# Globals (set by init_ui)
declare -g GUM_ENABLED=false
declare -g LOG_DIR=""
declare -g RUN_ID=""
declare -g RUN_LOG=""
declare -g RUN_START_MS=""

# Per-iteration stats (spec §3.1 IterationStats)
declare -g ITER_START_MS=""
declare -g ITER_END_MS=""
declare -g ITER_DURATION_MS=""
declare -g ITER_EXIT_CODE=""
declare -g ITER_COMPLETE_DETECTED=""
declare -g ITER_LOG_PATH=""

# Aggregated stats
declare -g TOTAL_ITERATIONS=0
declare -g COMPLETED_ITERATION=""
declare -g COMPLETION_MODE=""

# -----------------------------------------------------------------------------
# Gum detection and initialization
# -----------------------------------------------------------------------------

check_gum() {
  command -v gum &>/dev/null
}

init_ui() {
  local log_dir="${1:-logs/agent-loop}"
  local no_gum="${2:-false}"

  # Generate run ID
  RUN_ID=$(date +"%Y%m%d-%H%M%S")
  LOG_DIR="$log_dir"
  RUN_START_MS=$(get_epoch_ms)

  # Create log directory
  if ! mkdir -p "$LOG_DIR"; then
    ui_log "ERROR" "Cannot create log directory: $LOG_DIR"
    return 1
  fi

  RUN_LOG="$LOG_DIR/run-$RUN_ID.log"

  # Determine gum availability
  if [[ "$no_gum" == "true" ]]; then
    GUM_ENABLED=false
  elif [[ ! -t 1 ]]; then
    # Non-TTY environment
    GUM_ENABLED=false
  elif check_gum; then
    GUM_ENABLED=true
  else
    GUM_ENABLED=false
  fi

  return 0
}

require_gum() {
  if ! check_gum; then
    cat >&2 <<'EOF'
Error: gum is required but not installed.

Install with:
  brew install gum       # macOS
  go install github.com/charmbracelet/gum@latest

Or run with --no-gum for plain output.
EOF
    return 1
  fi
  return 0
}

# -----------------------------------------------------------------------------
# Timing helpers (spec §3.1)
# -----------------------------------------------------------------------------

get_epoch_ms() {
  # Use date with nanoseconds and truncate to milliseconds
  # Compatible with GNU date (Linux) and BSD date (macOS with coreutils)
  if date +%s%N &>/dev/null; then
    echo $(( $(date +%s%N) / 1000000 ))
  else
    # Fallback to seconds only (macOS without coreutils)
    echo $(( $(date +%s) * 1000 ))
  fi
}

format_duration_ms() {
  local ms="$1"
  local seconds=$((ms / 1000))
  local minutes=$((seconds / 60))
  local remaining_seconds=$((seconds % 60))

  if ((minutes > 0)); then
    printf '%dm %ds' "$minutes" "$remaining_seconds"
  else
    printf '%ds' "$seconds"
  fi
}

# -----------------------------------------------------------------------------
# Iteration tracking (spec §3.1 IterationStats)
# -----------------------------------------------------------------------------

start_iteration() {
  local iteration="$1"
  ITER_START_MS=$(get_epoch_ms)
  ITER_EXIT_CODE=""
  ITER_COMPLETE_DETECTED="false"
  ITER_LOG_PATH="$LOG_DIR/run-$RUN_ID-iter-$(printf '%02d' "$iteration").log"
  TOTAL_ITERATIONS=$iteration

  ui_log "ITERATION_START" "iteration=$iteration"

  # Show iteration status
  local elapsed_ms=$((ITER_START_MS - RUN_START_MS))
  local elapsed_str
  elapsed_str=$(format_duration_ms "$elapsed_ms")

  local status_line="Iteration $iteration | Elapsed: $elapsed_str"
  if [[ -n "$ITER_DURATION_MS" ]]; then
    local last_dur
    last_dur=$(format_duration_ms "$ITER_DURATION_MS")
    status_line="$status_line | Last: $last_dur"
  fi
  ui_status "$status_line"
}

end_iteration() {
  local iteration="$1"
  local exit_code="$2"

  ITER_END_MS=$(get_epoch_ms)
  ITER_DURATION_MS=$((ITER_END_MS - ITER_START_MS))
  ITER_EXIT_CODE="$exit_code"

  ui_log "ITERATION_END" "iteration=$iteration exit_code=$exit_code duration_ms=$ITER_DURATION_MS"
}

record_completion() {
  local iteration="$1"
  local mode="$2"

  ITER_COMPLETE_DETECTED="true"
  COMPLETED_ITERATION="$iteration"
  COMPLETION_MODE="$mode"

  ui_log "COMPLETE_DETECTED" "mode=$mode iteration=$iteration"
}

# -----------------------------------------------------------------------------
# Claude execution with spinner (spec §5.1)
# -----------------------------------------------------------------------------

run_claude_iteration() {
  local iteration="$1"
  local prompt="$2"
  local -n output_ref=$3

  start_iteration "$iteration"

  local temp_output
  temp_output=$(mktemp)
  local exit_code=0

  if [[ "$GUM_ENABLED" == "true" ]]; then
    # Run with gum spinner
    gum spin --spinner dot --title "Iteration $iteration: Running claude..." -- \
      bash -c "claude --dangerously-skip-permissions -p \"\$1\" > \"\$2\" 2>&1" \
      -- "$prompt" "$temp_output" || exit_code=$?
  else
    # Plain output mode
    printf 'Iteration %d: Running claude...\n' "$iteration"
    claude --dangerously-skip-permissions -p "$prompt" > "$temp_output" 2>&1 || exit_code=$?
  fi

  output_ref=$(cat "$temp_output")

  # Write to per-iteration log (spec §4.3)
  cp "$temp_output" "$ITER_LOG_PATH"
  rm -f "$temp_output"

  end_iteration "$iteration" "$exit_code"

  # Warn on empty output
  if [[ -z "$output_ref" ]]; then
    ui_log "WARN" "Empty output from claude in iteration $iteration"
  fi

  # Handle non-zero exit
  if ((exit_code != 0)); then
    ui_log "ERROR" "claude exited with code $exit_code in iteration $iteration"
    ui_log "ERROR" "See iteration log: $ITER_LOG_PATH"
  fi

  return "$exit_code"
}

# -----------------------------------------------------------------------------
# UI helpers (spec §4.2)
# -----------------------------------------------------------------------------

ui_header() {
  local title="$1"
  if [[ "$GUM_ENABLED" == "true" ]]; then
    gum style --border normal --padding "0 1" --border-foreground 212 "$title"
  else
    printf '\n=== %s ===\n\n' "$title"
  fi
}

ui_status() {
  local line="$1"
  if [[ "$GUM_ENABLED" == "true" ]]; then
    gum style --foreground 245 "$line"
  else
    printf '%s\n' "$line"
  fi
}

ui_spinner() {
  local title="$1"
  shift
  if [[ "$GUM_ENABLED" == "true" ]]; then
    gum spin --spinner dot --title "$title" -- "$@"
  else
    printf '%s... ' "$title"
    "$@"
    printf 'done\n'
  fi
}

ui_log() {
  local level="$1"
  local message="$2"
  local timestamp
  timestamp=$(date +"%Y-%m-%d %H:%M:%S")

  local formatted="[$timestamp] [$level] $message"

  # Write to run log if available
  if [[ -n "$RUN_LOG" ]]; then
    printf '%s\n' "$formatted" >> "$RUN_LOG"
  fi

  # Display based on level and gum availability
  case "$level" in
    ERROR)
      if [[ "$GUM_ENABLED" == "true" ]]; then
        gum style --foreground 196 "$formatted" >&2
      else
        printf '%s\n' "$formatted" >&2
      fi
      ;;
    WARN)
      if [[ "$GUM_ENABLED" == "true" ]]; then
        gum style --foreground 214 "$formatted"
      else
        printf '%s\n' "$formatted"
      fi
      ;;
    INFO)
      if [[ "$GUM_ENABLED" == "true" ]]; then
        gum style --foreground 45 "$formatted"
      else
        printf '%s\n' "$formatted"
      fi
      ;;
    *)
      printf '%s\n' "$formatted"
      ;;
  esac
}

ui_table() {
  local title="$1"
  shift
  local rows=("$@")

  if [[ "$GUM_ENABLED" == "true" ]]; then
    ui_header "$title"
    printf '%s\n' "${rows[@]}" | gum table
  else
    printf '\n--- %s ---\n' "$title"
    printf '%s\n' "${rows[@]}"
    printf '\n'
  fi
}

# -----------------------------------------------------------------------------
# Run header display
# -----------------------------------------------------------------------------

show_run_header() {
  local spec_path="$1"
  local plan_path="$2"
  local iterations="$3"

  ui_header "Agent Loop"
  ui_status "Run ID:      $RUN_ID"
  ui_status "Spec:        $spec_path"
  ui_status "Plan:        $plan_path"
  ui_status "Iterations:  $iterations"
  ui_status "Log dir:     $LOG_DIR"
  ui_status "Gum:         $GUM_ENABLED"

  ui_log "RUN_START" "spec=$spec_path plan=$plan_path iterations=$iterations"
}
