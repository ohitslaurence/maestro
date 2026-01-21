# Task: Implement Resizable Panels Pattern

## Objective

Port CodexMonitor's resizable panels pattern to Maestro. This provides draggable panel dividers with localStorage persistence.

## Reference

- `reference/codex-monitor/src/features/layout/hooks/useResizablePanels.ts` - Panel logic
- `reference/codex-monitor/src/features/layout/components/DesktopLayout.tsx` - Usage

## Output

Create `app/src/features/layout/hooks/useResizablePanels.ts` with:
- Sidebar width management
- Right panel width management
- Drag handlers
- localStorage persistence

## Implementation Details

### Pattern to Copy

```typescript
const STORAGE_KEY_SIDEBAR = "maestro.sidebarWidth";
const MIN_SIDEBAR_WIDTH = 220;
const MAX_SIDEBAR_WIDTH = 420;
const DEFAULT_SIDEBAR_WIDTH = 280;

function readStoredWidth(
  key: string,
  defaultValue: number,
  min: number,
  max: number
): number {
  const stored = localStorage.getItem(key);
  if (!stored) return defaultValue;
  const value = parseInt(stored, 10);
  if (isNaN(value)) return defaultValue;
  return Math.max(min, Math.min(max, value));
}

export function useResizablePanels(uiScale = 1) {
  const [sidebarWidth, setSidebarWidth] = useState(() =>
    readStoredWidth(
      STORAGE_KEY_SIDEBAR,
      DEFAULT_SIDEBAR_WIDTH,
      MIN_SIDEBAR_WIDTH,
      MAX_SIDEBAR_WIDTH
    )
  );

  const onSidebarResizeStart = useCallback((startEvent: React.MouseEvent) => {
    const startX = startEvent.clientX;
    const startWidth = sidebarWidth;

    const onMouseMove = (e: MouseEvent) => {
      const delta = e.clientX - startX;
      const newWidth = Math.max(
        MIN_SIDEBAR_WIDTH,
        Math.min(MAX_SIDEBAR_WIDTH, startWidth + delta)
      );
      setSidebarWidth(newWidth);
    };

    const onMouseUp = () => {
      document.removeEventListener("mousemove", onMouseMove);
      document.removeEventListener("mouseup", onMouseUp);
      localStorage.setItem(STORAGE_KEY_SIDEBAR, String(sidebarWidth));
    };

    document.addEventListener("mousemove", onMouseMove);
    document.addEventListener("mouseup", onMouseUp);
  }, [sidebarWidth]);

  return {
    sidebarWidth: sidebarWidth * uiScale,
    onSidebarResizeStart,
  };
}
```

### Resize Handle Component

```typescript
export function ResizeHandle({ onMouseDown }: { onMouseDown: (e: React.MouseEvent) => void }) {
  return (
    <div
      className="resize-handle"
      onMouseDown={onMouseDown}
      style={{ cursor: "col-resize" }}
    />
  );
}
```

### CSS

```css
.resize-handle {
  width: 4px;
  background: transparent;
  transition: background 0.15s;
}

.resize-handle:hover {
  background: var(--border-color);
}
```

### Panels to Support

1. **Sidebar** - left panel (session list, navigation)
2. **Right panel** - optional (git status, diff viewer)
3. **Bottom panel** - optional (terminal dock)

Each panel needs:
- Storage key
- Min/max/default values
- Resize handler

## Constraints

- Persist to localStorage on drag end (not during)
- Respect min/max constraints
- Support UI scale multiplier
- Clean up event listeners on unmount

## Future Considerations

- Double-click to reset to default
- Keyboard accessibility (arrow keys to resize)
- Collapse/expand toggle
