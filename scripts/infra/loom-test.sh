#!/usr/bin/env bash
# Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
# SPDX-License-Identifier: Proprietary

# Test loom configuration locally

# shellcheck source=../lib.sh
source "$(dirname "$0")/../lib.sh"

main() {
    script_header "Testing loom configuration"
    
    validate_devenv
    require_command nixos-rebuild
    
    step_start "Testing loom configuration locally"
    log_warn "This will make changes to the local system"
    run_cmd_verbose "nixos-rebuild test --flake .#virtualMachine --sudo --verbose"
    step_complete "Tested loom configuration"
    
    script_footer
}

main "$@"