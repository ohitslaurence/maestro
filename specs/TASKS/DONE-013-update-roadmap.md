# Task: Update ROADMAP.md

## Objective

Bring ROADMAP.md up to date with actual project state.

## Current Issues

1. **Commands section** says `npm run` instead of `bun run`
2. **Phase 2** tasks marked incomplete but most are done:
   - Feature-sliced architecture (005) - mostly done
   - Tauri IPC wrapper (002) - DONE
   - Event hub pattern (003) - DONE
   - Resizable panels (006) - DONE
3. **Phase 3** (Terminal) - largely complete:
   - PTY backend implemented
   - xterm.js frontend done
   - Bidirectional streaming works
4. **Phase 4** (Git) - backend done, frontend in progress:
   - Git commands implemented in Rust
   - Frontend components created (GitStatusPanel, DiffViewer)
5. **Research tasks** (007, 008) marked incomplete but are DONE
6. **Immediate Next Steps** section is stale

## Implementation Steps

1. Update commands from `npm` to `bun`
2. Mark completed Phase 2 items as done
3. Update Phase 3 status (terminal mostly complete)
4. Update Phase 4 status (backend done, frontend partial)
5. Mark research tasks as complete
6. Rewrite "Immediate Next Steps" to reflect current priorities
7. Add any new tasks discovered during implementation

## Acceptance Criteria

- [x] All `npm` references changed to `bun`
- [x] Phase 2/3/4 status reflects reality
- [x] Research tasks marked complete
- [x] Next steps section is actionable and current
