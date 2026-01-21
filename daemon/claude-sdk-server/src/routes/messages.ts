/**
 * Message routes (§4, §5)
 *
 * HTTP endpoint for sending messages to sessions.
 */

import { randomUUID } from 'crypto';
import { Hono } from 'hono';
import { sseEmitter } from '../events/emitter';
import { logger } from '../logger';
import { executeQuery, isAssistantMessage, isResultMessage, type QueryResult } from '../sdk/agent';
import { getSessionStore } from '../storage/sessions';
import {
  ErrorCode,
  type ApiError,
  type MessageInfo,
  type Part,
  type SendMessageRequest,
  type SendMessageResponse,
  type TextPart,
} from '../types';

export const messagesRouter = new Hono();

// Track active executions per session to prevent concurrent messages (§5 Edge Cases)
const activeExecutions = new Map<string, AbortController>();

/**
 * POST /session/:id/message
 *
 * Send a user message and stream the assistant response.
 * Emits SSE events during execution (§5 Main Flow).
 */
messagesRouter.post('/:id/message', async (c) => {
  const store = getSessionStore();
  const sessionId = c.req.param('id');

  // Get session
  const session = store.get(sessionId);
  if (!session) {
    const error: ApiError = {
      code: ErrorCode.SESSION_NOT_FOUND,
      message: 'Session not found',
    };
    return c.json(error, 404);
  }

  // Check for concurrent execution (§5 Edge Cases: SESSION_BUSY)
  if (activeExecutions.has(sessionId)) {
    const error: ApiError = {
      code: ErrorCode.SESSION_BUSY,
      message: 'Session is busy processing another message',
    };
    return c.json(error, 409);
  }

  // Parse request body
  let body: SendMessageRequest;
  try {
    body = await c.req.json<SendMessageRequest>();
  } catch {
    const error: ApiError = {
      code: ErrorCode.INVALID_REQUEST,
      message: 'Invalid JSON body',
    };
    return c.json(error, 400);
  }

  // Validate request
  if (!body.parts || !Array.isArray(body.parts) || body.parts.length === 0) {
    const error: ApiError = {
      code: ErrorCode.INVALID_REQUEST,
      message: 'Missing or empty "parts" array',
    };
    return c.json(error, 400);
  }

  // Extract text from parts (§4: only text type for now)
  const textParts = body.parts.filter((p) => p.type === 'text');
  if (textParts.length === 0) {
    const error: ApiError = {
      code: ErrorCode.INVALID_REQUEST,
      message: 'No text parts in message',
    };
    return c.json(error, 400);
  }

  const prompt = textParts.map((p) => p.text).join('\n');
  const now = Date.now();

  // Create user message (§5 Main Flow step 2)
  const userMessageId = randomUUID();
  const userMessage: MessageInfo = {
    id: userMessageId,
    sessionId,
    role: 'user',
    createdAt: now,
    completedAt: now,
  };

  await store.addMessage(sessionId, userMessage, [
    {
      id: randomUUID(),
      messageId: userMessageId,
      type: 'text',
      text: prompt,
    } as TextPart,
  ]);
  sseEmitter.emitMessageUpdated(userMessage);

  // Create assistant message (§5 Main Flow step 3)
  const assistantMessageId = randomUUID();
  const assistantMessage: MessageInfo = {
    id: assistantMessageId,
    sessionId,
    role: 'assistant',
    createdAt: now,
    modelId: session.modelId,
    providerId: 'anthropic',
  };

  await store.addMessage(sessionId, assistantMessage);
  sseEmitter.emitMessageUpdated(assistantMessage);

  // Emit session.status { type: 'busy' } (§5 Main Flow step 4)
  await store.update(sessionId, { status: 'busy' });
  sseEmitter.emitSessionStatus(sessionId, { type: 'busy' });

  // Set up abort controller and track execution
  const abortController = new AbortController();
  activeExecutions.set(sessionId, abortController);

  const parts: Part[] = [];
  let totalCost = 0;
  let totalInputTokens = 0;
  let totalOutputTokens = 0;

  try {
    // Execute SDK query (§5 Main Flow steps 5-7)
    const queryGenerator = executeQuery({
      prompt,
      session,
      resumeId: session.resumeId,
      abortController,
    });

    let result: QueryResult | undefined;
    let done = false;

    while (!done) {
      const iterResult = await queryGenerator.next();
      done = iterResult.done ?? false;

      if (done && iterResult.value) {
        result = iterResult.value as QueryResult;
        break;
      }

      const message = iterResult.value;

      // Process SDK messages and emit SSE events
      // Note: Detailed event mapping is Phase 5; for now emit basic text
      if (isAssistantMessage(message)) {
        for (const block of message.message.content) {
          if (isTextBlock(block)) {
            const textPart: TextPart = {
              id: randomUUID(),
              messageId: assistantMessageId,
              type: 'text',
              text: block.text,
            };
            parts.push(textPart);
            await store.upsertPart(sessionId, assistantMessageId, textPart);
            sseEmitter.emitMessagePartUpdated(textPart, block.text);
          }
        }
      }

      if (isResultMessage(message)) {
        totalCost = message.total_cost_usd ?? 0;
        if (message.usage) {
          totalInputTokens = message.usage.input_tokens ?? 0;
          totalOutputTokens = message.usage.output_tokens ?? 0;
        }
      }
    }

    // Update assistant message with completion info (§5 Main Flow step 7a-b)
    const completedAt = Date.now();
    const updatedAssistantMessage = await store.updateMessage(sessionId, assistantMessageId, {
      completedAt,
      cost: totalCost,
      tokens: { input: totalInputTokens, output: totalOutputTokens },
    });

    if (updatedAssistantMessage) {
      sseEmitter.emitMessageUpdated(updatedAssistantMessage.info);
    }

    // Save resumeId for future resume (§5 Main Flow step 7c)
    if (result?.resumeId) {
      await store.update(sessionId, { resumeId: result.resumeId });
    }

    // Emit session.status { type: 'idle' } (§5 Main Flow step 7d)
    await store.update(sessionId, { status: 'idle' });
    sseEmitter.emitSessionStatus(sessionId, { type: 'idle' });

    logger.info('message completed', {
      sessionId,
      messageId: assistantMessageId,
      cost: totalCost,
      tokens: { input: totalInputTokens, output: totalOutputTokens },
    });

    // Return final response (§5 Main Flow step 8)
    const response: SendMessageResponse = {
      info: updatedAssistantMessage?.info ?? assistantMessage,
      parts: updatedAssistantMessage?.parts ?? parts,
    };

    return c.json(response);
  } catch (err) {
    logger.error('message execution failed', {
      sessionId,
      messageId: assistantMessageId,
      error: String(err),
    });

    // Update message with error
    const errorMessage = err instanceof Error ? err.message : String(err);
    await store.updateMessage(sessionId, assistantMessageId, {
      error: errorMessage,
      completedAt: Date.now(),
    });

    // Emit session.error event
    sseEmitter.emit('session.error', { sessionId, error: errorMessage });

    // Set session back to idle (still usable for retry per §6)
    await store.update(sessionId, { status: 'idle' });
    sseEmitter.emitSessionStatus(sessionId, { type: 'idle' });

    const apiError: ApiError = {
      code: ErrorCode.SDK_ERROR,
      message: 'Claude SDK error',
      details: errorMessage,
    };
    return c.json(apiError, 500);
  } finally {
    // Clean up execution tracking
    activeExecutions.delete(sessionId);
  }
});

/**
 * POST /session/:id/abort
 *
 * Abort the current execution (§4, §5 Abort Flow).
 */
messagesRouter.post('/:id/abort', async (c) => {
  const store = getSessionStore();
  const sessionId = c.req.param('id');

  // Get session
  const session = store.get(sessionId);
  if (!session) {
    const error: ApiError = {
      code: ErrorCode.SESSION_NOT_FOUND,
      message: 'Session not found',
    };
    return c.json(error, 404);
  }

  // Abort active execution if any (§5 Abort Flow step 2)
  const abortController = activeExecutions.get(sessionId);
  if (abortController) {
    abortController.abort();
    activeExecutions.delete(sessionId);
    logger.info('execution aborted', { sessionId });
  }

  // Emit session.status { type: 'idle' } (§5 Abort Flow step 3)
  await store.update(sessionId, { status: 'idle' });
  sseEmitter.emitSessionStatus(sessionId, { type: 'idle' });

  // Return success (§5 Abort Flow step 4)
  return c.json({ ok: true });
});

// --- Helpers ---

function isTextBlock(block: unknown): block is { type: 'text'; text: string } {
  return (
    typeof block === 'object' &&
    block !== null &&
    'type' in block &&
    (block as { type: unknown }).type === 'text' &&
    'text' in block
  );
}

export { activeExecutions };
