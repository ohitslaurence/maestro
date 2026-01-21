// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

import * as vscode from 'vscode';
import { Logger } from '../logging';
import { ChatController } from './chatController';
import { SessionManager, LoomSession } from '../sessions/sessionManager';
import { AcpClient } from '../acp/acpClient';
import { ChatMessage, ToolCallStatus, ExtensionToWebviewMessage, WebviewToExtensionMessage, StopReason } from './models';

export class LoomChatViewProvider implements vscode.WebviewViewProvider {
  public static readonly viewType = 'loom.chatView';

  private view?: vscode.WebviewView;

  constructor(
    private readonly extensionUri: vscode.Uri,
    private readonly chatController: ChatController,
    private readonly sessionManager: SessionManager,
    private readonly acpClient: AcpClient,
    private readonly logger: Logger
  ) {
    this.setupEventHandlers();
  }

  public resolveWebviewView(
    webviewView: vscode.WebviewView,
    _context: vscode.WebviewViewResolveContext,
    _token: vscode.CancellationToken
  ): void {
    this.view = webviewView;

    webviewView.webview.options = {
      enableScripts: true,
      localResourceRoots: [vscode.Uri.joinPath(this.extensionUri, 'media')],
    };

    webviewView.webview.html = this.getHtmlContent(webviewView.webview);

    webviewView.webview.onDidReceiveMessage((message: WebviewToExtensionMessage) => {
      this.handleWebviewMessage(message);
    });

    webviewView.onDidDispose(() => {
      this.view = undefined;
    });
  }

  public postMessage(message: ExtensionToWebviewMessage): void {
    this.view?.webview.postMessage(message);
  }

  private setupEventHandlers(): void {
    // Chat controller events
    this.chatController.on('messageAdded', (message: ChatMessage) => {
      this.logger.debug('Event: messageAdded', { messageId: message.id, role: message.role });
      this.postMessage({ type: 'messageAdded', message });
    });

    this.chatController.on('messageUpdated', (data: { messageId: string; content: Partial<ChatMessage> }) => {
      this.logger.debug('Event: messageUpdated', { messageId: data.messageId });
      this.postMessage({ type: 'messageUpdated', messageId: data.messageId, content: data.content });
    });

    this.chatController.on('streamingChunk', (data: { messageId: string; text: string }) => {
      this.postMessage({ type: 'streamingChunk', messageId: data.messageId, text: data.text });
    });

    this.chatController.on('toolCallUpdate', (data: { messageId: string; toolCall: ToolCallStatus }) => {
      this.logger.debug('Event: toolCallUpdate', { 
        messageId: data.messageId, 
        toolName: data.toolCall.toolName,
        status: data.toolCall.status 
      });
      this.postMessage({ type: 'toolCallUpdate', messageId: data.messageId, toolCall: data.toolCall });
    });

    this.chatController.on('turnCompleted', (data: { messageId: string; stopReason: StopReason }) => {
      this.logger.debug('Event: turnCompleted', { messageId: data.messageId, stopReason: data.stopReason });
      this.postMessage({ type: 'turnCompleted', messageId: data.messageId, stopReason: data.stopReason });
    });

    this.chatController.on('sessionChanged', (session: LoomSession) => {
      this.logger.debug('Event: sessionChanged', { sessionId: session.id });
      this.postMessage({ type: 'sessionChanged', session });
    });

    this.chatController.on('conversationHistory', (messages: ChatMessage[]) => {
      this.logger.debug('Event: conversationHistory', { messageCount: messages.length });
      this.postMessage({ type: 'conversationHistory', messages });
    });

    this.chatController.on('error', (error: string) => {
      this.logger.error('Chat controller error', { error });
      this.postMessage({ type: 'error', error });
    });

    // Session manager events
    this.sessionManager.on('sessionCreated', () => {
      this.postMessage({
        type: 'sessionsUpdated',
        sessions: this.sessionManager.getAllSessions(),
      });
    });

    this.sessionManager.on('activeSessionChanged', (session: LoomSession | undefined) => {
      if (session) {
        this.postMessage({ type: 'sessionChanged', session });
      }
    });

    this.sessionManager.on('sessionUpdated', () => {
      this.postMessage({
        type: 'sessionsUpdated',
        sessions: this.sessionManager.getAllSessions(),
      });
    });

    this.sessionManager.on('sessionDeleted', () => {
      this.postMessage({
        type: 'sessionsUpdated',
        sessions: this.sessionManager.getAllSessions(),
      });
    });

    // ACP client events
    this.acpClient.on('initialized', () => {
      this.logger.debug('Event: ACP initialized');
      this.postMessage({ type: 'connectionStatus', status: 'connected' });
    });

    this.acpClient.on('disconnected', () => {
      this.logger.debug('Event: ACP disconnected');
      this.postMessage({ type: 'connectionStatus', status: 'disconnected' });
    });

    this.acpClient.on('error', (error: { message: string }) => {
      this.logger.error('ACP client error', { error: error.message });
      this.postMessage({ type: 'connectionStatus', status: 'error', error: error.message });
    });
  }

  private handleWebviewMessage(message: WebviewToExtensionMessage): void {
    this.logger.debug('Webview message received', { type: message.type });

    switch (message.type) {
      case 'ready':
        this.sendInitialState();
        break;

      case 'sendMessage':
        this.chatController.handleUserMessage(message.text, {
          includeSelection: message.includeSelection,
        }).catch((error) => {
          this.logger.error('Failed to send message', { error });
        });
        break;

      case 'cancel':
        this.chatController.cancelCurrentTurn().catch((error) => {
          this.logger.error('Failed to cancel', { error });
        });
        break;

      case 'newSession':
        this.chatController.newSession().catch((error) => {
          this.logger.error('Failed to create session', { error });
        });
        break;

      case 'switchSession':
        this.chatController.switchSession(message.sessionId).catch((error) => {
          this.logger.error('Failed to switch session', { error });
        });
        break;

      case 'deleteSession':
        this.sessionManager.deleteSession(message.sessionId);
        break;

      case 'copyCode':
        vscode.env.clipboard.writeText(message.code);
        vscode.window.showInformationMessage('Code copied to clipboard');
        break;

      case 'insertCode':
        this.insertCodeAtCursor(message.code);
        break;

      case 'openFile':
        this.openFile(message.path, message.line);
        break;
    }
  }

  private sendInitialState(): void {
    this.logger.debug('Sending initial state to webview');

    // Connection status
    this.postMessage({
      type: 'connectionStatus',
      status: this.acpClient.isInitialized ? 'connected' : 'connecting',
    });

    // Sessions
    this.postMessage({
      type: 'sessionsUpdated',
      sessions: this.sessionManager.getAllSessions(),
    });

    // Active session
    const activeSession = this.sessionManager.getActiveSession();
    if (activeSession) {
      this.postMessage({ type: 'sessionChanged', session: activeSession });
      this.postMessage({
        type: 'conversationHistory',
        messages: this.chatController.getConversationHistory(activeSession.id),
      });
    }
  }

  private insertCodeAtCursor(code: string): void {
    const editor = vscode.window.activeTextEditor;
    if (editor) {
      editor.edit((editBuilder) => {
        editBuilder.insert(editor.selection.active, code);
      });
    } else {
      vscode.window.showWarningMessage('No active editor to insert code');
    }
  }

  private openFile(filePath: string, line?: number): void {
    const uri = vscode.Uri.file(filePath);
    vscode.workspace.openTextDocument(uri).then((doc) => {
      vscode.window.showTextDocument(doc).then((editor) => {
        if (line !== undefined && line > 0) {
          const position = new vscode.Position(line - 1, 0);
          editor.selection = new vscode.Selection(position, position);
          editor.revealRange(
            new vscode.Range(position, position),
            vscode.TextEditorRevealType.InCenter
          );
        }
      });
    });
  }

  private getHtmlContent(webview: vscode.Webview): string {
    const styleUri = webview.asWebviewUri(
      vscode.Uri.joinPath(this.extensionUri, 'media', 'chat.css')
    );
    const scriptUri = webview.asWebviewUri(
      vscode.Uri.joinPath(this.extensionUri, 'media', 'chat.js')
    );
    const nonce = this.getNonce();

    return `<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <meta http-equiv="Content-Security-Policy" content="default-src 'none'; style-src ${webview.cspSource} 'unsafe-inline'; script-src 'nonce-${nonce}';">
  <link href="${styleUri}" rel="stylesheet">
  <title>Loom Chat</title>
</head>
<body>
  <div id="app">
    <header id="header">
      <div class="connection-status">
        <span id="status-indicator" class="status-dot"></span>
        <span id="status-text">Connecting...</span>
      </div>
      <div class="session-controls">
        <select id="session-select" title="Select session">
          <option value="">No session</option>
        </select>
        <button id="new-session-btn" title="New Session">+</button>
      </div>
    </header>

    <main id="messages-container">
      <div id="messages"></div>
    </main>

    <div id="context-strip">
      <label>
        <input type="checkbox" id="include-selection" checked>
        Include current selection
      </label>
    </div>

    <footer id="input-container">
      <textarea 
        id="message-input" 
        placeholder="Type your message... (Ctrl+Enter to send)"
        rows="3"
      ></textarea>
      <div class="input-actions">
        <button id="send-btn" title="Send (Ctrl+Enter)">Send</button>
        <button id="cancel-btn" title="Cancel" disabled>Cancel</button>
      </div>
    </footer>
  </div>

  <script nonce="${nonce}" src="${scriptUri}"></script>
</body>
</html>`;
  }

  private getNonce(): string {
    let text = '';
    const possible = 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789';
    for (let i = 0; i < 32; i++) {
      text += possible.charAt(Math.floor(Math.random() * possible.length));
    }
    return text;
  }
}
