#!/usr/bin/env bash
set -euo pipefail

echo "🔄 Regenerating Cargo.nix..."
echo "yes" | nix run .#cargo2nix 2>/dev/null || true

if ! git diff --quiet Cargo.nix 2>/dev/null; then
  git add Cargo.nix
  echo "✅ Cargo.nix updated and staged"
else
  echo "✅ Cargo.nix is up to date"
fi
