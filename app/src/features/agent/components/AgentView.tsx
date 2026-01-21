import { useState, useCallback } from "react";
import type { AgentHarness } from "../../../types";
import { AgentProviderSelector } from "./AgentProviderSelector";
import { ThreadView } from "../../opencode";
import { ClaudeThreadView } from "../../claudecode/components/ClaudeThreadView";

type AgentViewProps = {
  workspaceId: string | null;
};

/**
 * Wrapper component that allows switching between agent providers.
 *
 * Renders a provider selector and the appropriate ThreadView based
 * on the selected provider.
 */
export function AgentView({ workspaceId }: AgentViewProps) {
  const [provider, setProvider] = useState<AgentHarness>("open_code");

  const handleProviderChange = useCallback((newProvider: AgentHarness) => {
    setProvider(newProvider);
  }, []);

  return (
    <div className="agent-view">
      <div className="agent-view__header">
        <AgentProviderSelector
          provider={provider}
          onChange={handleProviderChange}
          disabled={!workspaceId}
        />
      </div>
      <div className="agent-view__content">
        {provider === "open_code" ? (
          <ThreadView workspaceId={workspaceId} />
        ) : (
          <ClaudeThreadView workspaceId={workspaceId} />
        )}
      </div>
    </div>
  );
}
