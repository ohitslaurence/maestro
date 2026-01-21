# Maestro Specifications

This index maps durable system specs to their implementation plans and code locations.
Keep this file current whenever a new spec or plan is added.

## Core Specs

| Spec | Plan | Code | Purpose |
| --- | --- | --- | --- |
| [orchestrator.md](./orchestrator.md) | -- | -- | System architecture overview for the orchestrator |
| [ROADMAP.md](./ROADMAP.md) | -- | -- | Delivery phases and milestones |
| [agent-state-machine.md](./agent-state-machine.md) | [planning/agent-state-machine-plan.md](./planning/agent-state-machine-plan.md) | app/src-tauri/src/sessions.rs | Deterministic agent state machine + post-tool hooks |
| [streaming-event-schema.md](./streaming-event-schema.md) | [planning/streaming-event-schema-plan.md](./planning/streaming-event-schema-plan.md) | app/src-tauri/src/sessions.rs | Unified streaming event schema |
| [session-persistence.md](./session-persistence.md) | [planning/session-persistence-plan.md](./planning/session-persistence-plan.md) | app/src-tauri/src/storage | Local-first session persistence + sync queue |

## Research Notes

| Spec | Plan | Code | Purpose |
| --- | --- | --- | --- |
| [CODEX_MONITOR_ANALYSIS.md](./research/CODEX_MONITOR_ANALYSIS.md) | [planning/codex-monitor-analysis-plan.md](./planning/codex-monitor-analysis-plan.md) | -- | Reference study of CodexMonitor |
| [OPENCODE_RESEARCH.md](./research/OPENCODE_RESEARCH.md) | -- | -- | OpenCode protocol research |
| [CLAUDE_CODE_RESEARCH.md](./research/CLAUDE_CODE_RESEARCH.md) | -- | -- | Claude Code SDK research |
| [LOOM_RESEARCH.md](./research/LOOM_RESEARCH.md) | -- | -- | Loom platform analysis for agent orchestration |

## Planning Conventions

- Plans live in `specs/planning/` and should be linked here once created.
- Specs live in `specs/` and remain stable; plans evolve as work is completed.
