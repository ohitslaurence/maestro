# Git Diff UI Implementation Plan

Reference: [git-diff-ui.md](../git-diff-ui.md)

## Phase 1: Dependencies & Setup

- [x] Run `bun add @pierre/diffs @tanstack/react-virtual` (See §2 Dependencies)
- [x] Create `app/src/utils/diffsWorker.ts` with worker factory (See §7.2)
- [x] Run `bun run typecheck` to verify dependencies installed correctly

## Phase 2: useDiffStyle Hook

- [x] Create `app/src/features/git/hooks/useDiffStyle.ts` (See §7.7)
- [x] Implement localStorage read/write with key `maestro:diffStyle` (See §3 Storage Schema)
- [x] Default to "split" if no stored value
- [x] Export from `app/src/features/git/index.ts`

## Phase 3: GitDiffPanel Component

- [x] Create `app/src/features/git/components/GitDiffPanel.tsx` (See §4 Interfaces)
- [x] Implement props interface: `branchName`, `stagedFiles`, `unstagedFiles`, `selectedPath`, `onSelectPath`, etc.
- [x] Render branch header with name (See §2 Components)
- [x] Render "Staged" section header with file count
- [x] Render "Changes" (unstaged) section header with file count
- [x] Implement FileRow subcomponent:
  - Status badge with color (A=green, M=yellow, D=red) (See §8.3)
  - Split filename: base in primary color, extension in tertiary (See §8.2)
  - Directory path below filename in tertiary color
  - Additions/deletions stats on right
- [x] Highlight selected file with `--surface-card-active` background
- [x] Handle empty states: "Working tree clean", "No commits yet" (See §5.6)
- [x] Handle loading state

## Phase 4: GitDiffViewer Component

- [x] Create `app/src/features/git/components/GitDiffViewer.tsx` (See §4 Interfaces)
- [x] Import and configure `WorkerPoolContextProvider` from `@pierre/diffs/react` (See §7.1)
- [x] Set up `useVirtualizer` with `estimateSize: 260`, `overscan: 6` (See §7.3)
- [x] Create DiffCard subcomponent (memoized):
  - Header with status badge and file path
  - `FileDiff` from `@pierre/diffs` with `parsePatchFiles` (See §7.1)
  - Handle empty diff: show "No changes" placeholder
- [x] Render virtualized list with absolute positioning (See §7.3)
- [x] Add split/unified toggle button in header
- [x] Implement scroll-to-file effect with `scrollRequestId` tracking (See §7.4)
- [x] Implement active path tracking on scroll with rAF throttling (See §7.5)
- [x] Handle loading state, error state (See §6)

## Phase 5: GitPanel Integration

- [x] Update `app/src/features/git/components/GitPanel.tsx`:
  - Import new `GitDiffPanel`, `GitDiffViewer`, `useDiffStyle`
  - Add `scrollRequestId` state, increment on file selection (See §5.2)
  - Add `activePath` state for scroll sync from viewer (See §5.3)
  - Wire `useDiffStyle` for split/unified toggle (See §5.4)
  - Pass all required props to new components
- [x] Update keyboard navigation to include j/k and Home/End (See §7.6)
- [x] Combine staged + unstaged files for keyboard navigation path list

## Phase 6: Styling

- [ ] Update `app/src/styles/diff-viewer.css`:
  - Add `.git-file-name`, `.git-file-name-base`, `.git-file-name-ext` (See §8.2)
  - Add `.git-file-dir` styles
  - Update status badge colors to match spec (See §8.3)
  - Add `.diff-viewer-item`, `.diff-viewer-header`, `.diff-viewer-output` for DiffCard
  - Add styles for split/unified toggle button
  - Ensure virtualized container has `position: relative` and `overflow: auto`

## Phase 7: Cleanup

- [ ] Delete `app/src/features/git/components/GitStatusPanel.tsx`
- [ ] Delete `app/src/features/git/components/DiffViewer.tsx`
- [ ] Update `app/src/features/git/index.ts` exports (remove old, add new)
- [ ] Remove any orphaned CSS selectors from `diff-viewer.css`

## Files to Create

- `app/src/utils/diffsWorker.ts`
- `app/src/features/git/hooks/useDiffStyle.ts`
- `app/src/features/git/components/GitDiffPanel.tsx`
- `app/src/features/git/components/GitDiffViewer.tsx`

## Files to Modify

- `app/package.json`
- `app/src/features/git/components/GitPanel.tsx`
- `app/src/features/git/index.ts`
- `app/src/styles/diff-viewer.css`

## Files to Delete

- `app/src/features/git/components/GitStatusPanel.tsx`
- `app/src/features/git/components/DiffViewer.tsx`

## Verification Checklist

- [ ] `bun run typecheck` passes
- [ ] Git tab displays branch name correctly
- [ ] Staged section shows with file count and files
- [ ] Unstaged section shows with file count and files
- [ ] File rows show status badge, split name/extension, directory, stats
- [ ] Clicking file highlights it and scrolls viewer to that file
- [ ] Scrolling viewer updates highlighted file in panel
- [ ] Split/unified toggle changes diff rendering style
- [ ] Toggle preference persists after page refresh
- [ ] Arrow keys (and j/k) navigate between files
- [ ] Home/End jump to first/last file
- [ ] "Working tree clean" shows when no changes
- [ ] Large changesets (50+ files) scroll smoothly
- [ ] Empty diff shows "No changes" placeholder
- [ ] Loading states display correctly

## Notes

- Phase 2 can be done in parallel with Phase 1
- Phase 3 and Phase 4 can be developed in parallel after Phase 1
- Phase 5 requires Phase 3 and 4 to be complete
- The `@pierre/diffs` worker factory may need adjustment based on Vite's worker handling - test early
- Reference CodexMonitor's `GitDiffViewer.tsx` (lines 145-243) for scroll sync implementation details
