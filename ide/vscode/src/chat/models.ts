// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

export type StopReason =
    | 'end_turn'
    | 'max_tokens'
    | 'stop_sequence'
    | 'tool_use'
    | 'cancelled'
    | 'error';

export interface ChatMessage {
    id: string;
    role: 'user' | 'assistant' | 'system';
    content: string;
    timestamp: number;
    isStreaming?: boolean;
    toolCalls?: ToolCallStatus[];
    stopReason?: StopReason;
    error?: string;
}

export interface ToolCallStatus {
    id: string;
    toolName: string;
    arguments?: string;
    status: 'pending' | 'running' | 'completed' | 'failed';
    result?: string;
    error?: string;
}

export interface MessageContext {
    includeSelection?: boolean;
    filePath?: string;
    selectionText?: string;
    selectionRange?: { startLine: number; startColumn: number; endLine: number; endColumn: number };
    languageId?: string;
}

export type ConnectionStatus = 'connecting' | 'connected' | 'disconnected' | 'error';

export type ExtensionToWebviewMessage =
    | { type: 'messageAdded'; message: ChatMessage }
    | { type: 'messageUpdated'; messageId: string; content: Partial<ChatMessage> }
    | { type: 'streamingChunk'; messageId: string; text: string }
    | { type: 'toolCallUpdate'; messageId: string; toolCall: ToolCallStatus }
    | { type: 'turnCompleted'; messageId: string; stopReason: StopReason }
    | { type: 'sessionChanged'; session: SessionInfo }
    | { type: 'sessionsUpdated'; sessions: SessionInfo[] }
    | { type: 'conversationHistory'; messages: ChatMessage[] }
    | { type: 'connectionStatus'; status: ConnectionStatus; error?: string }
    | { type: 'error'; error: string };

export type WebviewToExtensionMessage =
    | { type: 'sendMessage'; text: string; includeSelection: boolean }
    | { type: 'cancel' }
    | { type: 'newSession' }
    | { type: 'switchSession'; sessionId: string }
    | { type: 'deleteSession'; sessionId: string }
    | { type: 'copyCode'; code: string }
    | { type: 'insertCode'; code: string }
    | { type: 'openFile'; path: string; line?: number }
    | { type: 'ready' };

export interface SessionInfo {
    id: string;
    title: string;
    createdAt: number;
    lastUsedAt: number;
    isActive: boolean;
}

export function generateMessageId(): string {
    return `msg_${Date.now()}_${Math.random().toString(36).substring(2, 11)}`;
}
