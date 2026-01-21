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
        {/* Keep both views mounted to preserve state on provider switch */}
        <div style={{ display: provider === "open_code" ? "contents" : "none" }}>
          <ThreadView workspaceId={workspaceId} />
        </div>
        <div style={{ display: provider === "claude_code" ? "contents" : "none" }}>
          <ClaudeThreadView workspaceId={workspaceId} />
        </div>
      </div>
    </div>
  );
}
