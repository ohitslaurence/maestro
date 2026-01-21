# Agent Loop Terminal UX Implementation Plan

Reference: [agent-loop-terminal-ux.md](../agent-loop-terminal-ux.md)

## Phase 1: Gum scaffolding + logging layout
- [x] Add gum dependency checks, log directory creation, and run header output (see 2.1, 2.3, 4).
- [x] Create UI helper module and wire into `scripts/agent-loop.sh` for headers/status lines (see 2.1, 2.3, 4.2).
- [x] Add spec discovery + gum filter selection with Last Updated sorting when spec path is omitted (see 2.1, 2.3, 3.1, 4.1, 5.1).

## Phase 2: Live iteration feedback
- [x] Wrap `claude` execution with gum spinner and capture per-iteration logs + stats (see 3.1, 3.2, 5.1).
- [x] Emit completion detection (strict/lenient) and non-zero exit handling with summary output (see 5.1, 5.2, 6).

## Phase 3: Summary + fallback modes
- [x] Add run summary table, completion screen with optional wait, and summary JSON output (see 3.2, 4.1, 7).
- [ ] Implement signal traps and `--no-gum` fallback for non-TTY runs (see 5.2, 6).

## Files to Create
- `scripts/lib/agent-loop-ui.sh`
- `scripts/lib/spec-picker.sh`

## Files to Modify
- `scripts/agent-loop.sh`
- `specs/README.md`

## Verification Checklist
- [ ] Manual: run `scripts/agent-loop.sh <spec> <plan>` and confirm spinner/status/summary output.
- [ ] Manual: run with `--no-gum` and verify plain output + logs.
