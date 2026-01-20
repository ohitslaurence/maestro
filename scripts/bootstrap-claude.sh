#!/usr/bin/env bash
#
# Creates CLAUDE.md symlinks pointing to AGENTS.md files.
# Claude Code reads CLAUDE.md, other agents read AGENTS.md - this keeps them in sync.
#
# Usage: ./scripts/bootstrap-claude.sh
#
# Idempotent - safe to run multiple times.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

created=0
exists=0
skipped=0
removed=0

# Find all AGENTS.md files (excluding node_modules)
while IFS= read -r agents_file; do
  dir="$(dirname "$agents_file")"
  claude_file="$dir/CLAUDE.md"

  if [[ -L "$claude_file" ]]; then
    # Symlink exists - verify it points to AGENTS.md
    target="$(readlink "$claude_file")"
    if [[ "$target" == "AGENTS.md" ]]; then
      exists=$((exists + 1))
    else
      echo "WARN: $claude_file is a symlink to '$target', not AGENTS.md - skipping"
      skipped=$((skipped + 1))
    fi
  elif [[ -e "$claude_file" ]]; then
    # Regular file exists - don't overwrite
    echo "WARN: $claude_file exists as a regular file - skipping"
    skipped=$((skipped + 1))
  else
    # Create symlink
    ln -s AGENTS.md "$claude_file"
    echo "Created: $claude_file -> AGENTS.md"
    created=$((created + 1))
  fi
done < <(find . -name "AGENTS.md" -not -path "*/node_modules/*")

# Find orphaned CLAUDE.md symlinks (symlinks pointing to AGENTS.md where AGENTS.md doesn't exist)
while IFS= read -r claude_file; do
  if [[ -L "$claude_file" ]]; then
    target="$(readlink "$claude_file")"
    if [[ "$target" == "AGENTS.md" ]]; then
      dir="$(dirname "$claude_file")"
      agents_file="$dir/AGENTS.md"
      if [[ ! -e "$agents_file" ]]; then
        rm "$claude_file"
        echo "Removed orphan: $claude_file (AGENTS.md no longer exists)"
        removed=$((removed + 1))
      fi
    fi
  fi
done < <(find . -name "CLAUDE.md" -not -path "*/node_modules/*")

echo ""
echo "Done: $created created, $exists already correct, $skipped skipped, $removed orphans removed"
