import { useCallback, useRef, useState, type KeyboardEvent, type ChangeEvent } from "react";

type ThreadComposerProps = {
  onSend: (message: string) => void;
  onStop: () => void;
  canStop: boolean;
  disabled: boolean;
  isProcessing: boolean;
};

export function ThreadComposer({
  onSend,
  onStop,
  canStop,
  disabled,
  isProcessing,
}: ThreadComposerProps) {
  const [value, setValue] = useState("");
  const textareaRef = useRef<HTMLTextAreaElement>(null);

  const handleChange = useCallback((e: ChangeEvent<HTMLTextAreaElement>) => {
    setValue(e.target.value);
    // Auto-resize
    const textarea = e.target;
    textarea.style.height = "auto";
    textarea.style.height = `${Math.min(textarea.scrollHeight, 200)}px`;
  }, []);

  const handleSend = useCallback(() => {
    const trimmed = value.trim();
    if (!trimmed || disabled) return;
    onSend(trimmed);
    setValue("");
    if (textareaRef.current) {
      textareaRef.current.style.height = "auto";
    }
  }, [value, disabled, onSend]);

  const handleKeyDown = useCallback(
    (e: KeyboardEvent<HTMLTextAreaElement>) => {
      if (e.key === "Enter" && !e.shiftKey) {
        e.preventDefault();
        handleSend();
      }
    },
    [handleSend]
  );

  const showStop = isProcessing && canStop;
  const showSend = !isProcessing;

  return (
    <div className="oc-composer">
      <textarea
        ref={textareaRef}
        className="oc-composer__input"
        placeholder={disabled ? "Select a session..." : "Send a message..."}
        value={value}
        onChange={handleChange}
        onKeyDown={handleKeyDown}
        disabled={disabled || isProcessing}
        rows={1}
      />
      <div className="oc-composer__actions">
        {showStop && (
          <button
            type="button"
            className="oc-composer__btn oc-composer__btn--stop"
            onClick={onStop}
            title="Stop generation"
          >
            Stop
          </button>
        )}
        {showSend && (
          <button
            type="button"
            className="oc-composer__btn oc-composer__btn--send"
            onClick={handleSend}
            disabled={disabled || !value.trim()}
            title="Send message"
          >
            Send
          </button>
        )}
      </div>
    </div>
  );
}
