/**
 * SDK Agent wrapper (§4, Appendix A)
 *
 * Wraps @anthropic-ai/claude-code query() to match CLI behavior.
 */

import { query } from '@anthropic-ai/claude-code';
import type { Session } from '../types';
import { buildHooksConfig, createCanUseTool } from './permissions';

// --- Types ---

export interface QueryOptions {
  prompt: string;
  session: Session;
  resumeId?: string;
  abortController?: AbortController;
}

export interface QueryResult {
  sessionId: string;
  resumeId?: string;
}

// --- SDK Configuration (Appendix A) ---

/**
 * Build SDK options to match CLI behavior (§10).
 *
 * Note: The SDK's Options type doesn't include systemPrompt, tools, or
 * settingSources - these are CLI features not exposed via SDK API.
 * The SDK uses Claude Code defaults automatically.
 *
 * Includes canUseTool callback and hooks for permission handling (§5 Permission Flow).
 */
function buildSdkOptions(
  session: Session,
  resumeId?: string,
  abortController?: AbortController
) {
  return {
    cwd: session.directory,
    permissionMode: mapPermissionMode(session.permission),
    maxTurns: 100,
    model: session.modelId,
    abortController,
    resume: resumeId,
    // Permission handling (§5 Permission Flow, Phase 8)
    canUseTool: createCanUseTool(session.id, session.permission),
    hooks: buildHooksConfig(session.id),
  };
}

type SDKPermissionMode = 'bypassPermissions' | 'acceptEdits' | 'default';

/**
 * Map our permission mode to SDK permission mode.
 */
function mapPermissionMode(mode: Session['permission']): SDKPermissionMode {
  switch (mode) {
    case 'bypassPermissions':
      return 'bypassPermissions';
    case 'acceptEdits':
      return 'acceptEdits';
    case 'default':
    default:
      return 'default';
  }
}

// --- Query Execution ---

/**
 * Execute a query against the Claude SDK.
 *
 * Returns an async generator that yields SDK messages.
 * The caller is responsible for mapping these to OpenCode events.
 */
export async function* executeQuery(
  options: QueryOptions
): AsyncGenerator<unknown, QueryResult, undefined> {
  const { prompt, session, resumeId, abortController } = options;

  const sdkOptions = buildSdkOptions(session, resumeId, abortController);

  // Build query config
  const stream = query({ prompt, options: sdkOptions });

  let resultSessionId = session.id;
  let resultResumeId: string | undefined;

  for await (const message of stream) {
    yield message;

    // Extract session/resume info from result message
    if (isResultMessage(message)) {
      resultSessionId = message.session_id || session.id;
      // The SDK result contains session_id which can be used for resume
      resultResumeId = message.session_id;
    }
  }

  return {
    sessionId: resultSessionId,
    resumeId: resultResumeId,
  };
}

// --- Type guards ---

interface SDKResultMessage {
  type: 'result';
  session_id?: string;
  subtype?: string;
  is_error?: boolean;
  total_cost_usd?: number;
  usage?: {
    input_tokens?: number;
    output_tokens?: number;
  };
  modelUsage?: Record<string, { inputTokens?: number; outputTokens?: number }>;
}

function isResultMessage(message: unknown): message is SDKResultMessage {
  return (
    typeof message === 'object' &&
    message !== null &&
    'type' in message &&
    (message as { type: unknown }).type === 'result'
  );
}

export function isAssistantMessage(
  message: unknown
): message is {
  type: 'assistant';
  uuid: string;
  session_id: string;
  message: { role: 'assistant'; content: unknown[] };
} {
  return (
    typeof message === 'object' &&
    message !== null &&
    'type' in message &&
    (message as { type: unknown }).type === 'assistant'
  );
}

export function isUserMessage(
  message: unknown
): message is {
  type: 'user';
  uuid: string;
  session_id: string;
  message: { role: 'user'; content: unknown[] };
} {
  return (
    typeof message === 'object' &&
    message !== null &&
    'type' in message &&
    (message as { type: unknown }).type === 'user'
  );
}

export { isResultMessage };
