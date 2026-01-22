import type { AgentHarness } from "../../../types";

type AgentProviderSelectorProps = {
  provider: AgentHarness;
  onChange: (provider: AgentHarness) => void;
  disabled?: boolean;
};

const PROVIDERS: { id: AgentHarness; label: string }[] = [
  { id: "claude_code", label: "Claude" },
  { id: "open_code", label: "OpenCode" },
];

export function AgentProviderSelector({
  provider,
  onChange,
  disabled = false,
}: AgentProviderSelectorProps) {
  return (
    <div className="agent-provider-selector">
      {PROVIDERS.map((p) => (
        <button
          key={p.id}
          type="button"
          className={`agent-provider-selector__btn${provider === p.id ? " agent-provider-selector__btn--active" : ""}`}
          onClick={() => onChange(p.id)}
          disabled={disabled}
        >
          {p.label}
        </button>
      ))}
    </div>
  );
}
