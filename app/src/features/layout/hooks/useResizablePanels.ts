import { useState, useCallback, useEffect } from "react";

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

export function useResizablePanels() {
  const [sidebarWidth, setSidebarWidth] = useState(() =>
    readStoredWidth(
      STORAGE_KEY_SIDEBAR,
      DEFAULT_SIDEBAR_WIDTH,
      MIN_SIDEBAR_WIDTH,
      MAX_SIDEBAR_WIDTH
    )
  );
  const [isResizing, setIsResizing] = useState(false);

  const onSidebarResizeStart = useCallback(
    (startEvent: React.MouseEvent) => {
      startEvent.preventDefault();
      const startX = startEvent.clientX;
      const startWidth = sidebarWidth;
      setIsResizing(true);

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
        setIsResizing(false);
      };

      document.addEventListener("mousemove", onMouseMove);
      document.addEventListener("mouseup", onMouseUp);
    },
    [sidebarWidth]
  );

  // Persist width changes (handles case where onMouseUp captures stale value)
  useEffect(() => {
    if (!isResizing) {
      localStorage.setItem(STORAGE_KEY_SIDEBAR, String(sidebarWidth));
    }
  }, [sidebarWidth, isResizing]);

  return {
    sidebarWidth,
    isResizing,
    onSidebarResizeStart,
  };
}
