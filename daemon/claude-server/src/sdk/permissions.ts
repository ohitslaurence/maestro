/**
 * canUseTool handler factory for Claude SDK.
 * Reference: specs/dynamic-tool-approvals.md §4 Interfaces
 */

import type { PermissionResult, PermissionUpdate } from "@anthropic-ai/claude-agent-sdk";
import { permissionManager } from "../permissions/manager";
import type { PermissionMetadata, PermissionRequest, PermissionSuggestion } from "../permissions/types";

/**
 * Permission mode determines which tools require approval.
 * - 'default': dangerous tools (Write, Edit, Bash, WebFetch, WebSearch) require approval
 * - 'acceptEdits': file operations auto-approve, other dangerous tools require approval
 * - 'bypassPermissions': all tools auto-approve
 */
export type PermissionMode = "default" | "acceptEdits" | "bypassPermissions";

/**
 * Options for the canUseTool callback.
 */
interface CanUseToolOptions {
  signal: AbortSignal;
  suggestions?: PermissionUpdate[];
}

/**
 * Type for the canUseTool callback function.
 */
type CanUseTool = (
  toolName: string,
  input: Record<string, unknown>,
  options: CanUseToolOptions
) => Promise<PermissionResult>;

/**
 * Dangerous tools that require permission in default mode.
 * Reference: §4 canUseTool Handler
 */
const DANGEROUS_TOOLS = ["Write", "Edit", "Bash", "WebFetch", "WebSearch"];

/**
 * File operation tools that auto-approve in acceptEdits mode.
 */
const FILE_TOOLS = ["Read", "Write", "Edit", "Glob", "Grep"];

/**
 * Extract patterns from tool input for permission matching.
 * Patterns are exact strings used for "always allow" matching.
 * Reference: §3 Data Model
 */
export function extractPatterns(toolName: string, input: Record<string, unknown>): string[] {
  const filePath = (input.file_path ?? input.path) as string | undefined;
  const patterns: string[] = [];

  switch (toolName) {
    case "Read":
    case "Write":
    case "Edit":
    case "Glob":
    case "Grep":
      if (typeof filePath === "string" && filePath.trim()) {
        patterns.push(filePath.trim());
      }
      break;

    case "Bash": {
      const command = input.command as string | undefined;
      if (typeof command === "string" && command.trim()) {
        patterns.push(command.trim());
      }
      break;
    }

    case "WebFetch": {
      const url = input.url as string | undefined;
      if (typeof url === "string" && url.trim()) {
        patterns.push(url.trim());
      }
      break;
    }

    case "WebSearch": {
      const query = input.query as string | undefined;
      if (typeof query === "string" && query.trim()) {
        patterns.push(query.trim());
      }
      break;
    }

    default:
      // Unknown tools: no patterns
      break;
  }

  return patterns;
}

/**
 * Extract metadata from tool input for display in permission UI.
 * Reference: §3 Data Model - PermissionMetadata
 */
export function extractMetadata(toolName: string, input: Record<string, unknown>): PermissionMetadata {
  const metadata: PermissionMetadata = {};

  switch (toolName) {
    case "Read":
      metadata.description = "Read file contents";
      metadata.filePath = input.file_path as string | undefined;
      break;

    case "Write":
      metadata.description = "Write file";
      metadata.filePath = input.file_path as string | undefined;
      break;

    case "Edit":
      metadata.description = "Edit file";
      metadata.filePath = input.file_path as string | undefined;
      // Compute diff if old_string and new_string are provided
      if (typeof input.old_string === "string" && typeof input.new_string === "string") {
        metadata.diff = buildSimpleDiff(
          input.old_string as string,
          input.new_string as string,
          input.file_path as string | undefined
        );
      }
      break;

    case "Glob":
      metadata.description = "Search for files";
      metadata.filePath = input.path as string | undefined;
      break;

    case "Grep":
      metadata.description = "Search file contents";
      metadata.filePath = input.path as string | undefined;
      break;

    case "Bash":
      metadata.description = "Run shell command";
      metadata.command = input.command as string | undefined;
      break;

    case "WebFetch":
      metadata.description = "Fetch URL";
      metadata.url = input.url as string | undefined;
      break;

    case "WebSearch":
      metadata.description = "Web search";
      metadata.query = input.query as string | undefined;
      break;

    default:
      metadata.description = `Execute ${toolName}`;
      break;
  }

  return metadata;
}

/**
 * Build a simple unified diff representation for Edit operations.
 */
function buildSimpleDiff(oldString: string, newString: string, filePath?: string): string {
  const header = filePath ? `--- ${filePath}\n+++ ${filePath}\n` : "";
  const oldLines = oldString.split("\n").map((line) => `- ${line}`);
  const newLines = newString.split("\n").map((line) => `+ ${line}`);
  return `${header}${oldLines.join("\n")}\n${newLines.join("\n")}`;
}

/**
 * Map SDK permission suggestions to our PermissionSuggestion format.
 */
function mapSuggestions(suggestions?: PermissionUpdate[]): PermissionSuggestion[] {
  if (!suggestions || !Array.isArray(suggestions)) {
    return [];
  }

  return suggestions
    .filter((s) => s && typeof s === "object")
    .map((s) => {
      // SDK suggestions may have various formats; normalize them
      const typedSuggestion = s as {
        type?: string;
        patterns?: string[];
        description?: string;
        directories?: string[];
        rules?: string[];
      };

      if (typedSuggestion.type === "addDirectories" && Array.isArray(typedSuggestion.directories)) {
        return {
          type: "addDirectories" as const,
          patterns: typedSuggestion.directories,
          description: typedSuggestion.description ?? "Allow access to directories",
        };
      }

      if (typedSuggestion.type === "addRules" && Array.isArray(typedSuggestion.rules)) {
        return {
          type: "addRules" as const,
          patterns: typedSuggestion.rules,
          description: typedSuggestion.description ?? "Allow matching patterns",
        };
      }

      // Fallback for other formats
      return {
        type: "addRules" as const,
        patterns: typedSuggestion.patterns ?? [],
        description: typedSuggestion.description ?? "Allow",
      };
    })
    .filter((s) => s.patterns.length > 0);
}

/**
 * Map tool name to permission type for categorization.
 */
function getPermissionType(toolName: string): string {
  switch (toolName) {
    case "Read":
    case "Glob":
    case "Grep":
      return "read";
    case "Write":
    case "Edit":
      return "write";
    case "Bash":
      return "bash";
    case "WebFetch":
      return "web_fetch";
    case "WebSearch":
      return "web_search";
    default:
      return toolName.toLowerCase();
  }
}

/**
 * Build a full permission request for the PermissionManager.
 * Reference: §4 canUseTool Handler - buildPermissionRequest
 */
export function buildPermissionRequest(
  toolName: string,
  input: Record<string, unknown>,
  suggestions?: PermissionUpdate[]
): Omit<PermissionRequest, "id" | "createdAt" | "sessionId" | "messageId"> {
  return {
    tool: toolName,
    permission: getPermissionType(toolName),
    input,
    patterns: extractPatterns(toolName, input),
    metadata: extractMetadata(toolName, input),
    suggestions: mapSuggestions(suggestions),
  };
}

/**
 * Create a canUseTool callback for the Claude SDK.
 * Reference: specs/dynamic-tool-approvals.md §4
 *
 * @param sessionId - Current session ID
 * @param permissionMode - Permission mode (default, acceptEdits, bypassPermissions)
 * @param getMessageId - Callback to get current assistant message ID
 */
export function createCanUseTool(
  sessionId: string,
  permissionMode: PermissionMode,
  getMessageId: () => string
): CanUseTool {
  return async (
    toolName: string,
    input: Record<string, unknown>,
    options: CanUseToolOptions
  ): Promise<PermissionResult> => {
    // Bypass mode: auto-approve everything
    if (permissionMode === "bypassPermissions") {
      return { behavior: "allow", updatedInput: input };
    }

    // Deny AskUserQuestion (interactive questions not supported)
    if (toolName === "AskUserQuestion") {
      return {
        behavior: "deny",
        message: "Interactive questions are not supported in this session",
      };
    }

    // AcceptEdits mode: auto-approve file operations
    if (permissionMode === "acceptEdits" && FILE_TOOLS.includes(toolName)) {
      return { behavior: "allow", updatedInput: input };
    }

    // Default mode: only dangerous tools require approval
    if (!DANGEROUS_TOOLS.includes(toolName)) {
      return { behavior: "allow", updatedInput: input };
    }

    // Build permission request and block until user replies
    const request = buildPermissionRequest(toolName, input, options.suggestions);

    return permissionManager.request(sessionId, getMessageId(), request, options.signal);
  };
}
