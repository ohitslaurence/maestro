#!/usr/bin/env bash
# agent-loop-tui.sh - simple wrapper for tools/agent-loop-tui

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
MANIFEST_PATH="$REPO_ROOT/tools/agent-loop-tui/Cargo.toml"

tui_args=()
script_args=()

usage() {
  cat <<EOF
Usage: $(basename "$0") [--log-dir <path>] [--script <path>] [--] <agent-loop-args>

Examples:
  $(basename "$0") specs/my-spec.md
  $(basename "$0") --log-dir /tmp/agent-loop -- specs/my-spec.md --iterations 5

Notes:
  - Arguments after "--" are forwarded to scripts/agent-loop.sh
  - Use --log-dir to keep logs outside the repo
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --log-dir|--script)
      if [[ $# -lt 2 ]]; then
        echo "Error: $1 requires a value" >&2
        exit 1
      fi
      tui_args+=("$1" "$2")
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    --)
      shift
      while [[ $# -gt 0 ]]; do
        script_args+=("$1")
        shift
      done
      ;;
    *)
      script_args+=("$1")
      shift
      ;;
  esac
done

has_spec=false
for arg in "${script_args[@]}"; do
  if [[ "$arg" != -* ]]; then
    has_spec=true
    break
  fi
done

if [[ "$has_spec" == "false" ]]; then
  if picked_spec=$("$SCRIPT_DIR/agent-loop-pick-spec.sh"); then
    IFS='|' read -r spec_path plan_path <<< "$picked_spec"
    if [[ -z "$spec_path" ]]; then
      echo "Error: No spec selected" >&2
      exit 1
    fi
    original_args=("${script_args[@]}")
    script_args=("$spec_path")
    if [[ -n "$plan_path" ]]; then
      script_args+=("$plan_path")
    fi
    script_args+=("${original_args[@]}")
  else
    echo "Error: Spec selection canceled" >&2
    exit 1
  fi
fi

exec cargo run --manifest-path "$MANIFEST_PATH" -- "${tui_args[@]}" -- "${script_args[@]}"
