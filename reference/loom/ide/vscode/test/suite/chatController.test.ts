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

type StopReason = 'end_turn' | 'max_tokens' | 'stop_sequence' | 'tool_use' | 'cancelled' | 'error';

interface ChatMessage {
    id: string;
    role: 'user' | 'assistant' | 'system';
    content: string;
    timestamp: number;
    isStreaming?: boolean;
    toolCalls?: ToolCallStatus[];
    stopReason?: StopReason;
}

interface ToolCallStatus {
    id: string;
    toolName: string;
    arguments?: string;
    status: 'pending' | 'running' | 'completed' | 'failed';
    result?: string;
    error?: string;
}

interface MessageContext {
    selectionText?: string;
    filePath?: string;
    languageId?: string;
}

interface SessionInfo {
    id: string;
    title: string;
    createdAt: number;
    lastUsedAt: number;
    isActive: boolean;
}

interface ContentBlock {
    type: string;
    text?: string;
}

interface SessionUpdate {
    type: string;
    sessionId: string;
    content?: string;
    index?: number;
    toolCallId?: string;
    toolName?: string;
    result?: string;
    isError?: boolean;
    delta?: string;
}

function generateMessageId(): string {
    return `msg_${Date.now()}_${Math.random().toString(36).substring(2, 11)}`;
}

class MockAcpClient extends EventEmitter {
    private _promptCalled = false;
    private _cancelCalled = false;

    async prompt(
        _sessionId: string,
        _content: ContentBlock[]
    ): Promise<{ stopReason: StopReason }> {
        this._promptCalled = true;
        return { stopReason: 'end_turn' };
    }

    async cancel(_sessionId: string): Promise<void> {
        this._cancelCalled = true;
    }

    get promptCalled(): boolean {
        return this._promptCalled;
    }

    get cancelCalled(): boolean {
        return this._cancelCalled;
    }

    simulateSessionUpdate(update: SessionUpdate): void {
        this.emit('sessionUpdate', { params: update });
    }
}

class MockSessionManager {
    private _currentSessionId: string | null = 'test-session';
    private _sessions: Map<string, SessionInfo> = new Map();

    constructor() {
        this._sessions.set('test-session', {
            id: 'test-session',
            title: 'Test Session',
            createdAt: Date.now(),
            lastUsedAt: Date.now(),
            isActive: true,
        });
    }

    getCurrentSessionId(): string | null {
        return this._currentSessionId;
    }

    async createSession(_cwd: string): Promise<string> {
        const sessionId = `session-${Date.now()}`;
        this._sessions.set(sessionId, {
            id: sessionId,
            title: 'New Session',
            createdAt: Date.now(),
            lastUsedAt: Date.now(),
            isActive: true,
        });
        this._currentSessionId = sessionId;
        return sessionId;
    }

    async switchSession(sessionId: string): Promise<void> {
        this._currentSessionId = sessionId;
    }

    getSessionInfo(sessionId: string): SessionInfo | null {
        return this._sessions.get(sessionId) || null;
    }
}

class MockWorkspaceService {
    private _workspaceRoot: string | undefined = '/workspace';

    getWorkspaceRoot(): string | undefined {
        return this._workspaceRoot;
    }

    setWorkspaceRoot(root: string | undefined): void {
        this._workspaceRoot = root;
    }
}

class ChatController extends EventEmitter {
    private acpClient: MockAcpClient;
    private sessionManager: MockSessionManager;

    private conversationHistory: Map<string, ChatMessage[]> = new Map();
    private currentAssistantMessageId: string | null = null;
    private _isProcessing = false;
    private pendingToolCalls: Map<string, ToolCallStatus> = new Map();

    get isProcessing(): boolean {
        return this._isProcessing;
    }

    constructor(
        acpClient: MockAcpClient,
        sessionManager: MockSessionManager,
        _workspaceService: MockWorkspaceService,
        _logger: MockLogger
    ) {
        super();
        this.acpClient = acpClient;
        this.sessionManager = sessionManager;
        this.setupAcpClientEvents();
    }

    private setupAcpClientEvents(): void {
        this.acpClient.on('sessionUpdate', (notification: { params: SessionUpdate }) => {
            this.handleSessionUpdate(notification.params);
        });
    }

    async handleUserMessage(text: string, context?: MessageContext): Promise<void> {
        const sessionId = this.sessionManager.getCurrentSessionId();
        if (!sessionId) {
            this.emit('error', 'No active session');
            return;
        }

        const userMessage: ChatMessage = {
            id: generateMessageId(),
            role: 'user',
            content: text,
            timestamp: Date.now(),
        };

        this.addMessageToHistory(sessionId, userMessage);
        this.emit('messageAdded', userMessage);

        this._isProcessing = true;

        try {
            const contentBlocks = this.buildContentBlocks(text, context);

            this.currentAssistantMessageId = generateMessageId();
            const assistantMessage: ChatMessage = {
                id: this.currentAssistantMessageId,
                role: 'assistant',
                content: '',
                timestamp: Date.now(),
                isStreaming: true,
                toolCalls: [],
            };
            this.addMessageToHistory(sessionId, assistantMessage);
            this.emit('messageAdded', assistantMessage);

            const response = await this.acpClient.prompt(sessionId, contentBlocks);
            this.finalizeAssistantMessage(sessionId, response.stopReason);
        } catch (error) {
            this.emit('error', error instanceof Error ? error.message : String(error));
        } finally {
            this._isProcessing = false;
        }
    }

    async cancelCurrentTurn(): Promise<void> {
        const sessionId = this.sessionManager.getCurrentSessionId();
        if (!sessionId) {
            return;
        }

        await this.acpClient.cancel(sessionId);
        this.emit('turnCancelled');
    }

    getConversationHistory(sessionId?: string): ChatMessage[] {
        const id = sessionId ?? this.sessionManager.getCurrentSessionId();
        if (!id) {
            return [];
        }
        return this.conversationHistory.get(id) || [];
    }

    private handleSessionUpdate(update: SessionUpdate): void {
        const sessionId = update.sessionId;

        if (update.type === 'agent_message_chunk') {
            if (this.currentAssistantMessageId) {
                this.appendToCurrentMessage(sessionId, update.content || '');
                this.emit('streamingChunk', {
                    messageId: this.currentAssistantMessageId,
                    content: update.content,
                });
            }
        } else if (update.type === 'tool_call_started') {
            const toolCallStatus: ToolCallStatus = {
                id: update.toolCallId!,
                toolName: update.toolName!,
                status: 'running',
            };
            this.pendingToolCalls.set(update.toolCallId!, toolCallStatus);

            if (this.currentAssistantMessageId) {
                this.addToolCallToMessage(sessionId, this.currentAssistantMessageId, toolCallStatus);
                this.emit('toolCallUpdate', {
                    messageId: this.currentAssistantMessageId,
                    toolCall: toolCallStatus,
                });
            }
        } else if (update.type === 'tool_call_arguments_delta') {
            const toolCall = this.pendingToolCalls.get(update.toolCallId!);
            if (toolCall) {
                toolCall.arguments = (toolCall.arguments || '') + (update.delta || '');
            }
        } else if (update.type === 'tool_call_finished') {
            const toolCall = this.pendingToolCalls.get(update.toolCallId!);
            if (toolCall) {
                toolCall.status = update.isError ? 'failed' : 'completed';
                toolCall.result = update.result;
                this.pendingToolCalls.delete(update.toolCallId!);

                if (this.currentAssistantMessageId) {
                    this.emit('toolCallUpdate', {
                        messageId: this.currentAssistantMessageId,
                        toolCall,
                    });
                }
            }
        }
    }

    private buildContentBlocks(text: string, context?: MessageContext): ContentBlock[] {
        const blocks: ContentBlock[] = [];

        if (context?.selectionText && context.filePath) {
            blocks.push({
                type: 'text',
                text: `File: ${context.filePath}\nSelection:\n\`\`\`\n${context.selectionText}\n\`\`\`\n\n`,
            });
        }

        blocks.push({ type: 'text', text });
        return blocks;
    }

    private addMessageToHistory(sessionId: string, message: ChatMessage): void {
        const history = this.conversationHistory.get(sessionId) || [];
        history.push(message);
        this.conversationHistory.set(sessionId, history);
    }

    private appendToCurrentMessage(sessionId: string, content: string): void {
        const history = this.conversationHistory.get(sessionId);
        if (!history || !this.currentAssistantMessageId) {
            return;
        }

        const message = history.find((m) => m.id === this.currentAssistantMessageId);
        if (message) {
            message.content += content;
            this.emit('messageUpdated', message);
        }
    }

    private addToolCallToMessage(
        sessionId: string,
        messageId: string,
        toolCall: ToolCallStatus
    ): void {
        const history = this.conversationHistory.get(sessionId);
        if (!history) {
            return;
        }

        const message = history.find((m) => m.id === messageId);
        if (message) {
            message.toolCalls = message.toolCalls || [];
            message.toolCalls.push(toolCall);
        }
    }

    private finalizeAssistantMessage(sessionId: string, stopReason: StopReason): void {
        const history = this.conversationHistory.get(sessionId);
        if (!history || !this.currentAssistantMessageId) {
            return;
        }

        const message = history.find((m) => m.id === this.currentAssistantMessageId);
        if (message) {
            message.isStreaming = false;
            message.stopReason = stopReason;
            this.emit('messageUpdated', message);
            this.emit('turnCompleted', {
                messageId: this.currentAssistantMessageId,
                stopReason,
            });
        }

        this.currentAssistantMessageId = null;
        this.pendingToolCalls.clear();
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

suite('ChatController Test Suite', () => {
    test('should add user message to history', async () => {
        const acpClient = new MockAcpClient();
        const sessionManager = new MockSessionManager();
        const workspaceService = new MockWorkspaceService();
        const logger = createMockLogger();
        const controller = new ChatController(acpClient, sessionManager, workspaceService, logger);

        const addedMessages: ChatMessage[] = [];
        controller.on('messageAdded', (msg: ChatMessage) => addedMessages.push(msg));

        await controller.handleUserMessage('Hello, world!');

        assert.strictEqual(addedMessages.length, 2);
        assert.strictEqual(addedMessages[0].role, 'user');
        assert.strictEqual(addedMessages[0].content, 'Hello, world!');
        assert.strictEqual(addedMessages[1].role, 'assistant');
    });

    test('should handle streaming chunks', async () => {
        const acpClient = new MockAcpClient();
        const sessionManager = new MockSessionManager();
        const workspaceService = new MockWorkspaceService();
        const logger = createMockLogger();
        const controller = new ChatController(acpClient, sessionManager, workspaceService, logger);

        await controller.handleUserMessage('Test');

        const streamingChunks: { content: string }[] = [];
        controller.on('streamingChunk', (chunk: { content: string }) => streamingChunks.push(chunk));

        acpClient.simulateSessionUpdate({
            type: 'agent_message_chunk',
            sessionId: 'test-session',
            content: 'Hello ',
            index: 0,
        });

        acpClient.simulateSessionUpdate({
            type: 'agent_message_chunk',
            sessionId: 'test-session',
            content: 'world!',
            index: 1,
        });

        assert.strictEqual(streamingChunks.length, 2);
        assert.strictEqual(streamingChunks[0].content, 'Hello ');
        assert.strictEqual(streamingChunks[1].content, 'world!');
    });

    test('should track tool call status', async () => {
        const acpClient = new MockAcpClient();
        const sessionManager = new MockSessionManager();
        const workspaceService = new MockWorkspaceService();
        const logger = createMockLogger();
        const controller = new ChatController(acpClient, sessionManager, workspaceService, logger);

        await controller.handleUserMessage('Test');

        const toolCallUpdates: { toolCall: ToolCallStatus }[] = [];
        controller.on('toolCallUpdate', (update: { toolCall: ToolCallStatus }) =>
            toolCallUpdates.push(update)
        );

        acpClient.simulateSessionUpdate({
            type: 'tool_call_started',
            sessionId: 'test-session',
            toolCallId: 'tool-1',
            toolName: 'read_file',
        });

        assert.strictEqual(toolCallUpdates.length, 1);
        assert.strictEqual(toolCallUpdates[0].toolCall.toolName, 'read_file');
        assert.strictEqual(toolCallUpdates[0].toolCall.status, 'running');

        acpClient.simulateSessionUpdate({
            type: 'tool_call_finished',
            sessionId: 'test-session',
            toolCallId: 'tool-1',
            result: 'file content',
            isError: false,
        });

        assert.strictEqual(toolCallUpdates.length, 2);
        assert.strictEqual(toolCallUpdates[1].toolCall.status, 'completed');
        assert.strictEqual(toolCallUpdates[1].toolCall.result, 'file content');
    });

    test('should emit events during prompt flow', async () => {
        const acpClient = new MockAcpClient();
        const sessionManager = new MockSessionManager();
        const workspaceService = new MockWorkspaceService();
        const logger = createMockLogger();
        const controller = new ChatController(acpClient, sessionManager, workspaceService, logger);

        const events: string[] = [];
        controller.on('messageAdded', () => events.push('messageAdded'));
        controller.on('messageUpdated', () => events.push('messageUpdated'));
        controller.on('turnCompleted', () => events.push('turnCompleted'));

        await controller.handleUserMessage('Test message');

        assert.ok(events.includes('messageAdded'));
        assert.ok(events.includes('turnCompleted'));
        assert.ok(acpClient.promptCalled);
    });

    test('should handle cancellation', async () => {
        const acpClient = new MockAcpClient();
        const sessionManager = new MockSessionManager();
        const workspaceService = new MockWorkspaceService();
        const logger = createMockLogger();
        const controller = new ChatController(acpClient, sessionManager, workspaceService, logger);

        let cancelledEmitted = false;
        controller.on('turnCancelled', () => {
            cancelledEmitted = true;
        });

        await controller.cancelCurrentTurn();

        assert.strictEqual(acpClient.cancelCalled, true);
        assert.strictEqual(cancelledEmitted, true);
    });
});
