#!/usr/bin/env bash

set -euo pipefail

if [ $# -lt 1 ]; then
  echo "Usage: $0 <spec-path> [plan-path]"
  exit 1
fi

spec_path="$1"
iterations="50"
plan_path="${2:-}"

if [ -z "$plan_path" ]; then
  spec_base=$(basename "$spec_path")
  plan_path="specs/planning/${spec_base%.md}-plan.md"
fi

if [ ! -f "$spec_path" ]; then
  echo "Spec not found: $spec_path"
  exit 1
fi

if [ ! -f "$plan_path" ]; then
  echo "Plan not found: $plan_path"
  exit 1
fi

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

for ((i=1; i<=iterations; i++)); do
  result=$(claude --dangerously-skip-permissions -p "$prompt")
  trimmed_result="$result"
  trimmed_result="${trimmed_result#"${trimmed_result%%[!$'\t\n\r ']*}"}"
  trimmed_result="${trimmed_result%"${trimmed_result##*[!$'\t\n\r ']}"}"

  if [[ "$trimmed_result" == "<promise>COMPLETE</promise>" ]]; then
    printf '%s\n' "$trimmed_result"
    exit 0
  fi

  printf '%s\n' "$result"
done
