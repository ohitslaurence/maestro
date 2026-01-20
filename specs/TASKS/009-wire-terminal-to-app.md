# Task: Wire Terminal into App.tsx

## Objective

Replace the terminal placeholder in App.tsx with the actual TerminalPanel component, wired to useTerminalSession.

## Current State

- `features/terminal/` exists with `TerminalPanel` and `useTerminalSession`
- App.tsx:45-47 shows a placeholder div instead of real terminal
- Terminal should render when a session is selected

## Output

App.tsx uses the terminal feature:

```typescript
import { TerminalPanel, useTerminalSession } from "./features/terminal";

function App() {
  // ...existing state...
  const terminal = useTerminalSession({ sessionId: selectedSession });

  return (
    // ...
    <main className="main-panel">
      {selectedSession ? (
        <div className="session-view">
          <TerminalPanel
            containerRef={terminal.containerRef}
            status={terminal.status}
          />
        </div>
      ) : (
        <div className="welcome">...</div>
      )}
    </main>
  );
}
```

## Implementation Steps

1. Import `TerminalPanel` and `useTerminalSession` from `./features/terminal`
2. Call `useTerminalSession` with selected session context
3. Replace `.terminal-placeholder` div with `<TerminalPanel />`
4. Remove unused welcome h2/session name display if terminal fills the view
5. Verify terminal initializes when session selected

## Acceptance Criteria

- [ ] Terminal renders in main panel when session selected
- [ ] Terminal connects to PTY backend
- [ ] No placeholder div remains
- [ ] `bun run typecheck` passes
