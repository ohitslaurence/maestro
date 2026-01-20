# Task: Extract Sessions Feature

## Objective

Extract session list logic from App.tsx into `features/sessions/` following the feature-sliced pattern.

## Current State

App.tsx contains inline:
- `sessions` state and `setSessions`
- `selectedSession` state and `setSelectedSession`
- `useEffect` calling `listSessions()`
- Session list JSX in sidebar

## Output

```
app/src/features/sessions/
  components/
    SessionList.tsx      # Presentational list component
  hooks/
    useSessions.ts       # State + listSessions effect
  index.ts               # Barrel export
```

## Implementation Details

### useSessions.ts

```typescript
import { useState, useEffect } from "react";
import { listSessions } from "../../services/tauri";

export interface SessionsState {
  sessions: string[];
  selectedSession: string | null;
  selectSession: (id: string | null) => void;
}

export function useSessions(): SessionsState {
  const [sessions, setSessions] = useState<string[]>([]);
  const [selectedSession, setSelectedSession] = useState<string | null>(null);

  useEffect(() => {
    listSessions().then(setSessions).catch(console.error);
  }, []);

  return {
    sessions,
    selectedSession,
    selectSession: setSelectedSession,
  };
}
```

### SessionList.tsx

```typescript
interface SessionListProps {
  sessions: string[];
  selectedSession: string | null;
  onSelectSession: (id: string) => void;
}

export function SessionList({ sessions, selectedSession, onSelectSession }: SessionListProps) {
  return (
    <nav className="session-list">
      <h2>Sessions</h2>
      {sessions.length === 0 ? (
        <p className="empty">No active sessions</p>
      ) : (
        <ul>
          {sessions.map((session) => (
            <li
              key={session}
              className={selectedSession === session ? "selected" : ""}
              onClick={() => onSelectSession(session)}
            >
              {session}
            </li>
          ))}
        </ul>
      )}
    </nav>
  );
}
```

### index.ts

```typescript
export { SessionList } from "./components/SessionList";
export { useSessions } from "./hooks/useSessions";
export type { SessionsState } from "./hooks/useSessions";
```

### Updated App.tsx

```typescript
import { useSessions, SessionList } from "./features/sessions";

function App() {
  const { sessions, selectedSession, selectSession } = useSessions();
  // ...

  return (
    <aside className="sidebar">
      <div className="sidebar-header">
        <h1>Orchestrator</h1>
      </div>
      <SessionList
        sessions={sessions}
        selectedSession={selectedSession}
        onSelectSession={selectSession}
      />
    </aside>
    // ...
  );
}
```

## Implementation Steps

1. Create `features/sessions/` directory structure
2. Create `useSessions.ts` hook
3. Create `SessionList.tsx` component
4. Create `index.ts` barrel export
5. Update App.tsx to use the new feature
6. Remove inline session logic from App.tsx

## Acceptance Criteria

- [ ] `features/sessions/` follows same pattern as `features/terminal/`
- [ ] App.tsx imports from barrel export
- [ ] No session logic remains inline in App.tsx
- [ ] `bun run typecheck` passes
