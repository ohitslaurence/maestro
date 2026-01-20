# Task: OpenCode Thread UI Parity

## Objective

Bring the Maestro OpenCode thread UI to parity with the OpenCode TUI for session lifecycle, model config, and part rendering.

## Background

The current OpenCode thread UI renders a subset of message parts and only supports text prompts. The daemon/server bridge also drops model/provider metadata and advanced part types that the OpenCode TUI supports. Parity here means matching the OpenCode TUI surface area that affects thread view fidelity, configuration, and session workflows.

## Parity checklist

### Session lifecycle
- List sessions and resume from history
- Show session metadata (title, updated, model/provider)
- Support session deletion if OpenCode server exposes it
- Support session fork/branch if OpenCode server exposes it

### Prompt configuration
- Allow selecting provider/model per session
- Allow sending prompt parts beyond text (when applicable)
- Expose "thinking" / reasoning configuration if OpenCode supports it

### Thread rendering
- Render additional parts: `file`, `snapshot`, `compaction`, `agent`, `subtask`, `retry`
- Surface permission/approval items if present
- Render diff/patch items with preview and link to full diff
- Show tool status with timestamps and error states

### Event handling
- Merge both `message.updated` and `message.part.updated`
- Handle `step-start` / `step-finish` to drive processing state
- Handle streamed content updates without dropping partials

## Scope

### Backend (daemon + tauri)
- Extend OpenCode RPC prompt to pass provider/model + parts
- Add session detail, delete, fork endpoints if supported by OpenCode server
- Include session metadata in session list responses

### Frontend
- Session list/resume UI entry point
- Composer controls for provider/model and reasoning options
- Thread item renderers for missing part types

## Implementation plan

1. Extend daemon OpenCode protocol to accept provider/model and parts
2. Update Tauri command + frontend service wrappers
3. Update thread hook to track missing part types and new state fields
4. Add session list UI and route for resuming sessions
5. Extend composer to capture model/provider + reasoning options
6. Render additional part types with collapsible previews
7. Add loading/processing state from `step-start` and `step-finish`

## Files to touch

Backend
- `daemon/src/protocol.rs`
- `daemon/src/handlers/opencode.rs`
- `daemon/src/opencode.rs`
- `app/src-tauri/src/daemon/protocol.rs`
- `app/src-tauri/src/daemon/commands.rs`

Frontend
- `app/src/features/opencode/hooks/useOpenCodeThread.ts`
- `app/src/features/opencode/components/ThreadView.tsx`
- `app/src/features/opencode/components/ThreadComposer.tsx`
- `app/src/services/tauri.ts`
- `app/src/types.ts`

## Acceptance criteria

- OpenCode sessions can be listed and resumed in the UI
- Prompts can pass provider/model configuration to the server
- Reasoning configuration is user-controllable when supported
- Additional part types are rendered with sensible defaults
- Processing state reflects step-start/step-finish events

## Open questions

- Which OpenCode TUI features are out of scope for initial parity (e.g., permissions UI, full diff viewer)?
- Should model/provider selection be per-session, per-message, or global?
- Are session delete/fork required for parity MVP?
