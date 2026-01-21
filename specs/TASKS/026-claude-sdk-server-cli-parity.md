# Task: Claude SDK Server CLI Parity

## Objective

Close the gap between the Claude SDK server and the Claude Code CLI experience by adding missing event types, permission flows, and tool/PTY fidelity so the OpenCode UI can render a near‑CLI experience.

## Scope

- Load Claude Code settings/presets by default (CLI parity).
- Emit richer part types: tool, reasoning, agent, compaction, step/retry where applicable.
- Permission lifecycle: `permission.asked` + `permission.replied` handling.
- Map tool use/result blocks and hook events into OpenCode-compatible parts.
- Persist all new part types and metadata for replay.
- Surface usage/cost metadata per turn.

## Out of scope

- Full PTY streaming and raw CLI terminal replay (future task).
- UI rendering upgrades (handled by thread UI parity tasks).

## Acceptance criteria

- Claude SDK sessions render tool calls, results, and reasoning like CLI.
- Permission prompts appear and can be answered end‑to‑end.
- History replay includes all part types with correct ordering.
- Usage/cost is persisted and visible to the UI.

## Implementation notes

- Prefer SDK hooks (`PreToolUse`, `PostToolUse`, `PermissionRequest`, `Subagent*`, `PreCompact`).
- Use Claude Code presets: `systemPrompt` + `tools` preset + `settingSources`.
- Ensure every emitted event has a persisted record.
