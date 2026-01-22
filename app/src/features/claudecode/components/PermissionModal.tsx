import { useCallback, useState } from "react";
import type { PermissionRequest, PermissionReply } from "../../../types";

type PermissionModalProps = {
  request: PermissionRequest | null;
  onReply: (reply: PermissionReply, message?: string) => void;
  onClose: () => void;
};

/**
 * Modal for tool permission requests.
 * Displays tool context and allows user to Allow Once, Deny, or Always Allow.
 *
 * Per spec §UI Components:
 * - Shows tool name and permission context
 * - Supports three reply types: allow, deny, always
 * - Handles concurrent permissions via queue (managed by parent hook)
 */
export function PermissionModal({
  request,
  onReply,
  onClose,
}: PermissionModalProps) {
  const [isSubmitting, setIsSubmitting] = useState(false);

  const handleReply = useCallback(
    async (reply: PermissionReply) => {
      setIsSubmitting(true);
      try {
        onReply(reply);
      } finally {
        setIsSubmitting(false);
      }
    },
    [onReply]
  );

  const handleDeny = useCallback(() => {
    handleReply("deny");
  }, [handleReply]);

  const handleAlways = useCallback(() => {
    handleReply("always");
  }, [handleReply]);

  const handleAllow = useCallback(() => {
    handleReply("allow");
  }, [handleReply]);

  if (!request) {
    return null;
  }

  // Get tool icon based on tool type
  const getToolIcon = (tool: string): string => {
    switch (tool) {
      case "Edit":
        return "✏️";
      case "Write":
        return "📝";
      case "Bash":
        return "⌨️";
      case "WebFetch":
        return "🌐";
      case "WebSearch":
        return "🔍";
      case "Read":
        return "📖";
      default:
        return "🔧";
    }
  };

  return (
    <div className="modal-overlay" onClick={onClose}>
      <div
        className="modal permission-modal"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="modal__header">
          <div className="permission-modal__title">
            <span className="permission-modal__icon">
              {getToolIcon(request.tool)}
            </span>
            <h2>Permission Required</h2>
          </div>
          <button
            type="button"
            className="modal__close"
            onClick={onClose}
            aria-label="Close"
          >
            ×
          </button>
        </div>

        <div className="modal__body">
          <div className="permission-modal__tool">
            <span className="permission-modal__tool-label">Tool:</span>
            <code className="permission-modal__tool-name">{request.tool}</code>
          </div>

          <PermissionContextDisplay request={request} />

          {request.suggestions.length > 0 && (
            <div className="permission-modal__suggestions">
              <span className="permission-modal__suggestions-label">
                Suggested patterns for "Always Allow":
              </span>
              <ul className="permission-modal__suggestions-list">
                {request.suggestions.map((s, i) => (
                  <li key={i}>
                    <code>{s.patterns.join(", ")}</code>
                    <span className="permission-modal__suggestion-desc">
                      {s.description}
                    </span>
                  </li>
                ))}
              </ul>
            </div>
          )}
        </div>

        <div className="modal__footer permission-modal__footer">
          <button
            type="button"
            className="btn btn--danger"
            onClick={handleDeny}
            disabled={isSubmitting}
          >
            Deny
          </button>
          <button
            type="button"
            className="btn btn--secondary"
            onClick={handleAlways}
            disabled={isSubmitting}
          >
            Always Allow
          </button>
          <button
            type="button"
            className="btn btn--primary"
            onClick={handleAllow}
            disabled={isSubmitting}
          >
            Allow Once
          </button>
        </div>
      </div>
    </div>
  );
}

/**
 * Display tool-specific context for permission requests.
 * Per spec §UI Components - PermissionContext component.
 *
 * Note: This is an inline implementation. The spec suggests a separate
 * PermissionContext.tsx file which is the next plan task.
 */
function PermissionContextDisplay({ request }: { request: PermissionRequest }) {
  const { tool, metadata, input } = request;

  switch (tool) {
    case "Edit":
      return (
        <div className="permission-context">
          <p>
            Edit file: <code>{metadata.filePath}</code>
          </p>
          {metadata.diff && (
            <pre className="permission-context__diff">{metadata.diff}</pre>
          )}
        </div>
      );

    case "Bash":
      return (
        <div className="permission-context">
          <p>Run command:</p>
          <pre className="permission-context__command">{metadata.command}</pre>
        </div>
      );

    case "Write":
      return (
        <div className="permission-context">
          <p>
            Write file: <code>{metadata.filePath}</code>
          </p>
        </div>
      );

    case "WebFetch":
      return (
        <div className="permission-context">
          <p>
            Fetch URL: <code>{metadata.url}</code>
          </p>
        </div>
      );

    case "WebSearch":
      return (
        <div className="permission-context">
          <p>
            Search: <code>{metadata.query}</code>
          </p>
        </div>
      );

    default:
      return (
        <div className="permission-context">
          {metadata.description && <p>{metadata.description}</p>}
          <pre className="permission-context__raw">
            {JSON.stringify(input, null, 2)}
          </pre>
        </div>
      );
  }
}
