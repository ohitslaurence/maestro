#!/usr/bin/env bats

load "test_helper/bats-support/load"
load "test_helper/bats-assert/load"

setup() {
  export SCRIPT_DIR="$BATS_TEST_DIRNAME/../scripts"
  export spec_path=""
  export plan_path=""
  export iterations=50
  export log_dir="logs/agent-loop"
  export no_gum=false
  export summary_json=true
  export no_wait=false
  export model="opus"
  export postmortem=true
  export completion_mode="exact"
  source "$SCRIPT_DIR/agent-loop.sh"
}

@test "parse_args sets positional spec and plan" {
  parse_args "specs/foo.md" "specs/planning/foo-plan.md"
  assert_equal "$spec_path" "specs/foo.md"
  assert_equal "$plan_path" "specs/planning/foo-plan.md"
}

@test "parse_args handles options" {
  parse_args --iterations 3 --model sonnet --completion-mode fuzzy --no-gum
  assert_equal "$iterations" "3"
  assert_equal "$model" "sonnet"
  assert_equal "$completion_mode" "fuzzy"
  assert_equal "$no_gum" "true"
}

@test "validate_inputs rejects invalid iterations" {
  spec_path="$BATS_TEST_DIRNAME/fixtures/good-spec.md"
  plan_path="$BATS_TEST_DIRNAME/fixtures/good-plan.md"
  iterations="abc"
  run validate_inputs
  assert_failure
  assert_output --partial "--iterations must be a positive integer"
}

@test "validate_inputs rejects invalid completion mode" {
  spec_path="$BATS_TEST_DIRNAME/fixtures/good-spec.md"
  plan_path="$BATS_TEST_DIRNAME/fixtures/good-plan.md"
  completion_mode="nearby"
  run validate_inputs
  assert_failure
  assert_output --partial "--completion-mode must be 'exact' or 'fuzzy'"
}

@test "validate_inputs requires spec when gum disabled" {
  spec_path=""
  plan_path=""
  no_gum=true
  run validate_inputs
  assert_failure
  assert_output --partial "spec-path is required"
}
