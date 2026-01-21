// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

/**
 * Webview state management for the chat panel.
 */

export interface ChatMessage {
    id: string;
    role: 'user' | 'assistant' | 'system';
    content: string;
    timestamp: number;
    isStreaming?: boolean;
    toolCalls?: ToolCallStatus[];
    stopReason?: string;
    error?: string;
}

export interface ToolCallStatus {
    id: string;
    toolName: string;
    arguments?: Record<string, unknown>;
    status: 'pending' | 'running' | 'completed' | 'failed';
    result?: string;
    error?: string;
}

export interface LoomSession {
    id: string;
    title: string;
    createdAt: number;
    lastUsedAt: number;
    isActive: boolean;
}

export interface ChatState {
    connectionStatus: 'connecting' | 'connected' | 'disconnected' | 'error';
    currentSession: LoomSession | null;
    sessions: LoomSession[];
    messages: ChatMessage[];
    inputText: string;
    includeSelection: boolean;
}

export function initializeState(persisted?: Partial<ChatState>): ChatState {
    return {
        connectionStatus: 'connecting',
        currentSession: null,
        sessions: [],
        messages: [],
        inputText: '',
        includeSelection: true,
        ...persisted,
    };
}

export function addMessage(state: ChatState, message: ChatMessage): void {
    state.messages.push(message);
}

export function updateMessage(state: ChatState, messageId: string, updates: Partial<ChatMessage>): void {
    const index = state.messages.findIndex(m => m.id === messageId);
    if (index !== -1) {
        state.messages[index] = { ...state.messages[index], ...updates };
    }
}

export function clearMessages(state: ChatState): void {
    state.messages = [];
}
