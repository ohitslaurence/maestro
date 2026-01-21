/**
 * Events route (§4)
 *
 * SSE endpoint for real-time event streaming.
 */

import { Hono } from 'hono';
import { sseEmitter } from '../events/emitter';
import { logger } from '../logger';

export const eventsRouter = new Hono();

/**
 * GET /event
 *
 * SSE event stream for this workspace (§4).
 * Events are sent as: data: {"type":"...", "properties":{...}}\n\n
 */
eventsRouter.get('/', (c) => {
  const { stream, clientId } = sseEmitter.addClient();

  logger.info('sse client connected', { clientId });

  return new Response(stream, {
    headers: {
      'Content-Type': 'text/event-stream',
      'Cache-Control': 'no-cache',
      Connection: 'keep-alive',
      'X-Client-Id': clientId,
    },
  });
});
