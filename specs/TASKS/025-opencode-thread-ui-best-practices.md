# Task: OpenCode Thread UI Best Practices

## Objective

Raise OpenCode thread state management + UI rendering to CodexMonitor-quality (or better) by addressing streaming merge, ordering, history merge, normalization, dedupe, processing UX, and rich rendering.

## Background

Maestro currently rebuilds thread items from live events and renders a basic message list. CodexMonitor uses a reducer-based pipeline, merges streamed deltas, bounds item size, and reconciles history with optimistic items. This task captures the deltas needed for a world-class thread experience.

Reference implementation:
- `reference/codex-monitor/src/features/threads/hooks/useThreadsReducer.ts`
- `reference/codex-monitor/src/utils/threadItems.ts`
- `reference/codex-monitor/src/features/messages/components/Messages.tsx`

Maestro touchpoints:
- `app/src/features/opencode/hooks/useOpenCodeThread.ts`
- `app/src/features/opencode/components/ThreadView.tsx`
- `app/src/features/opencode/components/ThreadMessages.tsx`
- `app/src/features/opencode/components/MessageRow.tsx`
- `app/src/features/opencode/components/ToolRow.tsx`

## Task 1: Streaming delta accumulation

### Goal
Ensure streamed parts render as a continuously growing buffer, not a series of partials.

### Scope
- Track a per-part text buffer in the thread state.
- On `message.part.updated`, append `delta` to existing text when present.
- Prefer canonical `part.text` when the event contains full text.

### References
- CodexMonitor: `reference/codex-monitor/src/utils/threadItems.ts` (`mergeStreamingText`)
- Maestro: `app/src/features/opencode/hooks/useOpenCodeThread.ts`

### Acceptance criteria
- Streaming assistant output grows smoothly without flicker or resets.
- Replays of the same stream produce identical final text.

## Task 2: Deterministic part ordering

### Goal
Render message parts in a stable, deterministic order (independent of event arrival).

### Scope
- Store `order` on each part (timestamp + stable index).
- Sort parts by `order` before item construction.
- Keep ordering stable across re-renders.

### References
- Maestro: `app/src/features/opencode/hooks/useOpenCodeThread.ts`

### Acceptance criteria
- Parts render in consistent order across reloads and rapid streaming.

## Task 3: History rehydrate + merge

### Goal
Merge persisted history with optimistic in-flight items and avoid duplicates.

### Scope
- Add a thread history fetch/resume path (session load) that returns prior items.
- Merge server items with pending UI items; prefer server items with richer content.
- De-dupe by stable message/part IDs or hash of content + timestamp.

### References
- CodexMonitor: `reference/codex-monitor/src/utils/threadItems.ts` (`mergeThreadItems`, `buildItemsFromThread`)
- Maestro: `app/src/features/opencode/hooks/useOpenCodeThread.ts`, `app/src/features/opencode/hooks/useOpenCodeSession.ts`

### Acceptance criteria
- Reloading a session reproduces the exact prior thread.
- Pending user messages remain visible until confirmed or merged.

## Task 4: Thread normalization + bounds

### Goal
Prevent UI degradation by bounding item count and large tool outputs.

### Scope
- Define max items per thread (e.g., 500).
- Truncate oversized text/tool payloads with a visible indicator.
- Keep a list of trimmed items for progressive reveal on demand.

### References
- CodexMonitor: `reference/codex-monitor/src/utils/threadItems.ts` (`prepareThreadItems`)
- Maestro: `app/src/features/opencode/hooks/useOpenCodeThread.ts`

### Acceptance criteria
- Long sessions remain responsive; no exponential re-render costs.
- Truncation is explicit and user-visible.

## Task 5: Optimistic send de-dupe

### Goal
Avoid duplicate user messages when server echoes the same content.

### Scope
- Generate deterministic client message IDs and send them with prompt requests.
- When server echoes a message, reconcile by ID and replace pending.
- If server lacks ID support, dedupe by `(role, text, createdAt)` within a short window.

### References
- Maestro: `app/src/features/opencode/components/ThreadView.tsx`, `app/src/features/opencode/hooks/useOpenCodeThread.ts`

### Acceptance criteria
- No duplicated user messages after server confirms receipt.

## Task 6: Processing duration UX

### Goal
Provide a clear processing indicator with elapsed time.

### Scope
- Track `processingStartedAt` and `lastDurationMs` in thread state.
- Render a working indicator with elapsed time (e.g., `Working... 12s`).
- Reset on idle and when a new processing cycle begins.

### References
- CodexMonitor: `reference/codex-monitor/src/features/messages/components/Messages.tsx`
- Maestro: `app/src/features/opencode/components/ThreadMessages.tsx`, `app/src/features/opencode/hooks/useOpenCodeThread.ts`

### Acceptance criteria
- Users see accurate processing time per turn.

## Task 7: Rich message rendering

### Goal
Upgrade rendering beyond `<pre>` to match (or exceed) CodexMonitor's UX.

### Scope
- Markdown rendering for text parts (inline code, code blocks, links).
- Tool summary row with expandable output.
- Diff rendering for patch/snapshot (collapsed preview + open full).
- Reasoning rendering with collapse/expand.

### References
- CodexMonitor: `reference/codex-monitor/src/features/messages/components/Messages.tsx`
- Maestro: `app/src/features/opencode/components/MessageRow.tsx`, `app/src/features/opencode/components/ToolRow.tsx`, `app/src/features/opencode/components/ReasoningRow.tsx`

### Acceptance criteria
- Rich parts are readable, scannable, and expandable.
- Tool output does not dominate the thread by default.

## Implementation plan (milestones)

1. Milestone 1: Core state pipeline
   - Introduce reducer-based thread state (or equivalent) to centralize merge logic.
   - Implement streaming merge + deterministic ordering.
   - Commit after milestone 1.
2. Milestone 2: History + bounds
   - Add history rehydrate + merge with optimistic items.
   - Implement normalization + bounds (item count + truncation).
   - Commit after milestone 2.
3. Milestone 3: UX correctness
   - Add optimistic send de-dupe.
   - Add processing duration tracking + UI.
   - Commit after milestone 3.
4. Milestone 4: Rich rendering
   - Implement markdown rendering, tool summaries, diff previews, reasoning collapse.
   - Commit after milestone 4.

## Open questions

- Should thread normalization keep a fixed tail window or a rolling summary of earlier items?
- Do we need server support to store client-provided message IDs?
- Is markdown rendering allowed to run without sanitization (or should we sanitize)?
