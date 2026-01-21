// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

import { EventEmitter } from 'events';

export interface ContentBlock {
    type: string;
    text?: string;
}

export interface SessionUpdate {
    type: string;
    sessionId: string;
    content?: string;
    index?: number;
    toolCallId?: string;
    toolName?: string;
    result?: string;
    isError?: boolean;
}

export interface AgentCapabilities {
    streaming: boolean;
    tools: string[];
    models: string[];
}

export interface InitializeResponse {
    protocolVersion: string;
    agentInfo: { name: string; version: string };
    capabilities: AgentCapabilities;
}

export class FakeAcpAgent extends EventEmitter {
    private sessions: Map<string, { id: string; messages: ContentBlock[] }> = new Map();
    private sessionCounter = 0;
    private _isInitialized = false;

    async initialize(): Promise<InitializeResponse> {
        this._isInitialized = true;
        return {
            protocolVersion: '1.0',
            agentInfo: { name: 'mock-agent', version: '1.0.0' },
            capabilities: {
                streaming: true,
                tools: ['read_file', 'write_file', 'execute_command'],
                models: ['claude-3-sonnet', 'claude-3-opus'],
            },
        };
    }

    get isInitialized(): boolean {
        return this._isInitialized;
    }

    async createSession(_cwd: string): Promise<{ sessionId: string }> {
        this.sessionCounter++;
        const sessionId = `mock-session-${this.sessionCounter}`;
        this.sessions.set(sessionId, { id: sessionId, messages: [] });
        return { sessionId };
    }

    async prompt(
        sessionId: string,
        content: ContentBlock[]
    ): Promise<{ stopReason: string }> {
        const session = this.sessions.get(sessionId);
        if (!session) {
            throw new Error(`Session not found: ${sessionId}`);
        }

        session.messages.push(...content);

        const userText = content
            .filter((block) => block.type === 'text')
            .map((block) => block.text || '')
            .join('');

        await this.simulateStreaming(sessionId, userText);

        if (this.shouldGenerateToolCall(userText)) {
            await this.simulateToolCall(sessionId, userText);
        }

        return { stopReason: 'end_turn' };
    }

    private async simulateStreaming(sessionId: string, userText: string): Promise<void> {
        const response = `Echo: ${userText}`;
        const chunks = this.splitIntoChunks(response, 10);

        for (let i = 0; i < chunks.length; i++) {
            await this.delay(10);
            this.emitUpdate({
                type: 'agent_message_chunk',
                sessionId,
                content: chunks[i],
                index: i,
            });
        }
    }

    private shouldGenerateToolCall(text: string): boolean {
        const toolTriggers = ['read file', 'write file', 'execute', 'run command'];
        return toolTriggers.some((trigger) => text.toLowerCase().includes(trigger));
    }

    private async simulateToolCall(sessionId: string, userText: string): Promise<void> {
        let toolName = 'read_file';
        if (userText.toLowerCase().includes('write')) {
            toolName = 'write_file';
        } else if (
            userText.toLowerCase().includes('execute') ||
            userText.toLowerCase().includes('run')
        ) {
            toolName = 'execute_command';
        }

        const toolCallId = `tool-call-${Date.now()}`;

        this.emitUpdate({
            type: 'tool_call_started',
            sessionId,
            toolCallId,
            toolName,
        });

        await this.delay(50);

        this.emitUpdate({
            type: 'tool_call_finished',
            sessionId,
            toolCallId,
            result: `Mock result for ${toolName}`,
            isError: false,
        });
    }

    private splitIntoChunks(text: string, chunkSize: number): string[] {
        const chunks: string[] = [];
        for (let i = 0; i < text.length; i += chunkSize) {
            chunks.push(text.slice(i, i + chunkSize));
        }
        return chunks;
    }

    private emitUpdate(update: SessionUpdate): void {
        this.emit('sessionUpdate', update);
    }

    private delay(ms: number): Promise<void> {
        return new Promise((resolve) => setTimeout(resolve, ms));
    }

    async cancel(_sessionId: string): Promise<void> {
        this.emit('cancelled');
    }

    getSession(sessionId: string): { id: string; messages: ContentBlock[] } | undefined {
        return this.sessions.get(sessionId);
    }

    reset(): void {
        this.sessions.clear();
        this.sessionCounter = 0;
        this._isInitialized = false;
    }
}
