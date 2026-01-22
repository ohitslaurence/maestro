import { useCallback, useState } from "react";
import type { SessionSettings, SessionSettingsUpdate, SystemPromptMode } from "../../../types";
import { DEFAULT_SESSION_SETTINGS, CLAUDE_TOOLS } from "../../../types";

type SessionSettingsModalProps = {
  isOpen: boolean;
  settings: SessionSettings;
  isUpdating: boolean;
  error: string | null;
  onSave: (patch: SessionSettingsUpdate) => Promise<boolean>;
  onClose: () => void;
};

/**
 * Modal for editing Claude SDK session settings.
 *
 * Reference: session-settings spec §Appendix
 */
export function SessionSettingsModal({
  isOpen,
  settings,
  isUpdating,
  error,
  onSave,
  onClose,
}: SessionSettingsModalProps) {
  // Local form state
  const [maxTurns, setMaxTurns] = useState(settings.maxTurns);
  const [systemPromptMode, setSystemPromptMode] = useState<SystemPromptMode>(
    settings.systemPrompt.mode
  );
  const [systemPromptContent, setSystemPromptContent] = useState(
    settings.systemPrompt.content ?? ""
  );
  const [disallowedTools, setDisallowedTools] = useState<string[]>(
    settings.disallowedTools ?? []
  );

  // Validation
  const maxTurnsError =
    maxTurns < 1 || maxTurns > 1000 ? "Must be between 1 and 1000" : null;
  const systemPromptError =
    (systemPromptMode === "append" || systemPromptMode === "custom") &&
    !systemPromptContent.trim()
      ? "Content required for this mode"
      : null;
  const hasValidationError = !!maxTurnsError || !!systemPromptError;

  const handleSave = useCallback(async () => {
    if (hasValidationError) return;

    const patch: SessionSettingsUpdate = {};

    // Only include changed fields
    if (maxTurns !== settings.maxTurns) {
      patch.maxTurns = maxTurns;
    }

    if (
      systemPromptMode !== settings.systemPrompt.mode ||
      systemPromptContent !== (settings.systemPrompt.content ?? "")
    ) {
      patch.systemPrompt = {
        mode: systemPromptMode,
        content:
          systemPromptMode === "default" ? undefined : systemPromptContent,
      };
    }

    const currentDisallowed = settings.disallowedTools ?? [];
    if (
      disallowedTools.length !== currentDisallowed.length ||
      !disallowedTools.every((t) => currentDisallowed.includes(t))
    ) {
      patch.disallowedTools =
        disallowedTools.length > 0 ? disallowedTools : null;
    }

    // Only call if there are changes
    if (Object.keys(patch).length > 0) {
      const success = await onSave(patch);
      if (success) {
        onClose();
      }
    } else {
      onClose();
    }
  }, [
    hasValidationError,
    maxTurns,
    systemPromptMode,
    systemPromptContent,
    disallowedTools,
    settings,
    onSave,
    onClose,
  ]);

  const handleReset = useCallback(() => {
    setMaxTurns(DEFAULT_SESSION_SETTINGS.maxTurns);
    setSystemPromptMode(DEFAULT_SESSION_SETTINGS.systemPrompt.mode);
    setSystemPromptContent("");
    setDisallowedTools([]);
  }, []);

  const handleToolToggle = useCallback((toolId: string) => {
    setDisallowedTools((prev) =>
      prev.includes(toolId)
        ? prev.filter((t) => t !== toolId)
        : [...prev, toolId]
    );
  }, []);

  if (!isOpen) {
    return null;
  }

  // Group tools by category
  const toolsByCategory = CLAUDE_TOOLS.reduce(
    (acc, tool) => {
      const category = tool.category;
      if (!acc[category]) {
        acc[category] = [];
      }
      acc[category].push(tool);
      return acc;
    },
    {} as Record<string, typeof CLAUDE_TOOLS[number][]>
  );

  return (
    <div className="modal-overlay" onClick={onClose}>
      <div
        className="modal session-settings-modal"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="modal__header">
          <h2>Session Settings</h2>
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
          {error && <div className="form-error">{error}</div>}

          {/* Execution Section */}
          <fieldset className="settings-section">
            <legend className="settings-section__legend">Execution</legend>
            <div className="form-group">
              <label htmlFor="maxTurns">Max Turns</label>
              <input
                id="maxTurns"
                type="number"
                min={1}
                max={1000}
                value={maxTurns}
                onChange={(e) => setMaxTurns(Number(e.target.value))}
                disabled={isUpdating}
              />
              {maxTurnsError && (
                <span className="form-group__error">{maxTurnsError}</span>
              )}
              <span className="form-group__hint">
                Maximum API turns per interaction (1-1000)
              </span>
            </div>
          </fieldset>

          {/* System Prompt Section */}
          <fieldset className="settings-section">
            <legend className="settings-section__legend">System Prompt</legend>
            <div className="form-group">
              <label>Mode</label>
              <div className="settings-radio-group">
                <label className="settings-radio">
                  <input
                    type="radio"
                    name="systemPromptMode"
                    value="default"
                    checked={systemPromptMode === "default"}
                    onChange={() => setSystemPromptMode("default")}
                    disabled={isUpdating}
                  />
                  <span>Default</span>
                </label>
                <label className="settings-radio">
                  <input
                    type="radio"
                    name="systemPromptMode"
                    value="append"
                    checked={systemPromptMode === "append"}
                    onChange={() => setSystemPromptMode("append")}
                    disabled={isUpdating}
                  />
                  <span>Append</span>
                </label>
                <label className="settings-radio">
                  <input
                    type="radio"
                    name="systemPromptMode"
                    value="custom"
                    checked={systemPromptMode === "custom"}
                    onChange={() => setSystemPromptMode("custom")}
                    disabled={isUpdating}
                  />
                  <span>Custom</span>
                </label>
              </div>
            </div>
            <div className="form-group">
              <label htmlFor="systemPromptContent">
                {systemPromptMode === "append"
                  ? "Additional instructions"
                  : systemPromptMode === "custom"
                    ? "Custom system prompt"
                    : "System prompt content"}
              </label>
              <textarea
                id="systemPromptContent"
                className="settings-textarea"
                placeholder={
                  systemPromptMode === "default"
                    ? "Using default system prompt"
                    : systemPromptMode === "append"
                      ? "Instructions to append to the default prompt..."
                      : "Your custom system prompt..."
                }
                value={systemPromptContent}
                onChange={(e) => setSystemPromptContent(e.target.value)}
                disabled={isUpdating || systemPromptMode === "default"}
                rows={4}
              />
              {systemPromptError && (
                <span className="form-group__error">{systemPromptError}</span>
              )}
            </div>
          </fieldset>

          {/* Tools Section */}
          <fieldset className="settings-section">
            <legend className="settings-section__legend">Tools</legend>
            <p className="settings-section__desc">
              Disable specific tools for this session:
            </p>
            <div className="settings-tools">
              {Object.entries(toolsByCategory).map(([category, tools]) => (
                <div key={category} className="settings-tools__category">
                  <span className="settings-tools__category-label">
                    {category}
                  </span>
                  <div className="settings-tools__list">
                    {tools.map((tool) => (
                      <label
                        key={tool.id}
                        className="settings-tool-checkbox"
                        title={tool.description}
                      >
                        <input
                          type="checkbox"
                          checked={disallowedTools.includes(tool.id)}
                          onChange={() => handleToolToggle(tool.id)}
                          disabled={isUpdating}
                        />
                        <span className="settings-tool-checkbox__name">
                          {tool.name}
                        </span>
                      </label>
                    ))}
                  </div>
                </div>
              ))}
            </div>
          </fieldset>
        </div>

        <div className="modal__footer session-settings-modal__footer">
          <button
            type="button"
            className="btn btn--ghost"
            onClick={handleReset}
            disabled={isUpdating}
          >
            Reset
          </button>
          <div className="session-settings-modal__actions">
            <button
              type="button"
              className="btn btn--secondary"
              onClick={onClose}
              disabled={isUpdating}
            >
              Cancel
            </button>
            <button
              type="button"
              className="btn btn--primary"
              onClick={handleSave}
              disabled={isUpdating || hasValidationError}
            >
              {isUpdating ? "Saving..." : "Save Settings"}
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
