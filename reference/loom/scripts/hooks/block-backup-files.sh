#!/usr/bin/env bash
set -euo pipefail

if git diff --cached --name-only | grep -E '\.(backup|bak|tmp|temp|orig|copy|swp)$|~$'; then
  echo "❌ BLOCKED: Backup/temp files detected in commit!"
  echo "These files might contain secrets and should not be committed."
  exit 1
fi
