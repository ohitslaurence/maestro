interface ResizeHandleProps {
  onMouseDown: (e: React.MouseEvent) => void;
  isResizing?: boolean;
}

export function ResizeHandle({ onMouseDown, isResizing }: ResizeHandleProps) {
  return (
    <div
      className={`resize-handle ${isResizing ? "resize-handle--active" : ""}`}
      onMouseDown={onMouseDown}
    />
  );
}
