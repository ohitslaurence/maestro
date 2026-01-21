#!/usr/bin/env bash
# Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
# SPDX-License-Identifier: Proprietary

# Check format for entire project with dprint

# shellcheck source=../lib.sh
source "$(dirname "$0")/../lib.sh"

main() {
    script_header "Checking format for entire project with dprint"
    
    validate_devenv
    require_command dprint
    
    step_start "Checking format for entire project with dprint"
    run_cmd_verbose "dprint check"
    step_complete "dprint format check passed"
    
    script_footer
}

main "$@"
