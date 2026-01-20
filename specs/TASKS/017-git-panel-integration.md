# Task 017: Git Panel Integration

## Status: DONE

## Objective

Wire the existing git UI components (`GitStatusPanel`, `DiffViewer`) into the main app so users can view git status and diffs for the selected session.

## Background

We have:
- `GitStatusPanel` component - displays branch, staged/unstaged files
- `DiffViewer` component - renders file diffs with syntax highlighting
- `useGitStatus` hook - fetches git status from daemon
- `useGitDiffs` hook - fetches diffs from daemon
- Daemon endpoints: `git_status`, `git_diff`, `git_log`

Missing:
- Git panel not wired into `App.tsx`
- No UI to toggle between terminal and git views
- No auto-refresh when session changes

## Design Questions

### Q1: Layout - Where does the git panel go?

**Option A: Tabs in main panel**
```
┌─────────┬──────────────────────────┐
│ Sessions│  [Terminal] [Git]        │
│         │  ┌──────────────────────┐│
│ > proj1 │  │                      ││
│   proj2 │  │   Terminal OR Git    ││
│         │  │                      ││
└─────────┴──────────────────────────┘
```
- Simple tab switch between terminal and git
- Only one view at a time

**Option B: Split main panel (horizontal)**
```
┌─────────┬──────────────────────────┐
│ Sessions│  Terminal                │
│         │  ┌──────────────────────┐│
│ > proj1 │  │ $ ...                ││
│   proj2 │  ├──────────────────────┤│
│         │  │ Git Status / Diff    ││
└─────────┴──────────────────────────┘
```
- See both at once
- Terminal gets less vertical space

**Option C: Right sidebar (like CodexMonitor)**
```
┌─────────┬─────────────────┬────────┐
│ Sessions│  Terminal       │ Git    │
│         │  ┌─────────────┐│ Status │
│ > proj1 │  │ $ ...       ││ Files  │
│   proj2 │  │             ││ Diff   │
└─────────┴─────────────────┴────────┘
```
- Three-column layout
- Git always visible
- More complex resize handling

**Recommendation:** Start with **Option A (tabs)** - simplest, can evolve later.

### Q2: Git panel content - What to show?

**Minimal (v1):**
- Branch name
- Staged files list
- Unstaged files list
- Click file → show diff

**Extended (v2):**
- File tree with status indicators
- Inline diff preview
- Commit history tab
- Stage/unstage actions (requires new daemon commands)

**Recommendation:** Start with **Minimal** - matches existing components.

### Q3: Auto-refresh behavior?

Options:
- Poll every N seconds (simple, uses bandwidth)
- Refresh on tab focus (saves bandwidth)
- Event-driven from daemon (requires daemon changes)

**Recommendation:** Refresh on tab switch + manual refresh button. Add polling later if needed.

### Q4: What if session has no git?

The daemon's `session_info` returns `has_git: boolean`. Options:
- Hide git tab entirely
- Show git tab but with "Not a git repository" message
- Gray out git tab

**Recommendation:** Show tab with message - consistent UI, clear feedback.

## Implementation Plan

### 1. Add tab state to App.tsx

```typescript
type MainPanelTab = 'terminal' | 'git';
const [activeTab, setActiveTab] = useState<MainPanelTab>('terminal');
```

### 2. Create tab bar component

```
app/src/features/layout/components/TabBar.tsx
```

Simple tab bar with Terminal/Git tabs. Style to match app shell.

### 3. Create GitPanel container component

```
app/src/features/git/components/GitPanel.tsx
```

Composes:
- `useGitStatus` for status data
- `useGitDiffs` for diff data
- `GitStatusPanel` for file list
- `DiffViewer` for selected file diff

### 4. Wire into App.tsx

- Add `useGitStatus` and `useGitDiffs` hooks
- Conditionally render `TerminalPanel` or `GitPanel` based on tab
- Pass `sessionId` to git hooks
- Refresh git data when tab switches to git

### 5. Handle no-git case

- Use `sessionInfo` to check `has_git`
- Show appropriate message in GitPanel

### 6. Add styles

- Tab bar styles
- Ensure git components use design tokens

## File Changes

```
app/src/
├── features/
│   ├── layout/
│   │   ├── components/
│   │   │   └── TabBar.tsx (new)
│   │   └── index.ts (update export)
│   └── git/
│       ├── components/
│       │   └── GitPanel.tsx (new)
│       └── index.ts (update export)
├── styles/
│   └── tabs.css (new)
└── App.tsx (update)
```

## Testing Checklist

- [ ] Tab bar renders with Terminal/Git tabs
- [ ] Clicking tab switches view
- [ ] Terminal state preserved when switching away and back
- [ ] Git status loads for selected session
- [ ] Clicking file shows diff
- [ ] Switching sessions refreshes git data
- [ ] No-git sessions show appropriate message
- [ ] Disconnected state handled gracefully

## Decisions

1. **Layout:** Tabs (Option A) - simple tab switch between Terminal/Git
2. **No-git sessions:** Show tab with message
3. **Scope:** Minimal v1 (branch, file list, click-to-diff)

## Future Considerations

- Command palette (user requested) - would allow quick switching, actions
- Right sidebar layout as app grows
- Commit history panel

## Dependencies

- Daemon must be running with git commands implemented (done)
- Session must have git repository for meaningful data

## Estimate

~2-3 hours for minimal implementation (tabs + wiring).
