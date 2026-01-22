import { memo, useEffect, useMemo, useRef } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import { FileDiff, WorkerPoolContextProvider } from "@pierre/diffs/react";
import type { FileDiffMetadata } from "@pierre/diffs";
import { parsePatchFiles } from "@pierre/diffs";
import { workerFactory } from "../../../utils/diffsWorker";
import type { GitFileDiff } from "../../../types";

export type DiffStyle = "split" | "unified";

type GitDiffViewerProps = {
  diffs: GitFileDiff[];
  selectedPath: string | null;
  scrollRequestId: number;
  isLoading: boolean;
  error: string | null;
  diffStyle: DiffStyle;
  onDiffStyleChange: (style: DiffStyle) => void;
  onActivePathChange: (path: string) => void;
};

/** CSS to allow virtualized scroll to work with @pierre/diffs */
const DIFF_SCROLL_CSS = `
[data-column-number],
[data-buffer],
[data-separator-wrapper],
[data-annotation-content] {
  position: static !important;
}

[data-buffer] {
  background-image: none !important;
}
`;

/** Normalize git patch file paths (strip a/ b/ prefixes) */
function normalizePatchName(name: string): string {
  if (!name) return name;
  return name.replace(/^(?:a|b)\//, "");
}

type DiffCardProps = {
  entry: GitFileDiff;
  diffStyle: DiffStyle;
  isSelected: boolean;
};

const DiffCard = memo(function DiffCard({
  entry,
  diffStyle,
  isSelected,
}: DiffCardProps) {
  const diffOptions = useMemo(
    () => ({
      diffStyle,
      hunkSeparators: "line-info" as const,
      overflow: "wrap" as const,
      unsafeCSS: DIFF_SCROLL_CSS,
      disableFileHeader: true,
    }),
    [diffStyle],
  );

  const fileDiff = useMemo((): FileDiffMetadata | null => {
    if (!entry.diff.trim()) {
      return null;
    }
    const patch = parsePatchFiles(entry.diff);
    const parsed = patch[0]?.files[0];
    if (!parsed) {
      return null;
    }
    const normalizedName = normalizePatchName(parsed.name || entry.path);
    const normalizedPrevName = parsed.prevName
      ? normalizePatchName(parsed.prevName)
      : undefined;
    return {
      ...parsed,
      name: normalizedName,
      prevName: normalizedPrevName,
    };
  }, [entry.diff, entry.path]);

  const statusLower = entry.path.endsWith("/") ? "d" : "m"; // fallback

  return (
    <div
      data-diff-path={entry.path}
      className={`diff-viewer-item${isSelected ? " active" : ""}`}
    >
      <div className="diff-viewer-header">
        <span className="diff-viewer-status" data-status={statusLower}>
          {statusLower.toUpperCase()}
        </span>
        <span className="diff-viewer-path">{entry.path}</span>
      </div>
      {entry.diff.trim() ? (
        fileDiff ? (
          <div className="diff-viewer-output diff-viewer-output-flat">
            <FileDiff
              fileDiff={fileDiff}
              options={diffOptions}
              style={{ width: "100%", maxWidth: "100%", minWidth: 0 }}
            />
          </div>
        ) : (
          <div className="diff-viewer-output diff-viewer-raw">
            <pre>{entry.diff}</pre>
          </div>
        )
      ) : (
        <div className="diff-viewer-placeholder">No changes</div>
      )}
    </div>
  );
});

export function GitDiffViewer({
  diffs,
  selectedPath,
  scrollRequestId,
  isLoading,
  error,
  diffStyle,
  onDiffStyleChange,
  onActivePathChange,
}: GitDiffViewerProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const activePathRef = useRef<string | null>(null);
  const ignoreActivePathUntilRef = useRef<number>(0);
  const lastScrollRequestIdRef = useRef<number | null>(null);

  const poolOptions = useMemo(() => ({ workerFactory }), []);
  const highlighterOptions = useMemo(
    () => ({ theme: { dark: "pierre-dark", light: "pierre-light" } }),
    [],
  );

  const indexByPath = useMemo(() => {
    const map = new Map<string, number>();
    diffs.forEach((entry, index) => {
      map.set(entry.path, index);
    });
    return map;
  }, [diffs]);

  const rowVirtualizer = useVirtualizer({
    count: diffs.length,
    getScrollElement: () => containerRef.current,
    estimateSize: () => 260,
    overscan: 6,
  });

  const virtualItems = rowVirtualizer.getVirtualItems();

  // Sticky header entry (§7.1)
  const stickyEntry = useMemo(() => {
    if (!diffs.length) return null;
    if (selectedPath) {
      const index = indexByPath.get(selectedPath);
      if (index !== undefined) return diffs[index];
    }
    return diffs[0];
  }, [diffs, selectedPath, indexByPath]);

  // Scroll-to-file effect (§7.4)
  useEffect(() => {
    if (!selectedPath || !scrollRequestId) return;
    if (lastScrollRequestIdRef.current === scrollRequestId) return;

    const index = indexByPath.get(selectedPath);
    if (index === undefined) return;

    ignoreActivePathUntilRef.current = Date.now() + 250;
    rowVirtualizer.scrollToIndex(index, { align: "start" });
    lastScrollRequestIdRef.current = scrollRequestId;
  }, [selectedPath, scrollRequestId, indexByPath, rowVirtualizer]);

  // Keep activePathRef in sync
  useEffect(() => {
    activePathRef.current = selectedPath;
  }, [selectedPath]);

  // Active path tracking on scroll (§7.5)
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
      const canScroll = container.scrollHeight > container.clientHeight;
      const isAtBottom =
        canScroll &&
        scrollTop + container.clientHeight >= container.scrollHeight - 4;

      let nextPath: string | undefined;
      if (isAtBottom) {
        nextPath = diffs[diffs.length - 1]?.path;
      } else {
        const targetOffset = scrollTop + 8;
        let activeItem = items[0];
        for (const item of items) {
          if (item.start <= targetOffset) {
            activeItem = item;
          } else {
            break;
          }
        }
        nextPath = diffs[activeItem.index]?.path;
      }

      if (!nextPath || nextPath === activePathRef.current) return;

      activePathRef.current = nextPath;
      onActivePathChange(nextPath);
    };

    const handleScroll = () => {
      if (frameId !== null) return;
      frameId = requestAnimationFrame(updateActivePath);
    };

    handleScroll();
    container.addEventListener("scroll", handleScroll, { passive: true });
    return () => {
      if (frameId !== null) cancelAnimationFrame(frameId);
      container.removeEventListener("scroll", handleScroll);
    };
  }, [diffs, onActivePathChange, rowVirtualizer]);

  return (
    <WorkerPoolContextProvider
      poolOptions={poolOptions}
      highlighterOptions={highlighterOptions}
    >
      <div className="diff-viewer" ref={containerRef}>
        {/* Diff style toggle (§5.4) */}
        <div className="diff-viewer-toolbar">
          <div className="diff-style-toggle">
            <button
              type="button"
              className={`diff-style-btn${diffStyle === "split" ? " active" : ""}`}
              onClick={() => onDiffStyleChange("split")}
            >
              Split
            </button>
            <button
              type="button"
              className={`diff-style-btn${diffStyle === "unified" ? " active" : ""}`}
              onClick={() => onDiffStyleChange("unified")}
            >
              Unified
            </button>
          </div>
        </div>

        {/* Sticky file header */}
        {!error && stickyEntry && (
          <div className="diff-viewer-sticky">
            <div className="diff-viewer-header diff-viewer-header-sticky">
              <span className="diff-viewer-path">{stickyEntry.path}</span>
            </div>
          </div>
        )}

        {/* Error state (§6) */}
        {error && <div className="diff-viewer-empty">{error}</div>}

        {/* Loading overlay */}
        {!error && isLoading && diffs.length > 0 && (
          <div className="diff-viewer-loading diff-viewer-loading-overlay">
            Refreshing diff...
          </div>
        )}

        {/* Empty state (§5.6) */}
        {!error && !isLoading && !diffs.length && (
          <div className="diff-viewer-empty">No changes detected.</div>
        )}

        {/* Virtualized diff list (§7.3) */}
        {!error && diffs.length > 0 && (
          <div
            className="diff-viewer-list"
            style={{
              height: rowVirtualizer.getTotalSize(),
              position: "relative",
            }}
          >
            {virtualItems.map((virtualRow) => {
              const entry = diffs[virtualRow.index];
              return (
                <div
                  key={entry.path}
                  className="diff-viewer-row"
                  data-index={virtualRow.index}
                  ref={rowVirtualizer.measureElement}
                  style={{
                    position: "absolute",
                    top: 0,
                    left: 0,
                    width: "100%",
                    transform: `translate3d(0, ${virtualRow.start}px, 0)`,
                  }}
                >
                  <DiffCard
                    entry={entry}
                    diffStyle={diffStyle}
                    isSelected={entry.path === selectedPath}
                  />
                </div>
              );
            })}
          </div>
        )}
      </div>
    </WorkerPoolContextProvider>
  );
}
