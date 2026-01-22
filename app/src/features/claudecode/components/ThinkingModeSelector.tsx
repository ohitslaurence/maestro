import type { ThinkingMode } from "../../../types";

const THINKING_OPTIONS: { value: ThinkingMode; label: string }[] = [
  { value: "off", label: "Off" },
  { value: "low", label: "Low" },
  { value: "medium", label: "Medium" },
  { value: "high", label: "High" },
  { value: "max", label: "Max" },
];

type ThinkingModeSelectorProps = {
  selectedMode: ThinkingMode;
  onSelect: (mode: ThinkingMode) => void;
  disabled?: boolean;
};

/**
 * Dropdown for selecting thinking mode (composer-options spec §2).
 */
export function ThinkingModeSelector({
  selectedMode,
  onSelect,
  disabled = false,
}: ThinkingModeSelectorProps) {
  return (
    <div className="composer-option">
      <label htmlFor="thinking-selector" className="composer-option__label">
        Thinking
      </label>
      <select
        id="thinking-selector"
        className="composer-option__select"
        value={selectedMode}
        onChange={(e) => onSelect(e.target.value as ThinkingMode)}
        disabled={disabled}
      >
        {THINKING_OPTIONS.map((opt) => (
          <option key={opt.value} value={opt.value}>
            {opt.label}
          </option>
        ))}
      </select>
    </div>
  );
}
