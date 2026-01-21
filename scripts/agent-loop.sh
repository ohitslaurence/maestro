#!/usr/bin/env bash
# agent-loop.sh - Run claude in a loop until COMPLETE token is detected
# See: specs/agent-loop-terminal-ux.md

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# Source UI helpers
# shellcheck source=lib/agent-loop-ui.sh
source "$SCRIPT_DIR/lib/agent-loop-ui.sh"

# shellcheck source=lib/spec-picker.sh
source "$SCRIPT_DIR/lib/spec-picker.sh"

# -----------------------------------------------------------------------------
# Defaults (spec §4.1)
# -----------------------------------------------------------------------------
spec_path=""
plan_path=""
iterations=50
log_dir="logs/agent-loop"
no_gum=false
summary_json=false
no_wait=false

# -----------------------------------------------------------------------------
# Usage
# -----------------------------------------------------------------------------
usage() {
  cat <<EOF
Usage: $(basename "$0") [spec-path] [plan-path] [options]

Arguments:
  spec-path           Path to spec file (optional if gum available)
  plan-path           Path to plan file (defaults to specs/planning/<spec>-plan.md)

Options:
  --iterations <n>    Maximum loop iterations (default: 50)
  --log-dir <path>    Base log directory (default: logs/agent-loop)
  --no-gum            Disable gum UI, use plain output
  --summary-json      Write summary JSON at end of run
  --no-wait           Skip completion screen wait

Examples:
  $(basename "$0") specs/my-feature.md
  $(basename "$0") specs/my-feature.md specs/planning/my-feature-plan.md --iterations 10
  $(basename "$0") --no-gum specs/my-feature.md
EOF
}

# -----------------------------------------------------------------------------
# Argument parsing
# -----------------------------------------------------------------------------
parse_args() {
  while [[ $# -gt 0 ]]; do
    case "$1" in
      --iterations)
        iterations="$2"
        shift 2
        ;;
      --log-dir)
        log_dir="$2"
        shift 2
        ;;
      --no-gum)
        no_gum=true
        shift
        ;;
      --summary-json)
        summary_json=true
        shift
        ;;
      --no-wait)
        no_wait=true
        shift
        ;;
      -h|--help)
        usage
        exit 0
        ;;
      -*)
        echo "Unknown option: $1" >&2
        usage >&2
        exit 1
        ;;
      *)
        # Positional arguments
        if [[ -z "$spec_path" ]]; then
          spec_path="$1"
        elif [[ -z "$plan_path" ]]; then
          plan_path="$1"
        else
          echo "Unexpected argument: $1" >&2
          usage >&2
          exit 1
        fi
        shift
        ;;
    esac
  done
}

# -----------------------------------------------------------------------------
# Validation
# -----------------------------------------------------------------------------
validate_inputs() {
  # Check if spec path is required
  if [[ -z "$spec_path" ]]; then
    # If gum is available and not disabled, use spec picker (spec §4.1, §5.1)
    if [[ "$no_gum" == "true" ]] || ! check_gum; then
      echo "Error: spec-path is required when gum is unavailable or --no-gum is set" >&2
      list_known_specs >&2
      usage >&2
      exit 1
    else
      # Launch interactive spec picker
      if ! spec_picker; then
        echo "Error: No spec selected" >&2
        exit 1
      fi
      spec_path="$PICKED_SPEC_PATH"
      plan_path="$PICKED_PLAN_PATH"
    fi
  fi

  # Derive plan path if not provided
  if [[ -z "$plan_path" ]]; then
    local spec_base
    spec_base=$(basename "$spec_path")
    plan_path="specs/planning/${spec_base%.md}-plan.md"
  fi

  # Validate spec exists
  if [[ ! -f "$spec_path" ]]; then
    echo "Error: Spec not found: $spec_path" >&2
    exit 1
  fi

  # Validate plan exists
  if [[ ! -f "$plan_path" ]]; then
    echo "Error: Plan not found: $plan_path" >&2
    echo "Hint: Create plan at $plan_path" >&2
    exit 1
  fi

  # Validate iterations is a number
  if ! [[ "$iterations" =~ ^[0-9]+$ ]]; then
    echo "Error: --iterations must be a positive integer" >&2
    exit 1
  fi
}

# -----------------------------------------------------------------------------
# Main
# -----------------------------------------------------------------------------
main() {
  parse_args "$@"
  validate_inputs

  # Initialize UI and logging (spec §2.1, §2.3)
  if ! init_ui "$log_dir" "$no_gum"; then
    exit 1
  fi

  # Show run header (spec §4)
  show_run_header "$spec_path" "$plan_path" "$iterations"

  # Build prompt
  local prompt
  prompt=$(cat <<'EOF'
@SPEC_PATH @PLAN_PATH @specs/README.md @specs/planning/SPEC_AUTHORING.md

You are an implementation agent. Read the spec, the plan, and any referenced docs.
Check the plan for notes or feedback from other agents before choosing work.

Task:
1. Choose ONE unchecked task from the plan with the highest priority (not necessarily first).
2. Implement only that task (single feature). Avoid unrelated changes.
3. Run verification relevant to that task. If the plan lists a verification checklist, run what
   applies. If you cannot run a step, say why.
4. Update the plan checklist: mark only the task(s) you completed with [x]. Leave others untouched.
5. Make exactly one git commit for your changes using `gritty commit --accept`.
6. If (and only if) all tasks in the plan are complete after your update, respond with EXACTLY:
<promise>COMPLETE</promise>

Response format (strict):
- If complete: output exactly `<promise>COMPLETE</promise>` and nothing else (no other text, no code
  fences, no leading/trailing whitespace, no newline commentary).
- If not complete: do NOT output `<promise>COMPLETE</promise>` anywhere in your response.

Constraints:
- Do not modify files under `reference/`.
- Do not work on more than one plan item.
- If no changes were made, do not commit.
- Use `bun` for JavaScript/TypeScript commands.

The runner only stops when your entire output is exactly `<promise>COMPLETE</promise>`.
EOF
)

  prompt=${prompt//SPEC_PATH/$spec_path}
  prompt=${prompt//PLAN_PATH/$plan_path}

  # Run loop
  for ((i=1; i<=iterations; i++)); do
    local result=""
    local claude_exit=0

    # Run claude with spinner and per-iteration logging (spec §5.1)
    run_claude_iteration "$i" "$prompt" result || claude_exit=$?

    # Trim whitespace for comparison
    local trimmed_result="$result"
    trimmed_result="${trimmed_result#"${trimmed_result%%[!$'\t\n\r ']*}"}"
    trimmed_result="${trimmed_result%"${trimmed_result##*[!$'\t\n\r ']}"}"

    # Check for completion (spec §4.1 strict mode)
    if [[ "$trimmed_result" == "<promise>COMPLETE</promise>" ]]; then
      record_completion "$i" "strict"
      printf '%s\n' "$trimmed_result"
      exit 0
    fi

    # Check for completion token anywhere (spec §4.1 lenient mode)
    if [[ "$result" == *"<promise>COMPLETE</promise>"* ]]; then
      record_completion "$i" "lenient"
      ui_log "WARN" "Completion token found with extra output - protocol violation"
      printf '%s\n' "$result"
      exit 0
    fi

    printf '%s\n' "$result"

    # Exit on non-zero claude exit (spec §6)
    if ((claude_exit != 0)); then
      ui_log "RUN_END" "claude_failed exit_code=$claude_exit iteration=$i"
      exit "$claude_exit"
    fi
  done

  ui_log "RUN_END" "iterations_exhausted=$iterations"
}

main "$@"
