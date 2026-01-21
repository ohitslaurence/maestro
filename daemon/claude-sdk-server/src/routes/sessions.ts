/**
 * Session routes (§4)
 *
 * HTTP endpoints for session CRUD operations.
 */

import { Hono } from 'hono';
import { getSessionStore } from '../storage/sessions';
import { ErrorCode, type ApiError, type CreateSessionRequest, type ListSessionsQuery } from '../types';

export const sessionsRouter = new Hono();

/**
 * GET /session
 *
 * List sessions for this workspace.
 * Query params: start, limit, search
 */
sessionsRouter.get('/', (c) => {
  const store = getSessionStore();

  const query: ListSessionsQuery = {};

  const startParam = c.req.query('start');
  if (startParam) {
    const start = parseInt(startParam, 10);
    if (!isNaN(start)) {
      query.start = start;
    }
  }

  const limitParam = c.req.query('limit');
  if (limitParam) {
    const limit = parseInt(limitParam, 10);
    if (!isNaN(limit) && limit > 0) {
      query.limit = limit;
    }
  }

  const search = c.req.query('search');
  if (search) {
    query.search = search;
  }

  const sessions = store.list(query);
  return c.json(sessions);
});

/**
 * POST /session
 *
 * Create a new session.
 * Body: { title, parentId?, permission?, modelId? }
 */
sessionsRouter.post('/', async (c) => {
  const store = getSessionStore();

  let body: CreateSessionRequest;
  try {
    body = await c.req.json<CreateSessionRequest>();
  } catch {
    const error: ApiError = {
      code: ErrorCode.INVALID_REQUEST,
      message: 'Invalid JSON body',
    };
    return c.json(error, 400);
  }

  // Validate required fields
  if (!body.title || typeof body.title !== 'string') {
    const error: ApiError = {
      code: ErrorCode.INVALID_REQUEST,
      message: 'Missing or invalid "title" field',
    };
    return c.json(error, 400);
  }

  // Validate permission if provided
  if (body.permission !== undefined) {
    const validPermissions = ['default', 'acceptEdits', 'bypassPermissions'];
    if (!validPermissions.includes(body.permission)) {
      const error: ApiError = {
        code: ErrorCode.INVALID_REQUEST,
        message: `Invalid permission mode. Must be one of: ${validPermissions.join(', ')}`,
      };
      return c.json(error, 400);
    }
  }

  const session = await store.create(body);
  return c.json(session, 201);
});

/**
 * GET /session/:id
 *
 * Get session details.
 */
sessionsRouter.get('/:id', (c) => {
  const store = getSessionStore();
  const id = c.req.param('id');

  const session = store.get(id);
  if (!session) {
    const error: ApiError = {
      code: ErrorCode.SESSION_NOT_FOUND,
      message: 'Session not found',
    };
    return c.json(error, 404);
  }

  return c.json(session);
});
