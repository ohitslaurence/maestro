#!/usr/bin/env bash
# Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
# SPDX-License-Identifier: Proprietary

# Deploy workbench machine

# shellcheck source=../lib.sh
source "$(dirname "$0")/../lib.sh"

main() {
    script_header "Deploying loom machine"
    
    validate_devenv
    require_command nixos-rebuild
    
    step_start "Deploying loom machine"
    log_warn "This will make changes to the local system"
    run_cmd_verbose "nixos-rebuild switch --flake .#virtualMachine --sudo --verbose"
    step_complete "Deployed loom machine"
    
    script_footer
}

main "$@"