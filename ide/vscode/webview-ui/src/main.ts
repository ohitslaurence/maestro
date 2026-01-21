// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

/**
 * Webview UI entry point for the Loom chat panel.
 * This file is compiled and bundled into media/chat.js
 */

import { ChatState, initializeState } from './state/chatState';
import { renderMessages } from './components/messageList';
import { setupInputHandler } from './components/inputArea';
import { setupHeaderHandler } from './components/header';

declare const acquireVsCodeApi: () => {
    postMessage: (message: unknown) => void;
    getState: () => unknown;
    setState: (state: unknown) => void;
};

const vscode = acquireVsCodeApi();

let state: ChatState;

function initialize(): void {
    state = initializeState(vscode.getState() as Partial<ChatState> | undefined);
    
    setupInputHandler(vscode, state);
    setupHeaderHandler(vscode, state);
    renderMessages(state.messages);
    
    // Notify extension we're ready
    vscode.postMessage({ type: 'ready' });
}

// Handle messages from extension
window.addEventListener('message', (event) => {
    const message = event.data;
    
    switch (message.type) {
        case 'connectionStatus':
            state.connectionStatus = message.status;
            updateConnectionIndicator(message.status, message.error);
            break;
            
        case 'sessionChanged':
            state.currentSession = message.session;
            updateSessionHeader(message.session);
            break;
            
        case 'sessionsUpdated':
            state.sessions = message.sessions;
            updateSessionList(message.sessions);
            break;
            
        case 'messageAdded':
            state.messages.push(message.message);
            renderMessages(state.messages);
            scrollToBottom();
            break;
            
        case 'messageUpdated':
            updateMessage(message.messageId, message.content);
            break;
            
        case 'streamingChunk':
            appendStreamingChunk(message.messageId, message.text);
            break;
            
        case 'toolCallUpdate':
            updateToolCall(message.messageId, message.toolCall);
            break;
            
        case 'turnCompleted':
            completeTurn(message.messageId, message.stopReason);
            break;
            
        case 'error':
            showError(message.error);
            break;
    }
    
    // Persist state
    vscode.setState(state);
});

function updateConnectionIndicator(status: string, error?: string): void {
    const indicator = document.getElementById('connection-indicator');
    if (indicator) {
        indicator.className = `connection-status ${status}`;
        indicator.title = error || status;
    }
}

function updateSessionHeader(session: { id: string; title: string }): void {
    const header = document.getElementById('session-title');
    if (header) {
        header.textContent = session.title || 'New Session';
    }
}

function updateSessionList(_sessions: Array<{ id: string; title: string }>): void {
    // Session list dropdown update logic
}

function updateMessage(messageId: string, content: Partial<{ content: string }>): void {
    const element = document.querySelector(`[data-message-id="${messageId}"]`);
    if (element && content.content) {
        const contentEl = element.querySelector('.message-content');
        if (contentEl) {
            contentEl.textContent = content.content;
        }
    }
}

function appendStreamingChunk(messageId: string, text: string): void {
    const element = document.querySelector(`[data-message-id="${messageId}"]`);
    if (element) {
        const contentEl = element.querySelector('.message-content');
        if (contentEl) {
            contentEl.textContent += text;
        }
    }
    scrollToBottom();
}

function updateToolCall(messageId: string, toolCall: { id: string; status: string }): void {
    const element = document.querySelector(`[data-message-id="${messageId}"] [data-tool-id="${toolCall.id}"]`);
    if (element) {
        element.className = `tool-call ${toolCall.status}`;
    }
}

function completeTurn(messageId: string, _stopReason: string): void {
    const element = document.querySelector(`[data-message-id="${messageId}"]`);
    if (element) {
        element.classList.remove('streaming');
        element.classList.add('completed');
    }
}

function showError(error: string): void {
    const errorContainer = document.getElementById('error-container');
    if (errorContainer) {
        errorContainer.textContent = error;
        errorContainer.style.display = 'block';
        setTimeout(() => {
            errorContainer.style.display = 'none';
        }, 5000);
    }
}

function scrollToBottom(): void {
    const container = document.getElementById('messages-container');
    if (container) {
        container.scrollTop = container.scrollHeight;
    }
}

// Initialize when DOM is ready
if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', initialize);
} else {
    initialize();
}
