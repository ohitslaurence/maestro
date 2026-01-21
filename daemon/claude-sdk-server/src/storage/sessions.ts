/**
 * Session storage (§3)
 *
 * In-memory store with file persistence to ~/.maestro/claude/{workspace_id}/
 * Atomic writes via temp file + rename to avoid corruption.
 */

import { randomUUID } from 'crypto';
import { mkdir, readdir, readFile, rename, unlink, writeFile } from 'fs/promises';
import { homedir } from 'os';
import { join } from 'path';
import { logger } from '../logger';
import type {
  CreateSessionRequest,
  ListSessionsQuery,
  MessageInfo,
  Part,
  Session,
} from '../types';

// --- Storage paths ---

function getStorageRoot(workspaceId: string): string {
  return join(homedir(), '.maestro', 'claude', workspaceId);
}

function getSessionsDir(workspaceId: string): string {
  return join(getStorageRoot(workspaceId), 'sessions');
}

function getMessagesDir(workspaceId: string): string {
  return join(getStorageRoot(workspaceId), 'messages');
}

function getIndexPath(workspaceId: string): string {
  return join(getStorageRoot(workspaceId), 'index.json');
}

// --- Atomic file operations ---

async function atomicWriteJson(path: string, data: unknown): Promise<void> {
  const tempPath = `${path}.tmp.${randomUUID()}`;
  const content = JSON.stringify(data, null, 2);
  await writeFile(tempPath, content, 'utf-8');
  await rename(tempPath, path);
}

async function readJson<T>(path: string): Promise<T | null> {
  try {
    const content = await readFile(path, 'utf-8');
    return JSON.parse(content) as T;
  } catch (err) {
    if ((err as NodeJS.ErrnoException).code === 'ENOENT') {
      return null;
    }
    throw err;
  }
}

// --- Index file schema ---

interface SessionIndex {
  sessions: SessionIndexEntry[];
}

interface SessionIndexEntry {
  id: string;
  title: string;
  updatedAt: number;
}

// --- Message file schema ---

interface MessageFile {
  info: MessageInfo;
  parts: Part[];
}

// --- Session Store ---

export class SessionStore {
  private workspaceId: string;
  private directory: string;
  private sessions: Map<string, Session> = new Map();
  private messages: Map<string, MessageFile[]> = new Map(); // sessionId -> messages
  private initialized = false;

  constructor(workspaceId: string, directory: string) {
    this.workspaceId = workspaceId;
    this.directory = directory;
  }

  /**
   * Initialize storage directories and load existing data from disk.
   */
  async init(): Promise<void> {
    if (this.initialized) return;

    const root = getStorageRoot(this.workspaceId);
    const sessionsDir = getSessionsDir(this.workspaceId);
    const messagesDir = getMessagesDir(this.workspaceId);

    // Create directories
    await mkdir(root, { recursive: true });
    await mkdir(sessionsDir, { recursive: true });
    await mkdir(messagesDir, { recursive: true });

    // Load sessions from disk
    await this.loadFromDisk();

    this.initialized = true;
    logger.info('session store initialized', {
      workspaceId: this.workspaceId,
      sessionCount: this.sessions.size,
    });
  }

  /**
   * Load all sessions and messages from disk into memory.
   */
  private async loadFromDisk(): Promise<void> {
    const sessionsDir = getSessionsDir(this.workspaceId);

    try {
      const files = await readdir(sessionsDir);
      for (const file of files) {
        if (!file.endsWith('.json')) continue;
        const sessionPath = join(sessionsDir, file);
        const session = await readJson<Session>(sessionPath);
        if (session) {
          this.sessions.set(session.id, session);
          // Load messages for this session
          await this.loadSessionMessages(session.id);
        }
      }
    } catch (err) {
      if ((err as NodeJS.ErrnoException).code !== 'ENOENT') {
        logger.error('failed to load sessions', { error: String(err) });
      }
    }
  }

  /**
   * Load messages for a specific session from disk.
   */
  private async loadSessionMessages(sessionId: string): Promise<void> {
    const sessionMessagesDir = join(getMessagesDir(this.workspaceId), sessionId);

    try {
      const files = await readdir(sessionMessagesDir);
      const msgs: MessageFile[] = [];

      for (const file of files) {
        if (!file.endsWith('.json')) continue;
        const msgPath = join(sessionMessagesDir, file);
        const msg = await readJson<MessageFile>(msgPath);
        if (msg) {
          msgs.push(msg);
        }
      }

      // Sort by createdAt
      msgs.sort((a, b) => a.info.createdAt - b.info.createdAt);
      this.messages.set(sessionId, msgs);
    } catch (err) {
      if ((err as NodeJS.ErrnoException).code !== 'ENOENT') {
        logger.warn('failed to load messages', { sessionId, error: String(err) });
      }
      this.messages.set(sessionId, []);
    }
  }

  /**
   * List sessions with optional filtering (§4: GET /session).
   */
  list(query: ListSessionsQuery = {}): Session[] {
    const { start, limit = 50, search } = query;
    let sessions = Array.from(this.sessions.values());

    // Filter by updatedAt
    if (start !== undefined) {
      sessions = sessions.filter((s) => s.updatedAt > start);
    }

    // Filter by title search
    if (search) {
      const searchLower = search.toLowerCase();
      sessions = sessions.filter((s) => s.title.toLowerCase().includes(searchLower));
    }

    // Sort by updatedAt descending (most recent first)
    sessions.sort((a, b) => b.updatedAt - a.updatedAt);

    // Apply limit
    return sessions.slice(0, limit);
  }

  /**
   * Get a session by ID.
   */
  get(id: string): Session | undefined {
    return this.sessions.get(id);
  }

  /**
   * Create a new session (§4: POST /session).
   * workspaceId and directory are server-assigned, not from request.
   */
  async create(request: CreateSessionRequest): Promise<Session> {
    const now = Date.now();
    const session: Session = {
      id: randomUUID(),
      workspaceId: this.workspaceId,
      directory: this.directory,
      title: request.title,
      parentId: request.parentId ?? undefined,
      modelId: request.modelId,
      createdAt: now,
      updatedAt: now,
      status: 'idle',
      permission: request.permission ?? 'default',
    };

    this.sessions.set(session.id, session);
    this.messages.set(session.id, []);

    await this.persistSession(session);
    await this.updateIndex();

    logger.info('session created', { sessionId: session.id, title: session.title });
    return session;
  }

  /**
   * Update a session.
   */
  async update(id: string, updates: Partial<Session>): Promise<Session | undefined> {
    const session = this.sessions.get(id);
    if (!session) return undefined;

    const updated: Session = {
      ...session,
      ...updates,
      id: session.id, // ID cannot be changed
      workspaceId: session.workspaceId, // Server-assigned
      directory: session.directory, // Server-assigned
      createdAt: session.createdAt, // Cannot be changed
      updatedAt: Date.now(),
    };

    this.sessions.set(id, updated);
    await this.persistSession(updated);
    await this.updateIndex();

    return updated;
  }

  /**
   * Delete a session and its messages.
   */
  async delete(id: string): Promise<boolean> {
    if (!this.sessions.has(id)) return false;

    this.sessions.delete(id);
    this.messages.delete(id);

    // Remove session file
    const sessionPath = join(getSessionsDir(this.workspaceId), `${id}.json`);
    try {
      await unlink(sessionPath);
    } catch {
      // Ignore if file doesn't exist
    }

    // Remove messages directory
    const messagesDir = join(getMessagesDir(this.workspaceId), id);
    try {
      const files = await readdir(messagesDir);
      for (const file of files) {
        await unlink(join(messagesDir, file));
      }
      await unlink(messagesDir);
    } catch {
      // Ignore if directory doesn't exist
    }

    await this.updateIndex();

    logger.info('session deleted', { sessionId: id });
    return true;
  }

  /**
   * Get messages for a session.
   */
  getMessages(sessionId: string): MessageFile[] {
    return this.messages.get(sessionId) ?? [];
  }

  /**
   * Add a message to a session.
   */
  async addMessage(sessionId: string, info: MessageInfo, parts: Part[] = []): Promise<void> {
    const session = this.sessions.get(sessionId);
    if (!session) {
      throw new Error(`Session not found: ${sessionId}`);
    }

    const msgs = this.messages.get(sessionId) ?? [];
    const msgFile: MessageFile = { info, parts };
    msgs.push(msgFile);
    this.messages.set(sessionId, msgs);

    await this.persistMessage(sessionId, msgFile);

    // Update session updatedAt
    await this.update(sessionId, {});
  }

  /**
   * Update a message (e.g., when parts are added or status changes).
   */
  async updateMessage(
    sessionId: string,
    messageId: string,
    updates: Partial<MessageInfo>,
    parts?: Part[]
  ): Promise<MessageFile | undefined> {
    const msgs = this.messages.get(sessionId);
    if (!msgs) return undefined;

    const idx = msgs.findIndex((m) => m.info.id === messageId);
    if (idx === -1) return undefined;

    const existing = msgs[idx];
    const updated: MessageFile = {
      info: { ...existing.info, ...updates, id: messageId, sessionId },
      parts: parts ?? existing.parts,
    };

    msgs[idx] = updated;
    await this.persistMessage(sessionId, updated);

    // Update session updatedAt
    await this.update(sessionId, {});

    return updated;
  }

  /**
   * Add or update a part within a message.
   */
  async upsertPart(sessionId: string, messageId: string, part: Part): Promise<void> {
    const msgs = this.messages.get(sessionId);
    if (!msgs) return;

    const msg = msgs.find((m) => m.info.id === messageId);
    if (!msg) return;

    const existingIdx = msg.parts.findIndex((p) => p.id === part.id);
    if (existingIdx >= 0) {
      msg.parts[existingIdx] = part;
    } else {
      msg.parts.push(part);
    }

    await this.persistMessage(sessionId, msg);
  }

  /**
   * Persist a session to disk.
   */
  private async persistSession(session: Session): Promise<void> {
    const sessionPath = join(getSessionsDir(this.workspaceId), `${session.id}.json`);
    await atomicWriteJson(sessionPath, session);
  }

  /**
   * Persist a message to disk.
   */
  private async persistMessage(sessionId: string, msg: MessageFile): Promise<void> {
    const sessionMessagesDir = join(getMessagesDir(this.workspaceId), sessionId);
    await mkdir(sessionMessagesDir, { recursive: true });

    const msgPath = join(sessionMessagesDir, `${msg.info.id}.json`);
    await atomicWriteJson(msgPath, msg);
  }

  /**
   * Update the index file for fast session listing.
   */
  private async updateIndex(): Promise<void> {
    const entries: SessionIndexEntry[] = Array.from(this.sessions.values()).map((s) => ({
      id: s.id,
      title: s.title,
      updatedAt: s.updatedAt,
    }));

    entries.sort((a, b) => b.updatedAt - a.updatedAt);

    const index: SessionIndex = { sessions: entries };
    await atomicWriteJson(getIndexPath(this.workspaceId), index);
  }
}

// --- Singleton for server process ---

let storeInstance: SessionStore | null = null;

export function getSessionStore(): SessionStore {
  if (!storeInstance) {
    throw new Error('Session store not initialized. Call initSessionStore first.');
  }
  return storeInstance;
}

export async function initSessionStore(workspaceId: string, directory: string): Promise<SessionStore> {
  storeInstance = new SessionStore(workspaceId, directory);
  await storeInstance.init();
  return storeInstance;
}
