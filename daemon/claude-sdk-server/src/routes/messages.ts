/**
 * Message routes (§4, §5)
 *
 * HTTP endpoint for sending messages to sessions.
 */

import { randomUUID } from 'crypto';
import { Hono } from 'hono';
import { sseEmitter } from '../events/emitter';
import { isTextBlock, MessageMapper } from '../events/mapper';
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

  // Create mapper for SDK message → Part conversion (§3, Appendix B)
  const mapper = new MessageMapper(assistantMessageId);

  // Track text accumulation for delta streaming
  // Maps content block index to accumulated text for computing deltas
  const textAccumulator = new Map<number, { partId: string; text: string }>();

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
      if (isAssistantMessage(message)) {
        for (let blockIndex = 0; blockIndex < message.message.content.length; blockIndex++) {
          const block = message.message.content[blockIndex];

          // Map text content blocks to TextPart with delta streaming (Phase 5)
          if (isTextBlock(block)) {
            const existing = textAccumulator.get(blockIndex);

            if (existing) {
              // Compute delta: new text since last update
              const delta = block.text.slice(existing.text.length);
              if (delta.length > 0) {
                existing.text = block.text;

                // Find and update the existing part
                const partIndex = parts.findIndex((p) => p.id === existing.partId);
                if (partIndex !== -1 && parts[partIndex].type === 'text') {
                  (parts[partIndex] as TextPart).text = block.text;
                  await store.upsertPart(sessionId, assistantMessageId, parts[partIndex]);
                  sseEmitter.emitMessagePartUpdated(parts[partIndex], delta);
                }
              }
            } else {
              // First time seeing this text block - create new part
              const { part, delta } = mapper.mapTextBlock(block);
              textAccumulator.set(blockIndex, { partId: part.id, text: block.text });
              parts.push(part);
              await store.upsertPart(sessionId, assistantMessageId, part);
              sseEmitter.emitMessagePartUpdated(part, delta);
            }
          }

          // TODO Phase 5: Map thinking blocks to ReasoningPart
          // TODO Phase 5: Map tool_use blocks to ToolPart with status transitions
          // TODO Phase 5: Map tool_result blocks to ToolPart completion
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

export { activeExecutions };
