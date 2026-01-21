#!/usr/bin/env bash
# Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
# SPDX-License-Identifier: Proprietary

# Format entire project with dprint

# shellcheck source=../lib.sh
source "$(dirname "$0")/../lib.sh"

main() {
    script_header "Formatting entire project with dprint"
    
    validate_devenv
    require_command dprint
    
    step_start "Formatting entire project with dprint"
    run_cmd_verbose "dprint fmt"
    step_complete "dprint formatting completed"
    
    script_footer
}

main "$@"
