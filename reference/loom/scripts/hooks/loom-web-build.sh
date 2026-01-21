#!/usr/bin/env bash
set -euo pipefail

echo "🔨 Building loom-web..."
export NIXPKGS_ALLOW_UNFREE=1

if ! nix build .#loom-web --no-link --impure 2>&1; then
  echo "❌ BLOCKED: loom-web failed to compile!"
  echo "Fix the build errors before committing."
  exit 1
fi

echo "✅ loom-web builds successfully"
