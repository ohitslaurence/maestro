#!/usr/bin/env bash
# agent-loop-ui.sh - Gum UI helpers for agent-loop.sh
# See: specs/agent-loop-terminal-ux.md §2, §4.2

# Globals (set by init_ui)
declare -g GUM_ENABLED=false
declare -g LOG_DIR=""
declare -g RUN_ID=""
declare -g RUN_LOG=""

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
