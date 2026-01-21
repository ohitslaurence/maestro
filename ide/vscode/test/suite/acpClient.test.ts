// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

import * as assert from 'assert';
import { EventEmitter } from 'events';

interface MockLogger {
    info(message: string, data?: Record<string, unknown>): void;
    warn(message: string, data?: Record<string, unknown>): void;
    error(message: string, data?: Record<string, unknown>): void;
    debug(message: string, data?: Record<string, unknown>): void;
}

interface StdioStreams {
    stdin: NodeJS.WritableStream;
    stdout: NodeJS.ReadableStream;
}

class MockProcessManager extends EventEmitter {
    private _isRunning = false;
    private _stdio: StdioStreams | null = null;

    async start(): Promise<void> {
        this._isRunning = true;
        this._stdio = {
            stdin: process.stdin,
            stdout: process.stdout,
        };
        this.emit('ready');
    }

    getStdio(): StdioStreams | null {
        return this._stdio;
    }

    isRunning(): boolean {
        return this._isRunning;
    }

    simulateExit(code: number | null): void {
        this._isRunning = false;
        this._stdio = null;
        this.emit('exit', code);
    }

    simulateError(error: Error): void {
        this.emit('error', error);
    }
}

interface AgentCapabilities {
    streaming: boolean;
    tools: string[];
    models: string[];
    maxContextLength?: number;
}

interface InitializeResponse {
    protocolVersion: string;
    agentInfo?: { name: string; version: string };
    capabilities: AgentCapabilities;
}

interface SessionNotification {
    type: string;
    sessionId: string;
}

class MockConnection extends EventEmitter {
    private _initializeCalled = false;
    private _createSessionCount = 0;

    async initialize(params: {
        protocolVersion: string;
        clientInfo: { name: string; version: string };
        capabilities: { streaming: boolean; sessions: boolean };
    }): Promise<InitializeResponse> {
        this._initializeCalled = true;
        return {
            protocolVersion: params.protocolVersion,
            agentInfo: { name: 'test-agent', version: '1.0.0' },
            capabilities: {
                streaming: true,
                tools: ['read_file', 'write_file'],
                models: ['claude-3'],
            },
        };
    }

    async createSession(_params: { cwd: string }): Promise<{ sessionId: string }> {
        this._createSessionCount++;
        return { sessionId: `session-${this._createSessionCount}` };
    }

    async loadSession(_params: { sessionId: string }): Promise<void> {}

    async prompt(_params: { sessionId: string; content: unknown[] }): Promise<{ stopReason: string }> {
        return { stopReason: 'end_turn' };
    }

    async cancel(_params: { sessionId: string }): Promise<void> {}

    get initializeCalled(): boolean {
        return this._initializeCalled;
    }

    simulateNotification(notification: SessionNotification): void {
        this.emit('notification', notification);
    }
}

class AcpClient extends EventEmitter {
    private connection: MockConnection | null = null;
    private processManager: MockProcessManager;
    private logger: MockLogger;
    private _isInitialized = false;
    private _agentCapabilities: AgentCapabilities | null = null;

    constructor(processManager: MockProcessManager, logger: MockLogger) {
        super();
        this.processManager = processManager;
        this.logger = logger;
        this.setupProcessManagerEvents();
    }

    get isInitialized(): boolean {
        return this._isInitialized;
    }

    get agentCapabilities(): AgentCapabilities | null {
        return this._agentCapabilities;
    }

    private setupProcessManagerEvents(): void {
        this.processManager.on('ready', () => {
            this.setupConnection();
        });

        this.processManager.on('exit', (code: number | null) => {
            this.logger.info('Process exited', { exitCode: code });
            this._isInitialized = false;
            this.connection = null;
            this.emit('disconnected');
        });

        this.processManager.on('error', (error: Error) => {
            this.logger.error('Process manager error', { error: error.message });
            this.emit('error', { code: 'PROCESS_ERROR', message: error.message, cause: error });
        });
    }

    private setupConnection(): void {
        const stdio = this.processManager.getStdio();
        if (!stdio) {
            return;
        }

        this.connection = new MockConnection();
        this.connection.on('notification', (notification: SessionNotification) => {
            this.emit('sessionUpdate', notification);
        });
    }

    async ensureStarted(): Promise<void> {
        if (!this.processManager.isRunning()) {
            await this.processManager.start();
        }
        if (!this._isInitialized) {
            await this.initialize();
        }
    }

    async initialize(): Promise<InitializeResponse> {
        if (!this.connection) {
            throw new Error('Connection not established');
        }

        const response = await this.connection.initialize({
            protocolVersion: '1.0',
            clientInfo: { name: 'loom-vscode', version: '1.0.0' },
            capabilities: { streaming: true, sessions: true },
        });

        this._isInitialized = true;
        this._agentCapabilities = response.capabilities;
        this.emit('initialized', response.capabilities);

        return response;
    }

    async newSession(cwd: string): Promise<{ sessionId: string }> {
        if (!this.connection) {
            throw new Error('Connection not established');
        }
        return this.connection.createSession({ cwd });
    }

    async loadSession(sessionId: string): Promise<void> {
        if (!this.connection) {
            throw new Error('Connection not established');
        }
        await this.connection.loadSession({ sessionId });
    }

    getConnection(): MockConnection | null {
        return this.connection;
    }
}

function createMockLogger(): MockLogger {
    return {
        info: () => {},
        warn: () => {},
        error: () => {},
        debug: () => {},
    };
}

suite('AcpClient Test Suite', () => {
    test('should initialize connection', async () => {
        const processManager = new MockProcessManager();
        const logger = createMockLogger();
        const client = new AcpClient(processManager, logger);

        assert.strictEqual(client.isInitialized, false);

        await client.ensureStarted();

        assert.strictEqual(client.isInitialized, true);
        assert.ok(client.agentCapabilities);
        assert.strictEqual(client.agentCapabilities?.streaming, true);
    });

    test('should create new session', async () => {
        const processManager = new MockProcessManager();
        const logger = createMockLogger();
        const client = new AcpClient(processManager, logger);

        await client.ensureStarted();

        const result = await client.newSession('/workspace');

        assert.strictEqual(result.sessionId, 'session-1');

        const result2 = await client.newSession('/workspace');
        assert.strictEqual(result2.sessionId, 'session-2');
    });

    test('should handle session updates', async () => {
        const processManager = new MockProcessManager();
        const logger = createMockLogger();
        const client = new AcpClient(processManager, logger);

        await client.ensureStarted();

        const receivedUpdates: SessionNotification[] = [];
        client.on('sessionUpdate', (update: SessionNotification) => {
            receivedUpdates.push(update);
        });

        const connection = client.getConnection();
        assert.ok(connection);

        connection.simulateNotification({
            type: 'agent_message_chunk',
            sessionId: 'test-session',
        });

        assert.strictEqual(receivedUpdates.length, 1);
        assert.strictEqual(receivedUpdates[0].type, 'agent_message_chunk');
        assert.strictEqual(receivedUpdates[0].sessionId, 'test-session');
    });

    test('should emit disconnected on process exit', async () => {
        const processManager = new MockProcessManager();
        const logger = createMockLogger();
        const client = new AcpClient(processManager, logger);

        await client.ensureStarted();
        assert.strictEqual(client.isInitialized, true);

        let disconnectedEmitted = false;
        client.on('disconnected', () => {
            disconnectedEmitted = true;
        });

        processManager.simulateExit(0);

        assert.strictEqual(disconnectedEmitted, true);
        assert.strictEqual(client.isInitialized, false);
    });

    test('should track initialization state', async () => {
        const processManager = new MockProcessManager();
        const logger = createMockLogger();
        const client = new AcpClient(processManager, logger);

        assert.strictEqual(client.isInitialized, false);
        assert.strictEqual(client.agentCapabilities, null);

        await client.ensureStarted();

        assert.strictEqual(client.isInitialized, true);
        const capabilities = client.agentCapabilities as unknown as AgentCapabilities;
        assert.ok(capabilities);
        assert.deepStrictEqual(capabilities.tools, ['read_file', 'write_file']);
        assert.deepStrictEqual(capabilities.models, ['claude-3']);
    });
});
