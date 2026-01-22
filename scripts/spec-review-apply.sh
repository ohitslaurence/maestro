#!/usr/bin/env bash
# spec-review-apply.sh - Apply spec review fixes

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# shellcheck source=lib/agent-loop-ui.sh
source "$SCRIPT_DIR/lib/agent-loop-ui.sh"

# shellcheck source=lib/spec-picker.sh
source "$SCRIPT_DIR/lib/spec-picker.sh"

spec_path=""
plan_path=""
report_path=""
specs_dir="specs"
plans_dir="specs/planning"
log_dir="logs/spec-review-apply"
reports_dir="reports/specs"
no_gum=false
model="opus"

usage() {
  cat <<EOF
Usage: $(basename "$0") [spec-path] [report-run] [options]

Arguments:
  spec-path           Path to spec file (optional if gum available)
  report-run          Report run directory name (optional if gum available)

Options:
  --log-dir <path>    Base log directory (default: logs/spec-review-apply)
  --reports-dir <path> Base reports directory (default: reports/specs)
  --model <name>      Claude model or alias (default: opus)
  --no-gum            Disable gum UI, use plain output
  -h, --help          Show this help

Notes:
  - Applies fixes using template/solution/context review reports.
  - Report runs live under reports/specs/<spec>/<spec-stem>-N/.
EOF
}

parse_args() {
  while [[ $# -gt 0 ]]; do
    case "$1" in
      --log-dir)
        log_dir="$2"
        shift 2
        ;;
      --reports-dir)
        reports_dir="$2"
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
        elif [[ -z "$report_path" ]]; then
          report_path="$1"
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

validate_spec_and_plan() {
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

discover_report_runs() {
  local spec_base
  spec_base=$(basename "$spec_path")
  local spec_stem="${spec_base%.md}"
  local group_dir="$reports_dir/$spec_base"
  local entries=()

  if [[ ! -d "$group_dir" ]]; then
    return 1
  fi

  shopt -s nullglob
  local path
  for path in "$group_dir/$spec_stem-"*; do
    if [[ ! -d "$path" ]]; then
      continue
    fi
    local dir_base
    dir_base=$(basename "$path")
    local suffix="${dir_base#${spec_stem}-}"
    if [[ ! "$suffix" =~ ^[0-9]+$ ]]; then
      continue
    fi
    local mtime
    mtime=$(stat -c '%Y' "$path" 2>/dev/null || stat -f '%m' "$path" 2>/dev/null || echo "0")
    local date
    date=$(date -d "@$mtime" +%Y-%m-%d 2>/dev/null || date -r "$mtime" +%Y-%m-%d 2>/dev/null || echo "unknown")
    entries+=("$dir_base|$path|$date|$suffix")
  done
  shopt -u nullglob

  if [[ ${#entries[@]} -eq 0 ]]; then
    return 1
  fi

  printf '%s\n' "${entries[@]}" | sort -t'|' -k4,4nr
}

select_report_run() {
  local entries=()
  while IFS= read -r entry; do
    entries+=("$entry")
  done < <(discover_report_runs)

  if [[ ${#entries[@]} -eq 0 ]]; then
    echo "Error: No report runs found for spec" >&2
    return 1
  fi

  if [[ "$no_gum" == "true" ]] || ! check_gum; then
    echo "Error: report-run is required when gum is unavailable or --no-gum is set" >&2
    return 1
  fi

  local display_lines=()
  declare -A entry_map
  local entry
  for entry in "${entries[@]}"; do
    local run_name run_path run_date
    run_name=$(echo "$entry" | cut -d'|' -f1)
    run_path=$(echo "$entry" | cut -d'|' -f2)
    run_date=$(echo "$entry" | cut -d'|' -f3)
    local display="$run_name ($run_date)"
    display_lines+=("$display")
    entry_map["$display"]="$run_path"
  done

  local selected
  selected=$(printf '%s\n' "${display_lines[@]}" | gum filter --placeholder "Select a report run...")
  if [[ -z "$selected" ]]; then
    return 1
  fi

  report_path="${entry_map[$selected]}"
  if [[ -z "$report_path" ]]; then
    echo "Error: Report selection lookup failed" >&2
    return 1
  fi
}

resolve_report_dir() {
  if [[ -n "$report_path" ]]; then
    if [[ -d "$report_path" ]]; then
      return 0
    fi

    local spec_base
    spec_base=$(basename "$spec_path")
    local candidate="$reports_dir/$spec_base/$report_path"
    if [[ -d "$candidate" ]]; then
      report_path="$candidate"
      return 0
    fi

    echo "Error: Report run not found: $report_path" >&2
    return 1
  fi

  select_report_run
}

validate_report_files() {
  local template_review="$report_path/template-review.md"
  local solution_review="$report_path/solution-review.md"
  local context_review="$report_path/context-review.md"

  if [[ ! -f "$template_review" ]]; then
    echo "Error: Missing template review: $template_review" >&2
    return 1
  fi
  if [[ ! -f "$solution_review" ]]; then
    echo "Error: Missing solution review: $solution_review" >&2
    return 1
  fi
  if [[ ! -f "$context_review" ]]; then
    echo "Error: Missing context review: $context_review" >&2
    return 1
  fi
}

build_prompt() {
  cat <<'EOF'
@SPEC_PATH @PLAN_PATH @specs/README.md @specs/planning/SPEC_AUTHORING.md
@TEMPLATE_REVIEW @SOLUTION_REVIEW @CONTEXT_REVIEW

You are a spec editor. Apply the review findings to update the spec, plan, and index.

Order of work (required):
1) Apply context references: add or update a "### References" subsection under
   "## 1. Overview" with the required files from the context review. Keep it brief
   and add one-line reasons per file.
2) Fix template/format issues from the template review.
3) Improve solution clarity and remove ambiguity per the solution review.
4) Update the plan to match the updated spec sections and citations.

Constraints:
- Edit only the spec file, the plan file, and specs/README.md.
- Keep section numbering stable; do not add new top-level sections.
- If a review item conflicts with SPEC_AUTHORING.md, follow SPEC_AUTHORING.md.
- Keep edits minimal and focused on the review findings.

Return:
- Short summary of edits.
- List of files changed.
EOF
}

render_prompt() {
  local prompt="$1"
  prompt=${prompt//SPEC_PATH/$spec_path}
  prompt=${prompt//PLAN_PATH/$plan_path}
  prompt=${prompt//TEMPLATE_REVIEW/$report_path/template-review.md}
  prompt=${prompt//SOLUTION_REVIEW/$report_path/solution-review.md}
  prompt=${prompt//CONTEXT_REVIEW/$report_path/context-review.md}
  printf '%s' "$prompt"
}

main() {
  parse_args "$@"
  validate_spec_and_plan
  resolve_report_dir
  validate_report_files

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

  ui_header "Spec Review Apply"
  ui_status "Run ID:     $RUN_ID"
  ui_status "Spec:       $spec_path"
  ui_status "Plan:       $plan_path"
  ui_status "Report:     $report_path"
  ui_status "Model:      $model"
  ui_status "Run dir:    $RUN_DIR"
  ui_status "Gum:        $GUM_ENABLED"

  local prompt
  prompt=$(render_prompt "$(build_prompt)")

  write_prompt_snapshot "$prompt"

  local result=""
  local claude_exit=0
  run_claude_iteration "1" "$prompt" "$model" result || claude_exit=$?

  printf '%s\n' "$result"

  if ((claude_exit != 0)); then
    show_run_summary "claude_failed" "$claude_exit"
    exit "$claude_exit"
  fi

  show_run_summary "complete" "0"
}

if [[ "${BASH_SOURCE[0]}" == "$0" ]]; then
  main "$@"
fi
