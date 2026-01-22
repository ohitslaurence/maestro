import type { ModelInfo } from "../../../types";

type ModelSelectorProps = {
  models: ModelInfo[];
  selectedModel: string | undefined;
  onSelect: (modelId: string) => void;
  disabled?: boolean;
};

/**
 * Dropdown for selecting Claude model (composer-options spec §2).
 */
export function ModelSelector({
  models,
  selectedModel,
  onSelect,
  disabled = false,
}: ModelSelectorProps) {
  return (
    <div className="composer-option">
      <label htmlFor="model-selector" className="composer-option__label">
        Model
      </label>
      <select
        id="model-selector"
        className="composer-option__select"
        value={selectedModel ?? ""}
        onChange={(e) => onSelect(e.target.value)}
        disabled={disabled || models.length === 0}
      >
        {models.length === 0 ? (
          <option value="">Loading...</option>
        ) : (
          models.map((model) => (
            <option key={model.value} value={model.value}>
              {model.displayName}
            </option>
          ))
        )}
      </select>
    </div>
  );
}
