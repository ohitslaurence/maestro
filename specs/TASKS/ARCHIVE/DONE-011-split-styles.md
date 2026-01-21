# Task: Split styles.css into Per-Area Files

## Objective

Break the monolithic `styles.css` into focused per-area CSS files in `src/styles/`.

## Current State

`app/src/styles.css` (~150 lines) contains:
- CSS variables (`:root`)
- Reset styles (`*`, `body`)
- Layout (`.container`)
- Sidebar styles (`.sidebar`, `.sidebar-header`)
- Session list styles (`.session-list`)
- Main panel styles (`.main-panel`, `.welcome`, `.session-view`)
- Terminal placeholder styles
- Resize handle styles

## Output

```
app/src/styles/
  base.css           # Variables, reset, body, .container
  sidebar.css        # .sidebar, .sidebar-header, .session-list
  main-panel.css     # .main-panel, .welcome, .session-view
  terminal.css       # Terminal-related styles (placeholder for now)
  resize-handle.css  # .resize-handle styles
  index.css          # Imports all other files
```

## Implementation Details

### base.css

```css
:root {
  --bg-primary: #1a1a1a;
  --bg-secondary: #242424;
  --bg-tertiary: #2d2d2d;
  --text-primary: #ffffff;
  --text-secondary: #888888;
  --accent: #646cff;
  --border: #3d3d3d;
}

* {
  margin: 0;
  padding: 0;
  box-sizing: border-box;
}

body {
  font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, Oxygen,
    Ubuntu, Cantarell, "Open Sans", "Helvetica Neue", sans-serif;
  background-color: var(--bg-primary);
  color: var(--text-primary);
}

.container {
  display: flex;
  height: 100vh;
  width: 100vw;
}

.container--resizing {
  user-select: none;
  cursor: col-resize;
}
```

### index.css

```css
@import "./base.css";
@import "./sidebar.css";
@import "./main-panel.css";
@import "./terminal.css";
@import "./resize-handle.css";
```

### main.tsx update

```typescript
import "./styles/index.css";  // was: import "./styles.css"
```

## Implementation Steps

1. Create `app/src/styles/` directory
2. Create `base.css` with variables, reset, container
3. Create `sidebar.css` with sidebar and session-list styles
4. Create `main-panel.css` with main panel, welcome, session-view
5. Create `terminal.css` (placeholder styles for now)
6. Create `resize-handle.css` with resize handle styles
7. Create `index.css` that imports all files
8. Update `main.tsx` to import `./styles/index.css`
9. Delete old `styles.css`

## Acceptance Criteria

- [ ] All styles organized by area
- [ ] `index.css` imports all partials
- [ ] `main.tsx` imports `./styles/index.css`
- [ ] Old `styles.css` removed
- [ ] App renders identically to before
- [ ] `bun run typecheck` passes
