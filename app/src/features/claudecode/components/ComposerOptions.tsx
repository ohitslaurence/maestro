import type { ModelInfo, ThinkingMode } from "../../../types";
import { ModelSelector } from "./ModelSelector";
import { ThinkingModeSelector } from "./ThinkingModeSelector";

type ComposerOptionsProps = {
  models: ModelInfo[];
  selectedModel: string | undefined;
  onModelSelect: (modelId: string) => void;
  thinkingMode: ThinkingMode;
  onThinkingModeSelect: (mode: ThinkingMode) => void;
  disabled?: boolean;
};

/**
 * Container for model and thinking mode selectors (composer-options spec §2).
 * Positioned above the composer input.
 */
export function ComposerOptions({
  models,
  selectedModel,
  onModelSelect,
  thinkingMode,
  onThinkingModeSelect,
  disabled = false,
}: ComposerOptionsProps) {
  return (
    <div className="composer-options">
      <ModelSelector
        models={models}
        selectedModel={selectedModel}
        onSelect={onModelSelect}
        disabled={disabled}
      />
      <ThinkingModeSelector
        selectedMode={thinkingMode}
        onSelect={onThinkingModeSelect}
        disabled={disabled}
      />
    </div>
  );
}
