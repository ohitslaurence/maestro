// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

/**
 * Header component for the chat panel.
 */

import type { ChatState } from '../state/chatState';

interface VsCodeApi {
    postMessage: (message: unknown) => void;
}

export function setupHeaderHandler(vscode: VsCodeApi, _state: ChatState): void {
    const newSessionButton = document.getElementById('new-session-button');
    const sessionSelector = document.getElementById('session-selector');

    if (newSessionButton) {
        newSessionButton.addEventListener('click', () => {
            vscode.postMessage({ type: 'newSession' });
        });
    }

    if (sessionSelector) {
        sessionSelector.addEventListener('change', (event) => {
            const target = event.target as HTMLSelectElement;
            const sessionId = target.value;
            if (sessionId) {
                vscode.postMessage({ type: 'switchSession', sessionId });
            }
        });
    }
}
