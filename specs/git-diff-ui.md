# Git Diff UI

**Status:** Implemented
**Version:** 1.1
**Last Updated:** 2026-01-21

---

## 1. Overview

### Purpose
Replace the current basic diff viewer with a professional-grade git diff UI matching CodexMonitor's implementation. The new UI provides split/unified diff views, virtualized file lists, and polished styling.

### Goals
- Side-by-side and stacked (unified) diff toggle with localStorage persistence
- Scrollable file list with staged/unstaged sections
- Status badges (A/M/D/R/T) with distinct colors
- File path display with split name/extension styling
- Additions/deletions stats per file
- Virtualized rendering for large changesets
- Syntax highlighting in diffs via `@pierre/diffs`
- Bidirectional scroll sync between file list and diff viewer
- Expanded keyboard navigation (arrows, j/k, Home/End)

### Non-Goals
- Git operations (stage/unstage/revert) - future phase
- PR/issues/log tabs - future phase
- Context menus - future phase
- GitHub integration - future phase

---

## 2. Architecture

### Components
```
GitPanel (container) - app/src/features/git/components/GitPanel.tsx
├── GitDiffPanel (left sidebar) - NEW
│   ├── Branch header with name
│   ├── Staged files section (collapsible)
│   │   └── FileRow[] with status, name, stats
│   └── Unstaged files section (collapsible)
│       └── FileRow[] with status, name, stats
└── GitDiffViewer (main viewport) - NEW
    ├── Sticky file header (current file path)
    ├── Diff style toggle (split/unified)
    └── Virtualized diff list
        └── DiffCard[] (one per file, uses @pierre/diffs FileDiff)
```

### Dependencies

| Package | Version | Purpose |
|---------|---------|---------|
| `@pierre/diffs` | ^1.0.6 | Diff rendering with split/unified views, syntax highlighting |
| `@tanstack/react-virtual` | ^3.13.18 | Virtualized list rendering for performance |

### Module/Folder Layout
```
app/src/features/git/
├── components/
│   ├── GitPanel.tsx         # Container - MODIFY
│   ├── GitDiffPanel.tsx     # Left sidebar file list - NEW
│   ├── GitDiffViewer.tsx    # Main diff viewport - NEW
│   ├── GitStatusPanel.tsx   # DELETE after migration
│   └── DiffViewer.tsx       # DELETE after migration
├── hooks/
│   ├── useGitStatus.ts      # Unchanged
│   ├── useGitDiffs.ts       # Unchanged
│   └── useDiffStyle.ts      # NEW - localStorage persistence
└── index.ts                 # Update exports

app/src/styles/
├── diff-viewer.css          # EXISTS - update with new styles
└── (no new files needed)
```

---

## 3. Data Model

### Core Types
Types already exist in `app/src/types.ts` (lines 59-123):

```typescript
// From app/src/types.ts - DO NOT MODIFY
type GitFileStatus = {
  path: string;
  status: "A" | "M" | "D" | "R" | "T";
  additions: number;
  deletions: number;
};

type GitFileDiff = {
  path: string;
  diff: string; // Unified diff format from git
};

type GitStatusResult = {
  branchName: string;
  stagedFiles: GitFileStatus[];
  unstagedFiles: GitFileStatus[];
  totalAdditions: number;
  totalDeletions: number;
};
```

### New Types (add to components or types.ts)
```typescript
type DiffStyle = "split" | "unified";

type FileRowData = GitFileStatus & {
  isSelected: boolean;
};
```

### Storage Schema
```typescript
// localStorage key: "maestro:diffStyle"
// Value: "split" | "unified"
// Default: "split"
```

---

## 4. Interfaces

### GitDiffPanel Props
```typescript
type GitDiffPanelProps = {
  branchName: string;
  stagedFiles: GitFileStatus[];
  unstagedFiles: GitFileStatus[];
  selectedPath: string | null;
  onSelectPath: (path: string) => void;
  totalAdditions: number;
  totalDeletions: number;
  isLoading: boolean;
};
```

### GitDiffViewer Props
```typescript
type GitDiffViewerProps = {
  diffs: GitFileDiff[];
  selectedPath: string | null;
  scrollRequestId: number; // Increment to trigger scroll-to-file
  isLoading: boolean;
  error: string | null;
  diffStyle: DiffStyle;
  onDiffStyleChange: (style: DiffStyle) => void;
  onActivePathChange: (path: string) => void;
};
```

### useDiffStyle Hook
```typescript
function useDiffStyle(): {
  diffStyle: DiffStyle;
  setDiffStyle: (style: DiffStyle) => void;
};
```

---

## 5. Workflows

### 5.1 Main Flow
1. User opens Git tab
2. `useGitStatus` fetches staged/unstaged files (existing, polls every 3s)
3. `useGitDiffs` fetches diff content for all files (existing)
4. `useDiffStyle` loads preference from localStorage
5. GitDiffPanel renders file list in left sidebar
6. GitDiffViewer renders all diffs in virtualized scroll container
7. First file auto-selected if none selected

### 5.2 File Selection Flow
1. User clicks file in GitDiffPanel
2. `onSelectPath(path)` called
3. Parent increments `scrollRequestId`
4. GitDiffViewer receives new `selectedPath` + `scrollRequestId`
5. `useEffect` triggers `rowVirtualizer.scrollToIndex()`
6. Sets `ignoreActivePathUntil = Date.now() + 250` to prevent scroll sync fighting

### 5.3 Scroll Sync Flow (Viewer → Panel)
1. User scrolls in GitDiffViewer
2. `scroll` event handler queues `requestAnimationFrame`
3. On frame: check if `Date.now() < ignoreActivePathUntil`, if so skip
4. Calculate which file is at scroll position + 8px offset
5. If different from current `activePath`, call `onActivePathChange(path)`
6. GitDiffPanel highlights the new active file

### 5.4 Diff Style Toggle
1. User clicks "Split"/"Unified" toggle button
2. `onDiffStyleChange(newStyle)` called
3. `useDiffStyle` persists to localStorage
4. GitDiffViewer re-renders with new `diffStyle` passed to `@pierre/diffs`

### 5.5 Keyboard Navigation
| Key | Action |
|-----|--------|
| `ArrowDown` / `j` | Select next file |
| `ArrowUp` / `k` | Select previous file |
| `Home` | Select first file |
| `End` | Select last file |

Navigation wraps at boundaries. Keys ignored when focus is in input/textarea.

### 5.6 Edge Cases
| Condition | Behavior |
|-----------|----------|
| Empty repository (no commits) | Show "No commits yet" in GitDiffPanel |
| No changes | Show "Working tree clean" in GitDiffPanel |
| Binary file | `@pierre/diffs` shows "Binary file" placeholder |
| Empty diff string | Show "No changes" in DiffCard |
| 1000+ files | Virtualization handles it; overscan=6 items |

---

## 6. Error Handling

### Error Display
| Error | Location | Display |
|-------|----------|---------|
| Git not initialized | GitDiffPanel | "Not a git repository" |
| Daemon disconnected | GitPanel | "Connection lost" |
| Diff fetch failed | GitDiffViewer | "Failed to load diffs" |

Errors display inline. No retry buttons in MVP - rely on polling.

---

## 7. Implementation Details

### 7.1 @pierre/diffs Integration

```tsx
import { FileDiff, WorkerPoolContextProvider } from "@pierre/diffs/react";
import { parsePatchFiles } from "@pierre/diffs";

// Worker pool for syntax highlighting (wrap GitDiffViewer)
const poolOptions = useMemo(() => ({ workerFactory }), []);
const highlighterOptions = useMemo(
  () => ({ theme: { dark: "pierre-dark", light: "pierre-light" } }),
  [],
);

<WorkerPoolContextProvider poolOptions={poolOptions} highlighterOptions={highlighterOptions}>
  {/* DiffCards here */}
</WorkerPoolContextProvider>

// Per-file diff rendering
const fileDiff = useMemo(() => {
  const patch = parsePatchFiles(entry.diff);
  return patch[0]?.files[0] ?? null;
}, [entry.diff]);

const diffOptions = useMemo(() => ({
  diffStyle: diffStyle, // "split" | "unified"
  hunkSeparators: "line-info" as const,
  overflow: "scroll" as const,
  disableFileHeader: true,
}), [diffStyle]);

<FileDiff fileDiff={fileDiff} options={diffOptions} />
```

### 7.2 Worker Factory
Create `app/src/utils/diffsWorker.ts`:
```typescript
export function workerFactory() {
  return new Worker(
    new URL("@pierre/diffs/worker", import.meta.url),
    { type: "module" }
  );
}
```

### 7.3 Virtualization Setup
```tsx
import { useVirtualizer } from "@tanstack/react-virtual";

const rowVirtualizer = useVirtualizer({
  count: diffs.length,
  getScrollElement: () => containerRef.current,
  estimateSize: () => 260, // Approximate height per diff card
  overscan: 6,
});

// Render only visible items
{rowVirtualizer.getVirtualItems().map((virtualItem) => (
  <div
    key={virtualItem.key}
    style={{
      position: "absolute",
      top: virtualItem.start,
      width: "100%",
    }}
  >
    <DiffCard entry={diffs[virtualItem.index]} />
  </div>
))}
```

### 7.4 Scroll-to-File Effect
```tsx
const ignoreActivePathUntilRef = useRef<number>(0);
const lastScrollRequestIdRef = useRef<number | null>(null);

useEffect(() => {
  if (!selectedPath || !scrollRequestId) return;
  if (lastScrollRequestIdRef.current === scrollRequestId) return;

  const index = indexByPath.get(selectedPath);
  if (index === undefined) return;

  ignoreActivePathUntilRef.current = Date.now() + 250;
  rowVirtualizer.scrollToIndex(index, { align: "start" });
  lastScrollRequestIdRef.current = scrollRequestId;
}, [selectedPath, scrollRequestId, indexByPath, rowVirtualizer]);
```

### 7.5 Active Path Tracking on Scroll
```tsx
useEffect(() => {
  const container = containerRef.current;
  if (!container || !onActivePathChange) return;

  let frameId: number | null = null;

  const updateActivePath = () => {
    frameId = null;
    if (Date.now() < ignoreActivePathUntilRef.current) return;

    const items = rowVirtualizer.getVirtualItems();
    if (!items.length) return;

    const scrollTop = container.scrollTop;
    const targetOffset = scrollTop + 8;

    let activeItem = items[0];
    for (const item of items) {
      if (item.start <= targetOffset) {
        activeItem = item;
      } else {
        break;
      }
    }

    const nextPath = diffs[activeItem.index]?.path;
    if (nextPath && nextPath !== activePathRef.current) {
      activePathRef.current = nextPath;
      onActivePathChange(nextPath);
    }
  };

  const handleScroll = () => {
    if (frameId !== null) return;
    frameId = requestAnimationFrame(updateActivePath);
  };

  container.addEventListener("scroll", handleScroll, { passive: true });
  return () => {
    if (frameId !== null) cancelAnimationFrame(frameId);
    container.removeEventListener("scroll", handleScroll);
  };
}, [diffs, onActivePathChange, rowVirtualizer]);
```

### 7.6 Keyboard Navigation
```tsx
// In GitPanel.tsx
useEffect(() => {
  const paths = [...stagedFiles, ...unstagedFiles].map(f => f.path);
  if (paths.length === 0) return;

  const handleKeyDown = (event: KeyboardEvent) => {
    const target = event.target as HTMLElement | null;
    if (target?.tagName === "INPUT" || target?.tagName === "TEXTAREA") return;

    const currentIndex = selectedPath ? paths.indexOf(selectedPath) : -1;
    let nextIndex: number | null = null;

    switch (event.key) {
      case "ArrowDown":
      case "j":
        nextIndex = currentIndex === -1 ? 0 : (currentIndex + 1) % paths.length;
        break;
      case "ArrowUp":
      case "k":
        nextIndex = currentIndex === -1 ? paths.length - 1 : (currentIndex - 1 + paths.length) % paths.length;
        break;
      case "Home":
        nextIndex = 0;
        break;
      case "End":
        nextIndex = paths.length - 1;
        break;
    }

    if (nextIndex !== null) {
      event.preventDefault();
      selectPath(paths[nextIndex]);
    }
  };

  window.addEventListener("keydown", handleKeyDown);
  return () => window.removeEventListener("keydown", handleKeyDown);
}, [stagedFiles, unstagedFiles, selectedPath, selectPath]);
```

### 7.7 useDiffStyle Hook
```tsx
// app/src/features/git/hooks/useDiffStyle.ts
import { useState, useCallback, useEffect } from "react";

type DiffStyle = "split" | "unified";
const STORAGE_KEY = "maestro:diffStyle";

export function useDiffStyle() {
  const [diffStyle, setDiffStyleState] = useState<DiffStyle>(() => {
    const stored = localStorage.getItem(STORAGE_KEY);
    return stored === "unified" ? "unified" : "split";
  });

  const setDiffStyle = useCallback((style: DiffStyle) => {
    setDiffStyleState(style);
    localStorage.setItem(STORAGE_KEY, style);
  }, []);

  return { diffStyle, setDiffStyle };
}
```

---

## 8. Styling

### 8.1 CSS Variables (existing in base.css)
The spec uses existing CSS variables. Key ones:
- `--surface-card`, `--surface-card-hover`, `--surface-card-active`
- `--status-success`, `--status-warning`, `--status-error` (and `-muted` variants)
- `--text-primary`, `--text-secondary`, `--text-tertiary`
- `--border-subtle`, `--radius-sm`, `--radius-lg`
- `--space-*` for spacing, `--text-*` for font sizes

### 8.2 File Row Styling
```css
/* Split filename into base and extension */
.git-file-name {
  display: flex;
  align-items: baseline;
  gap: 0;
}

.git-file-name-base {
  color: var(--text-primary);
}

.git-file-name-ext {
  color: var(--text-tertiary);
}

/* Directory path below filename */
.git-file-dir {
  font-size: var(--text-xs);
  color: var(--text-tertiary);
  overflow: hidden;
  text-overflow: ellipsis;
}
```

### 8.3 Status Badge Colors
```css
.git-status-a { /* Added */
  color: #47d488;
  background: rgba(71, 212, 136, 0.12);
  border: 1px solid rgba(71, 212, 136, 0.4);
}

.git-status-m { /* Modified */
  color: #f5c363;
  background: rgba(245, 195, 99, 0.12);
  border: 1px solid rgba(245, 195, 99, 0.4);
}

.git-status-d { /* Deleted */
  color: #ff6b6b;
  background: rgba(255, 107, 107, 0.12);
  border: 1px solid rgba(255, 107, 107, 0.45);
}

.git-status-r, .git-status-t { /* Renamed, Type change */
  color: var(--text-secondary);
  background: var(--surface-card);
  border: 1px solid var(--border-subtle);
}
```

---

## 9. Migration

### Files to Create
1. `app/src/features/git/components/GitDiffPanel.tsx`
2. `app/src/features/git/components/GitDiffViewer.tsx`
3. `app/src/features/git/hooks/useDiffStyle.ts`
4. `app/src/utils/diffsWorker.ts`

### Files to Modify
1. `app/package.json` - add `@pierre/diffs`, `@tanstack/react-virtual`
2. `app/src/features/git/components/GitPanel.tsx` - use new components
3. `app/src/styles/diff-viewer.css` - add new styles, update existing
4. `app/src/features/git/index.ts` - update exports
5. `app/src/main.tsx` - no changes needed (CSS already imported via diff-viewer.css)

### Files to Delete (after migration verified)
1. `app/src/features/git/components/GitStatusPanel.tsx`
2. `app/src/features/git/components/DiffViewer.tsx`

### Rollout Steps
1. Install dependencies: `bun add @pierre/diffs @tanstack/react-virtual`
2. Create new components (keep old ones temporarily)
3. Update GitPanel to use new components
4. Test thoroughly
5. Delete old components
6. Clean up unused CSS

---

## 10. Open Questions

None - all questions resolved:
- Diff style persists to localStorage ✓
- Scroll sync uses rAF throttling with 250ms ignore window ✓
- Keyboard navigation expanded with j/k and Home/End ✓
