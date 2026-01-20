# Task: Implement Feature-Sliced Architecture

## Objective

Establish CodexMonitor's feature-sliced architecture pattern in Maestro. This provides clear separation between presentational components and stateful hooks.

## Reference

- `reference/codex-monitor/src/features/` - Feature structure
- `reference/codex-monitor/src/App.tsx` - Composition root

## Output

Restructure `app/src/` to follow feature-sliced pattern:

```
app/src/
  features/
    terminal/                  # ✓ DONE
      components/
        TerminalPanel.tsx      # Presentational
      hooks/
        useTerminalSession.ts  # State + effects
      index.ts                 # Barrel export
    git/
      components/
        GitStatusPanel.tsx
        DiffViewer.tsx
      hooks/
        useGitStatus.ts
        useGitDiffs.ts
      index.ts
    sessions/
      components/
        SessionList.tsx
      hooks/
        useSessions.ts
      index.ts
    layout/                    # ✓ DONE (partial)
      components/
        ResizeHandle.tsx       # Drag handle for panel resize
      hooks/
        useResizablePanels.ts  # Sidebar width state + mouse handlers
      index.ts
  services/
    tauri.ts                   # IPC wrappers
    events.ts                  # Event hub
  styles/
    base.css
    sidebar.css
    terminal.css
    diff-viewer.css
  types.ts                     # Shared types
  App.tsx                      # Composition root
  main.tsx                     # Entry point
```

## Implementation Details

### Design Principles

**Presentational Components:**
```typescript
// Only props → JSX
// No Tauri calls, no hooks (except useRef for DOM)
export function TerminalPanel({ containerRef, status }: TerminalPanelProps) {
  return (
    <div className="terminal-shell">
      <div ref={containerRef} className="terminal-surface" />
      {status !== "ready" && <div className="terminal-overlay">{status}</div>}
    </div>
  );
}
```

**Hooks Own State & Effects:**
```typescript
// Manages state, subscriptions, async operations
export function useTerminalSession(options: Options): TerminalSessionState {
  const [status, setStatus] = useState<TerminalStatus>("idle");
  const containerRef = useRef<HTMLDivElement | null>(null);

  useEffect(() => { /* initialize */ }, []);
  useEffect(() => { /* subscribe to events */ }, []);

  return { status, containerRef, /* ... */ };
}
```

**Composition Root (App.tsx):**
```typescript
function App() {
  // All hooks called here
  const { sessions } = useSessions();
  const { terminalState } = useTerminalSession({ /* ... */ });
  const { gitStatus } = useGitStatus(activeSession);

  // Pass state down as props
  return (
    <Layout
      sidebar={<Sidebar sessions={sessions} />}
      main={<MainPanel terminalState={terminalState} gitStatus={gitStatus} />}
    />
  );
}
```

### Key Patterns

1. **No Context API** - explicit props drilling
2. **Hooks compose with hooks** - composition over inheritance
3. **One CSS file per area** - in `src/styles/`
4. **Types in central file** - `src/types.ts`
5. **Barrel exports** - each feature has `index.ts` for clean imports:
   ```typescript
   // features/terminal/index.ts
   export { TerminalPanel } from "./components/TerminalPanel";
   export { useTerminalSession } from "./hooks/useTerminalSession";
   export type { TerminalSessionState } from "./hooks/useTerminalSession";
   ```
6. **Third-party CSS in hooks** - library CSS imported where used, not in `styles/`:
   ```typescript
   // In useTerminalSession.ts
   import "@xterm/xterm/css/xterm.css";
   ```

## Constraints

- Components: pure, presentational only
- Hooks: no JSX, just state/effects
- Services: no React, just async functions
- No business logic leaking into components

## Migration Steps

1. ~~Create directory structure~~ ✓
2. ~~Move existing code into appropriate locations~~ ✓ (terminal, layout)
3. Split any mixed component/logic into separate files
4. Update imports in App.tsx to use barrel exports

## Remaining Work

- [x] `features/git/` - GitStatusPanel, DiffViewer, hooks
- [x] `features/sessions/` - SessionList, useSessions
- [x] Split `styles.css` into per-area files in `styles/`
- [x] Wire terminal into App.tsx (currently placeholder)
