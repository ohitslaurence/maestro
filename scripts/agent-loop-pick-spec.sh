#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# shellcheck source=lib/agent-loop-ui.sh
source "$SCRIPT_DIR/lib/agent-loop-ui.sh"

# shellcheck source=lib/spec-picker.sh
source "$SCRIPT_DIR/lib/spec-picker.sh"

if ! spec_picker; then
  exit 1
fi

printf '%s|%s\n' "$PICKED_SPEC_PATH" "$PICKED_PLAN_PATH"
