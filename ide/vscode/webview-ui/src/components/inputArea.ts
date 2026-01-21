// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

/**
 * Input area component for the chat panel.
 */

import type { ChatState } from '../state/chatState';

interface VsCodeApi {
    postMessage: (message: unknown) => void;
}

export function setupInputHandler(vscode: VsCodeApi, state: ChatState): void {
    const textarea = document.getElementById('message-input') as HTMLTextAreaElement | null;
    const sendButton = document.getElementById('send-button');
    const cancelButton = document.getElementById('cancel-button');
    const selectionCheckbox = document.getElementById('include-selection') as HTMLInputElement | null;

    if (textarea) {
        textarea.addEventListener('keydown', (event) => {
            if (event.key === 'Enter' && (event.ctrlKey || event.metaKey)) {
                event.preventDefault();
                sendMessage(vscode, state, textarea);
            }
        });

        textarea.addEventListener('input', () => {
            state.inputText = textarea.value;
            autoResize(textarea);
        });
    }

    if (sendButton && textarea) {
        sendButton.addEventListener('click', () => {
            sendMessage(vscode, state, textarea);
        });
    }

    if (cancelButton) {
        cancelButton.addEventListener('click', () => {
            vscode.postMessage({ type: 'cancel' });
        });
    }

    if (selectionCheckbox) {
        selectionCheckbox.checked = state.includeSelection;
        selectionCheckbox.addEventListener('change', () => {
            state.includeSelection = selectionCheckbox.checked;
        });
    }
}

function sendMessage(vscode: VsCodeApi, state: ChatState, textarea: HTMLTextAreaElement): void {
    const text = textarea.value.trim();
    if (!text) return;

    vscode.postMessage({
        type: 'sendMessage',
        text,
        includeSelection: state.includeSelection,
    });

    textarea.value = '';
    state.inputText = '';
    autoResize(textarea);
}

function autoResize(textarea: HTMLTextAreaElement): void {
    textarea.style.height = 'auto';
    textarea.style.height = `${Math.min(textarea.scrollHeight, 200)}px`;
}
