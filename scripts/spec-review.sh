#!/usr/bin/env bash
# spec-review.sh - Analyze a spec + plan for readiness

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# shellcheck source=lib/agent-loop-ui.sh
source "$SCRIPT_DIR/lib/agent-loop-ui.sh"

# shellcheck source=lib/spec-picker.sh
source "$SCRIPT_DIR/lib/spec-picker.sh"

spec_path=""
plan_path=""
specs_dir="specs"
plans_dir="specs/planning"
log_dir="logs/spec-review"
no_gum=false
model="opus"
reports_dir="reports/specs"

usage() {
  cat <<EOF
Usage: $(basename "$0") [spec-path] [plan-path] [options]

Arguments:
  spec-path           Path to spec file (optional if gum available)
  plan-path           Path to plan file (defaults to <plans_dir>/<spec>-plan.md)

Options:
  --log-dir <path>    Base log directory (default: logs/spec-review)
  --model <name>      Claude model or alias (default: opus)
  --no-gum            Disable gum UI, use plain output
  -h, --help          Show this help

Notes:
  - Runs three independent reviews: template, solution quality, context references.
  - Writes outputs to reports/specs/<spec>/<spec-stem>-N/.

Examples:
  $(basename "$0") specs/agent-loop-terminal-ux.md
  $(basename "$0") specs/agent-loop-terminal-ux.md specs/planning/agent-loop-terminal-ux-plan.md
  $(basename "$0") --no-gum specs/agent-loop-terminal-ux.md
EOF
}

parse_args() {
  while [[ $# -gt 0 ]]; do
    case "$1" in
      --log-dir)
        log_dir="$2"
        shift 2
        ;;
      --model)
        model="$2"
        shift 2
        ;;
      --no-gum)
        no_gum=true
        shift
        ;;
      -h|--help)
        usage
        exit 0
        ;;
      -* )
        echo "Unknown option: $1" >&2
        usage >&2
        exit 1
        ;;
      *)
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

validate_inputs() {
  if [[ -z "$spec_path" ]]; then
    if [[ "$no_gum" == "true" ]] || ! check_gum; then
      echo "Error: spec-path is required when gum is unavailable or --no-gum is set" >&2
      list_known_specs >&2
      usage >&2
      exit 1
    fi

    if ! spec_picker; then
      echo "Error: No spec selected" >&2
      exit 1
    fi
    spec_path="$PICKED_SPEC_PATH"
    plan_path="$PICKED_PLAN_PATH"
  fi

  if [[ ! -f "$spec_path" && -n "$specs_dir" ]]; then
    local candidate_spec="$specs_dir/$spec_path"
    if [[ -f "$candidate_spec" ]]; then
      spec_path="$candidate_spec"
    fi
  fi

  if [[ -z "$plan_path" ]]; then
    local spec_base
    spec_base=$(basename "$spec_path")
    plan_path="$plans_dir/${spec_base%.md}-plan.md"
  elif [[ ! -f "$plan_path" && -n "$plans_dir" ]]; then
    local candidate_plan="$plans_dir/$(basename "$plan_path")"
    if [[ -f "$candidate_plan" ]]; then
      plan_path="$candidate_plan"
    fi
  fi

  if [[ ! -f "$spec_path" ]]; then
    echo "Error: Spec not found: $spec_path" >&2
    exit 1
  fi

  if [[ ! -f "$plan_path" ]]; then
    echo "Error: Plan not found: $plan_path" >&2
    echo "Hint: Create plan at $plan_path" >&2
    exit 1
  fi
}

build_template_prompt() {
  cat <<'EOF'
@SPEC_PATH @PLAN_PATH @specs/README.md @specs/planning/SPEC_AUTHORING.md

You are a format and template reviewer. Ensure the spec and plan follow SPEC_AUTHORING.md
exactly and are in the correct shape for implementation. This review is independent of
other reviews.

Focus areas:
- Spec metadata header and numbered sections match the template requirements.
- Required spec sections exist and are in the expected order.
- Plan structure and checklist conventions match the plan template.
- Files to Create/Modify and Verification Checklist sections exist (or explicitly "None").
- specs/README.md includes the spec + plan entry with correct links.

Return a Markdown report with sections:
1) Template compliance verdict (PASS/FAIL) with one-sentence rationale.
2) Missing or malformed spec sections (with section numbers).
3) Plan format issues (phases, checklist items, verification list).
4) Indexing issues in specs/README.md.
5) Required edits (bullet list of concrete changes).
EOF
}

build_solution_prompt() {
  cat <<'EOF'
@SPEC_PATH @PLAN_PATH

You are a solution reviewer. Evaluate whether the spec design is sound, minimal, and
unambiguous. This review is independent of other reviews.

Focus areas:
- Is the solution clearly defined and implementable without guessing?
- Are data shapes, workflows, and edge cases sufficiently specified?
- Are there better alternatives or simplifications that should be considered?
- Are risks, assumptions, or tradeoffs missing?

Return a Markdown report with sections:
1) Solution quality verdict (PASS/FAIL) with one-sentence rationale.
2) Ambiguities or underspecified behavior (must-fix).
3) Missing requirements or edge cases.
4) Potential improvements or alternatives.
5) Risks or assumptions to clarify.
EOF
}

build_context_prompt() {
  cat <<'EOF'
@SPEC_PATH @PLAN_PATH @specs/README.md

You are a context curator. For an implementing agent seeing this spec fresh, list the
exact repo files that should be included as references in the prompt so they do not
need to search. This review is independent of other reviews.

Return a Markdown report with sections:
1) Required references (ordered list of exact file paths + one-line reason each).
2) Optional references (nice-to-have).
3) Missing references the spec should add (if any).

Guidelines:
- Prefer specific files over directories.
- Keep the list minimal and directly relevant.
- Use paths relative to the repo root.
EOF
}

render_prompt() {
  local prompt="$1"
  prompt=${prompt//SPEC_PATH/$spec_path}
  prompt=${prompt//PLAN_PATH/$plan_path}
  printf '%s' "$prompt"
}

run_review() {
  local label="$1"
  local prompt="$2"
  local output_path="$3"
  local iteration="$4"

  local result=""
  local claude_exit=0

  run_claude_iteration "$iteration" "$prompt" "$model" result || claude_exit=$?
  printf '%s\n' "$result" > "$output_path"

  printf '\n=== %s Review ===\n' "$label"
  printf '%s\n' "$result"

  return "$claude_exit"
}

next_report_dir() {
  local spec_base
  spec_base=$(basename "$spec_path")
  local spec_stem="${spec_base%.md}"
  local group_dir="$reports_dir/$spec_base"
  local prefix="$group_dir/$spec_stem-"
  local max_index=0

  mkdir -p "$group_dir"

  shopt -s nullglob
  local path
  for path in "$prefix"*; do
    if [[ ! -d "$path" ]]; then
      continue
    fi
    local dir_base
    dir_base=$(basename "$path")
    local suffix="${dir_base#${spec_stem}-}"
    if [[ "$suffix" =~ ^[0-9]+$ ]]; then
      if ((suffix > max_index)); then
        max_index=$suffix
      fi
    fi
  done
  shopt -u nullglob

  local next_index=$((max_index + 1))
  printf '%s/%s-%d' "$group_dir" "$spec_stem" "$next_index"
}

main() {
  parse_args "$@"
  validate_inputs

  if ! command -v claude >/dev/null 2>&1; then
    echo "Error: claude CLI not found" >&2
    exit 1
  fi

  if ! init_ui "$log_dir" "$no_gum"; then
    exit 1
  fi

  PLAN_PATH="$plan_path"
  RUN_MODEL="$model"

  setup_signal_traps

  ui_header "Spec Review"
  ui_status "Run ID:     $RUN_ID"
  ui_status "Spec:       $spec_path"
  ui_status "Plan:       $plan_path"
  ui_status "Model:      $model"
  ui_status "Run dir:    $RUN_DIR"
  ui_status "Gum:        $GUM_ENABLED"

  local review_dir
  review_dir=$(next_report_dir)
  local prompt_dir="$review_dir/prompts"
  mkdir -p "$review_dir" "$prompt_dir"
  ui_status "Report dir: $review_dir"

  local template_prompt
  local solution_prompt
  local context_prompt
  template_prompt=$(render_prompt "$(build_template_prompt)")
  solution_prompt=$(render_prompt "$(build_solution_prompt)")
  context_prompt=$(render_prompt "$(build_context_prompt)")

  local prompt_manifest
  prompt_manifest=$(cat <<EOF
# Spec Review Prompts

## Template Review
$template_prompt

## Solution Review
$solution_prompt

## Context Review
$context_prompt
EOF
  )

  write_prompt_snapshot "$prompt_manifest"
  printf '%s\n' "$template_prompt" > "$prompt_dir/template.txt"
  printf '%s\n' "$solution_prompt" > "$prompt_dir/solution.txt"
  printf '%s\n' "$context_prompt" > "$prompt_dir/context.txt"

  run_review "Template" "$template_prompt" "$review_dir/template-review.md" "1" || {
    local exit_code=$?
    show_run_summary "claude_failed" "$exit_code"
    exit "$exit_code"
  }

  run_review "Solution" "$solution_prompt" "$review_dir/solution-review.md" "2" || {
    local exit_code=$?
    show_run_summary "claude_failed" "$exit_code"
    exit "$exit_code"
  }

  run_review "Context" "$context_prompt" "$review_dir/context-review.md" "3" || {
    local exit_code=$?
    show_run_summary "claude_failed" "$exit_code"
    exit "$exit_code"
  }

  show_run_summary "complete" "0"
}

if [[ "${BASH_SOURCE[0]}" == "$0" ]]; then
  main "$@"
fi
