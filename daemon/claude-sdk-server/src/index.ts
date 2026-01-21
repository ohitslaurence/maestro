/**
 * Claude SDK Server entry point (§2)
 *
 * HTTP server wrapping Claude Agent SDK for per-workspace sessions.
 * Run: bun run src/index.ts --port 9100 --directory /path/to/project
 */

import { Hono } from 'hono';
import { sseEmitter } from './events/emitter';
import { logger } from './logger';
import { eventsRouter } from './routes/events';
import { messagesRouter } from './routes/messages';
import { sessionsRouter } from './routes/sessions';
import { initSessionStore } from './storage/sessions';

// Parse CLI arguments
function parseArgs(): { port: number; directory: string; workspaceId: string } {
  const args = process.argv.slice(2);
  let port = 9100;
  let directory = process.cwd();
  let workspaceId = 'default';

  for (let i = 0; i < args.length; i++) {
    if (args[i] === '--port' && args[i + 1]) {
      port = parseInt(args[i + 1], 10);
      i++;
    } else if (args[i] === '--directory' && args[i + 1]) {
      directory = args[i + 1];
      i++;
    } else if (args[i] === '--workspace-id' && args[i + 1]) {
      workspaceId = args[i + 1];
      i++;
    }
  }

  return { port, directory, workspaceId };
}

const config = parseArgs();

const app = new Hono();

// Health check endpoint (§4)
app.get('/health', (c) => {
  return c.json({ ok: true });
});

// Session routes (§4)
app.route('/session', sessionsRouter);

// Message routes (§4) - mounted under /session for :id/message paths
app.route('/session', messagesRouter);

// SSE event stream (§4)
app.route('/event', eventsRouter);

// Graceful shutdown handler
let isShuttingDown = false;

function shutdown(signal: string): void {
  if (isShuttingDown) return;
  isShuttingDown = true;

  logger.info('shutdown initiated', { signal });

  // Close all SSE connections
  sseEmitter.closeAll();

  // Give connections time to close gracefully
  setTimeout(() => {
    logger.info('shutdown complete');
    process.exit(0);
  }, 1000);
}

process.on('SIGTERM', () => shutdown('SIGTERM'));
process.on('SIGINT', () => shutdown('SIGINT'));

// Initialize session store and start server
async function start(): Promise<void> {
  logger.info('server starting', {
    port: config.port,
    directory: config.directory,
    workspaceId: config.workspaceId,
  });

  await initSessionStore(config.workspaceId, config.directory);
}

start().catch((err) => {
  logger.error('failed to start server', { error: String(err) });
  process.exit(1);
});

export default {
  port: config.port,
  fetch: app.fetch,
};

export { app, config };
