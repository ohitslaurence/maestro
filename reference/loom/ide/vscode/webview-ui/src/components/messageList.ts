// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

/**
 * Message list rendering component.
 */

import type { ChatMessage, ToolCallStatus } from '../state/chatState';

export function renderMessages(messages: ChatMessage[]): void {
    const container = document.getElementById('messages-container');
    if (!container) return;

    container.innerHTML = messages.map(renderMessage).join('');
}

function renderMessage(message: ChatMessage): string {
    const roleClass = message.role;
    const streamingClass = message.isStreaming ? 'streaming' : '';
    const roleIcon = message.role === 'user' ? '👤' : '🤖';
    const timestamp = formatTimestamp(message.timestamp);

    return `
        <div class="message ${roleClass} ${streamingClass}" data-message-id="${message.id}">
            <div class="message-header">
                <span class="message-role">${roleIcon} ${capitalize(message.role)}</span>
                <span class="message-time">${timestamp}</span>
            </div>
            <div class="message-content">${escapeHtml(message.content)}</div>
            ${message.toolCalls ? renderToolCalls(message.toolCalls) : ''}
            ${message.error ? `<div class="message-error">${escapeHtml(message.error)}</div>` : ''}
        </div>
    `;
}

function renderToolCalls(toolCalls: ToolCallStatus[]): string {
    return `
        <div class="tool-calls">
            ${toolCalls.map(renderToolCall).join('')}
        </div>
    `;
}

function renderToolCall(toolCall: ToolCallStatus): string {
    const statusIcon = getStatusIcon(toolCall.status);
    return `
        <div class="tool-call ${toolCall.status}" data-tool-id="${toolCall.id}">
            <span class="tool-icon">🔧</span>
            <span class="tool-name">${escapeHtml(toolCall.toolName)}</span>
            <span class="tool-status">${statusIcon} ${toolCall.status}</span>
            ${toolCall.result ? `<div class="tool-result">${escapeHtml(toolCall.result)}</div>` : ''}
            ${toolCall.error ? `<div class="tool-error">${escapeHtml(toolCall.error)}</div>` : ''}
        </div>
    `;
}

function getStatusIcon(status: ToolCallStatus['status']): string {
    switch (status) {
        case 'pending': return '⏳';
        case 'running': return '⏳';
        case 'completed': return '✓';
        case 'failed': return '✗';
    }
}

function formatTimestamp(timestamp: number): string {
    return new Date(timestamp).toLocaleTimeString([], { 
        hour: '2-digit', 
        minute: '2-digit' 
    });
}

function capitalize(str: string): string {
    return str.charAt(0).toUpperCase() + str.slice(1);
}

function escapeHtml(text: string): string {
    const div = document.createElement('div');
    div.textContent = text;
    return div.innerHTML;
}
