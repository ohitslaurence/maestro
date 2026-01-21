// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

import * as assert from 'assert';
import { EventEmitter } from 'events';

interface LoomSession {
    id: string;
    title: string;
    createdAt: number;
    lastUsedAt: number;
    isActive: boolean;
}

interface WorkspaceSessionsState {
    activeSessionId?: string;
    sessions: LoomSession[];
}

interface MockMemento {
    get<T>(key: string): T | undefined;
    update(key: string, value: unknown): Promise<void>;
}

interface MockAcpClient {
    newSession(): Promise<{ sessionId: string }>;
    loadSession(sessionId: string): Promise<void>;
}

interface MockLogger {
    info(message: string, data?: Record<string, unknown>): void;
    warn(message: string, data?: Record<string, unknown>): void;
    error(message: string, data?: Record<string, unknown>): void;
    debug(message: string, data?: Record<string, unknown>): void;
}

const STATE_KEY = 'loom.sessions';

class SessionManager extends EventEmitter {
    private sessions: Map<string, LoomSession> = new Map();
    private activeSessionId: string | undefined;

    constructor(
        private readonly workspaceState: MockMemento,
        private readonly acpClient: MockAcpClient,
        private readonly logger: MockLogger
    ) {
        super();
        this.loadFromState();
    }

    loadFromState(): void {
        const state = this.workspaceState.get<WorkspaceSessionsState>(STATE_KEY);
        if (state) {
            this.sessions.clear();
            for (const session of state.sessions) {
                this.sessions.set(session.id, session);
            }
            this.activeSessionId = state.activeSessionId;
            this.logger.info('Loaded sessions from state', {
                sessionCount: this.sessions.size,
                activeSessionId: this.activeSessionId,
            });
        }
    }

    async saveToState(): Promise<void> {
        const state: WorkspaceSessionsState = {
            activeSessionId: this.activeSessionId,
            sessions: Array.from(this.sessions.values()),
        };
        await this.workspaceState.update(STATE_KEY, state);
    }

    getActiveSession(): LoomSession | undefined {
        if (!this.activeSessionId) {
            return undefined;
        }
        return this.sessions.get(this.activeSessionId);
    }

    getAllSessions(): LoomSession[] {
        return Array.from(this.sessions.values()).sort((a, b) => b.lastUsedAt - a.lastUsedAt);
    }

    async createNewSession(_cwd: string): Promise<LoomSession> {
        const response = await this.acpClient.newSession();
        const now = Date.now();
        const session: LoomSession = {
            id: response.sessionId,
            title: 'New Session',
            createdAt: now,
            lastUsedAt: now,
            isActive: true,
        };

        if (this.activeSessionId) {
            const previousSession = this.sessions.get(this.activeSessionId);
            if (previousSession) {
                previousSession.isActive = false;
            }
        }

        this.sessions.set(session.id, session);
        this.activeSessionId = session.id;

        await this.saveToState();
        this.emit('sessionCreated', session);
        this.emit('activeSessionChanged', session);

        return session;
    }

    async setActiveSession(sessionId: string): Promise<void> {
        const session = this.sessions.get(sessionId);
        if (!session) {
            this.logger.warn('Attempted to set non-existent session as active', { sessionId });
            return;
        }

        if (this.activeSessionId) {
            const previousSession = this.sessions.get(this.activeSessionId);
            if (previousSession) {
                previousSession.isActive = false;
            }
        }

        try {
            await this.acpClient.loadSession(sessionId);
        } catch {
            this.logger.warn('Failed to load session from server, continuing anyway', { sessionId });
        }

        session.isActive = true;
        session.lastUsedAt = Date.now();
        this.activeSessionId = sessionId;

        await this.saveToState();
        this.emit('activeSessionChanged', session);
    }

    deleteSession(sessionId: string): void {
        const session = this.sessions.get(sessionId);
        if (!session) {
            return;
        }

        this.sessions.delete(sessionId);

        if (this.activeSessionId === sessionId) {
            this.activeSessionId = undefined;
        }

        this.emit('sessionDeleted', sessionId);
    }

    deriveSessionTitleFromMessage(text: string): string {
        const firstLine = text.split('\n')[0] || '';
        return firstLine.slice(0, 50);
    }
}

function createMockMemento(initialState?: WorkspaceSessionsState): MockMemento {
    let storedState: WorkspaceSessionsState | undefined = initialState;
    return {
        get<T>(key: string): T | undefined {
            if (key === STATE_KEY) {
                return storedState as T | undefined;
            }
            return undefined;
        },
        update: async (key: string, value: unknown): Promise<void> => {
            if (key === STATE_KEY) {
                storedState = value as WorkspaceSessionsState;
            }
        },
    };
}

function createMockAcpClient(): MockAcpClient {
    let sessionCounter = 0;
    return {
        newSession: async (): Promise<{ sessionId: string }> => {
            sessionCounter++;
            return { sessionId: `session-${sessionCounter}` };
        },
        loadSession: async (): Promise<void> => {},
    };
}

function createMockLogger(): MockLogger {
    return {
        info: () => {},
        warn: () => {},
        error: () => {},
        debug: () => {},
    };
}

suite('SessionManager Test Suite', () => {
    test('should create new session', async () => {
        const memento = createMockMemento();
        const acpClient = createMockAcpClient();
        const logger = createMockLogger();
        const manager = new SessionManager(memento, acpClient, logger);

        const session = await manager.createNewSession('/workspace');

        assert.strictEqual(session.id, 'session-1');
        assert.strictEqual(session.title, 'New Session');
        assert.strictEqual(session.isActive, true);
        assert.strictEqual(manager.getActiveSession()?.id, session.id);
    });

    test('should load sessions from state', () => {
        const existingSessions: LoomSession[] = [
            { id: 'existing-1', title: 'Test Session', createdAt: 1000, lastUsedAt: 2000, isActive: true },
            { id: 'existing-2', title: 'Another Session', createdAt: 1500, lastUsedAt: 1800, isActive: false },
        ];
        const initialState: WorkspaceSessionsState = {
            activeSessionId: 'existing-1',
            sessions: existingSessions,
        };
        const memento = createMockMemento(initialState);
        const acpClient = createMockAcpClient();
        const logger = createMockLogger();
        const manager = new SessionManager(memento, acpClient, logger);

        const allSessions = manager.getAllSessions();
        assert.strictEqual(allSessions.length, 2);
        assert.strictEqual(manager.getActiveSession()?.id, 'existing-1');
    });

    test('should switch active session', async () => {
        const existingSessions: LoomSession[] = [
            { id: 'session-a', title: 'Session A', createdAt: 1000, lastUsedAt: 2000, isActive: true },
            { id: 'session-b', title: 'Session B', createdAt: 1500, lastUsedAt: 1800, isActive: false },
        ];
        const initialState: WorkspaceSessionsState = {
            activeSessionId: 'session-a',
            sessions: existingSessions,
        };
        const memento = createMockMemento(initialState);
        const acpClient = createMockAcpClient();
        const logger = createMockLogger();
        const manager = new SessionManager(memento, acpClient, logger);

        await manager.setActiveSession('session-b');

        assert.strictEqual(manager.getActiveSession()?.id, 'session-b');
        assert.strictEqual(manager.getActiveSession()?.isActive, true);
    });

    test('should delete session', async () => {
        const memento = createMockMemento();
        const acpClient = createMockAcpClient();
        const logger = createMockLogger();
        const manager = new SessionManager(memento, acpClient, logger);

        const session = await manager.createNewSession('/workspace');
        assert.strictEqual(manager.getAllSessions().length, 1);

        manager.deleteSession(session.id);

        assert.strictEqual(manager.getAllSessions().length, 0);
        assert.strictEqual(manager.getActiveSession(), undefined);
    });

    test('should derive title from message', () => {
        const memento = createMockMemento();
        const acpClient = createMockAcpClient();
        const logger = createMockLogger();
        const manager = new SessionManager(memento, acpClient, logger);

        const title1 = manager.deriveSessionTitleFromMessage('Hello world\nMore content here');
        assert.strictEqual(title1, 'Hello world');

        const longMessage = 'A'.repeat(100);
        const title2 = manager.deriveSessionTitleFromMessage(longMessage);
        assert.strictEqual(title2.length, 50);
    });

    test('should emit events on session changes', async () => {
        const memento = createMockMemento();
        const acpClient = createMockAcpClient();
        const logger = createMockLogger();
        const manager = new SessionManager(memento, acpClient, logger);

        const emittedEvents: string[] = [];
        manager.on('sessionCreated', () => emittedEvents.push('sessionCreated'));
        manager.on('activeSessionChanged', () => emittedEvents.push('activeSessionChanged'));
        manager.on('sessionDeleted', () => emittedEvents.push('sessionDeleted'));

        const session = await manager.createNewSession('/workspace');
        assert.ok(emittedEvents.includes('sessionCreated'));
        assert.ok(emittedEvents.includes('activeSessionChanged'));

        manager.deleteSession(session.id);
        assert.ok(emittedEvents.includes('sessionDeleted'));
    });
});
