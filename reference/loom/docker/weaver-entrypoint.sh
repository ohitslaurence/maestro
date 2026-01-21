#!/bin/sh
# Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
# SPDX-License-Identifier: Proprietary

# Weaver pod entrypoint script
# Clones a git repository if specified, then starts loom REPL

set -e

WORKSPACE="/workspace"

# Clone repository if LOOM_REPO is set
if [ -n "$LOOM_REPO" ]; then
	echo "Cloning $LOOM_REPO..."
	
	if [ -n "$LOOM_BRANCH" ]; then
		git clone --branch "$LOOM_BRANCH" --single-branch "$LOOM_REPO" "$WORKSPACE"
	else
		git clone "$LOOM_REPO" "$WORKSPACE"
	fi
	
	cd "$WORKSPACE"
	echo "Cloning complete."
	echo ""
else
	mkdir -p "$WORKSPACE"
	cd "$WORKSPACE"
fi

# Start loom REPL
exec loom
