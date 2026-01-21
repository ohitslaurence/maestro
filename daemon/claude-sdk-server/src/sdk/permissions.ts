/**
 * Permission handling (§5 Permission Flow)
 *
 * Auto-approve logic based on permissionMode setting for MVP.
 * Emits permission.asked and permission.replied events for UI awareness.
 */

import { randomUUID } from 'crypto';
import { sseEmitter } from '../events/emitter';
import { logger } from '../logger';
import type { PermissionMode } from '../types';
import type { CanUseTool, PermissionResult, HookCallbackMatcher, HookInput } from '@anthropic-ai/claude-code';

// --- Permission Decision Logic ---

/**
 * Determine permission result based on permissionMode (§5 Permission Flow).
 *
 * MVP behavior:
 * - 'bypassPermissions': approve all
 * - 'acceptEdits': approve all (reads/writes and others)
 * - 'default': approve all (auto-approve for MVP)
 */
function shouldApprove(
  permissionMode: PermissionMode,
  _toolName: string,
  _input: Record<string, unknown>
): boolean {
  // For MVP, auto-approve all permissions regardless of mode
  // Future: implement fine-grained permission checks
  switch (permissionMode) {
    case 'bypassPermissions':
      return true;
    case 'acceptEdits':
      return true;
    case 'default':
    default:
      return true;
  }
}

// --- CanUseTool Callback ---

/**
 * Create a canUseTool callback that auto-approves based on permissionMode.
 * Emits permission.asked and permission.replied SSE events (§4).
 */
export function createCanUseTool(sessionId: string, permissionMode: PermissionMode): CanUseTool {
  return async (
    toolName: string,
    input: Record<string, unknown>,
    options: { signal: AbortSignal; suggestions?: unknown[] }
  ): Promise<PermissionResult> => {
    const requestId = randomUUID();

    // Check for abort
    if (options.signal.aborted) {
      return {
        behavior: 'deny',
        message: 'Operation aborted',
        interrupt: true,
      };
    }

    // Emit permission.asked event (§4)
    sseEmitter.emitPermissionAsked({
      id: requestId,
      sessionId,
      permission: 'tool_use',
      tool: toolName,
    });

    // Determine approval based on permissionMode
    const approved = shouldApprove(permissionMode, toolName, input);

    logger.debug('permission decision', {
      sessionId,
      requestId,
      toolName,
      permissionMode,
      approved,
    });

    // Emit permission.replied event (§4)
    sseEmitter.emitPermissionReplied({
      sessionId,
      requestId,
      reply: approved ? 'allow' : 'deny',
    });

    if (approved) {
      return {
        behavior: 'allow',
        updatedInput: input,
      };
    }

    return {
      behavior: 'deny',
      message: `Permission denied for tool: ${toolName}`,
    };
  };
}

// --- Hook Callbacks ---

/**
 * Create PreToolUse hook callbacks for logging/tracking.
 * These run before the canUseTool permission check.
 */
export function createPreToolUseHooks(sessionId: string): HookCallbackMatcher[] {
  return [
    {
      hooks: [
        async (input: HookInput, toolUseId: string | undefined, _options) => {
          if (input.hook_event_name !== 'PreToolUse') {
            return {};
          }

          logger.debug('pre-tool-use hook', {
            sessionId,
            toolName: input.tool_name,
            toolUseId,
          });

          // Allow tool to proceed (no blocking)
          return { continue: true };
        },
      ],
    },
  ];
}

/**
 * Create PostToolUse hook callbacks for logging/tracking.
 * These run after tool execution completes.
 */
export function createPostToolUseHooks(sessionId: string): HookCallbackMatcher[] {
  return [
    {
      hooks: [
        async (input: HookInput, toolUseId: string | undefined, _options) => {
          if (input.hook_event_name !== 'PostToolUse') {
            return {};
          }

          logger.debug('post-tool-use hook', {
            sessionId,
            toolName: input.tool_name,
            toolUseId,
          });

          return { continue: true };
        },
      ],
    },
  ];
}

/**
 * Build complete hooks configuration for SDK options.
 */
export function buildHooksConfig(sessionId: string): {
  PreToolUse: HookCallbackMatcher[];
  PostToolUse: HookCallbackMatcher[];
} {
  return {
    PreToolUse: createPreToolUseHooks(sessionId),
    PostToolUse: createPostToolUseHooks(sessionId),
  };
}
