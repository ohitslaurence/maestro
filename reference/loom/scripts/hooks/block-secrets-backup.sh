#!/usr/bin/env bash
set -euo pipefail

if git diff --cached --name-only | grep -E 'secrets/.*\.(backup|bak)$'; then
  echo "🚨 CRITICAL: Encrypted secrets backup file detected!"
  echo "These files contain encrypted secrets and MUST NOT be committed."
  exit 1
fi
