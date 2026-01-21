// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

import * as vscode from 'vscode';
import { EventEmitter } from 'events';
import { AcpClient, SessionNotification } from '../acp/acpClient';
import {
    isAgentMessageChunk,
    isToolCall,
    isToolCallUpdate,
    ContentBlock,
    TextBlock,
} from '../acp/types';
import { Logger } from '../logging';
import {
    ChatMessage,
    ToolCallStatus,
    MessageContext,
    generateMessageId,
    StopReason,
} from './models';

interface SessionManager {
    getActiveSession(): { id: string; title: string } | undefined;
    createNewSession(cwd: string): Promise<{ id: string; title: string }>;
    setActiveSession(sessionId: string): Promise<void>;
    updateSessionTitle(sessionId: string, title: string): void;
    deriveSessionTitleFromMessage(text: string): string;
}

interface WorkspaceService {
    getWorkspaceRoot(): string | undefined;
}

export class ChatController extends EventEmitter {
    private acpClient: AcpClient;
    private sessionManager: SessionManager;
    private workspaceService: WorkspaceService;
    private logger: Logger;

    private conversationHistory: Map<string, ChatMessage[]> = new Map();
    private currentAssistantMessageId: string | null = null;
    private _isProcessing: boolean = false;
    private pendingToolCalls: Map<string, ToolCallStatus> = new Map();

    get isProcessing(): boolean {
        return this._isProcessing;
    }

    constructor(
        acpClient: AcpClient,
        sessionManager: SessionManager,
        workspaceService: WorkspaceService,
        logger: Logger
    ) {
        super();
        this.acpClient = acpClient;
        this.sessionManager = sessionManager;
        this.workspaceService = workspaceService;
        this.logger = logger;

        this.setupAcpClientEvents();
    }

    private setupAcpClientEvents(): void {
        this.acpClient.on('sessionUpdate', (notification: SessionNotification) => {
            this.handleSessionUpdate(notification);
        });
    }

    async handleUserMessage(text: string, context?: MessageContext): Promise<void> {
        let activeSession = this.sessionManager.getActiveSession();
        
        // Auto-create session if none exists
        if (!activeSession) {
            const workspaceRoot = this.workspaceService.getWorkspaceRoot();
            if (!workspaceRoot) {
                this.logger.error('No workspace root for new session', {
                    component: 'ChatController',
                });
                this.emit('error', 'No workspace folder open');
                return;
            }
            
            this.logger.info('Auto-creating session for first message', {
                component: 'ChatController',
                workspaceRoot,
            });
            
            try {
                activeSession = await this.sessionManager.createNewSession(workspaceRoot);
                this.conversationHistory.set(activeSession.id, []);
                this.emit('sessionChanged', activeSession);
            } catch (error) {
                this.logger.error('Failed to auto-create session', {
                    component: 'ChatController',
                    error: error instanceof Error ? error.message : String(error),
                });
                this.emit('error', 'Failed to create session');
                return;
            }
        }
        
        const sessionId = activeSession.id;

        this.logger.info('Handling user message', {
            component: 'ChatController',
            sessionId,
            messageLength: text.length,
            hasSelectionContext: !!context?.selectionText,
        });

        const userMessage: ChatMessage = {
            id: generateMessageId(),
            role: 'user',
            content: text,
            timestamp: Date.now(),
        };

        this.addMessageToHistory(sessionId, userMessage);
        this.emit('messageAdded', userMessage);

        this._isProcessing = true;
        await vscode.commands.executeCommand('setContext', 'loom.isProcessing', true);

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

            this.finalizeAssistantMessage(sessionId, response.stopReason as StopReason || 'end_turn');
        } catch (error) {
            this.logger.error('Error sending prompt', {
                component: 'ChatController',
                sessionId,
                error: error instanceof Error ? error.message : String(error),
            });
            this.emit('error', error instanceof Error ? error.message : String(error));
        } finally {
            this._isProcessing = false;
            await vscode.commands.executeCommand('setContext', 'loom.isProcessing', false);
        }
    }

    async cancelCurrentTurn(): Promise<void> {
        const activeSession = this.sessionManager.getActiveSession();
        if (!activeSession) {
            return;
        }
        const sessionId = activeSession.id;

        this.logger.info('Cancelling current turn', {
            component: 'ChatController',
            sessionId,
        });

        try {
            await this.acpClient.cancel(sessionId);
            this.logger.debug('Cancel request sent', {
                component: 'ChatController',
                sessionId,
            });
        } catch (error) {
            this.logger.error('Error cancelling turn', {
                component: 'ChatController',
                sessionId,
                error: error instanceof Error ? error.message : String(error),
            });
        }
    }

    async newSession(): Promise<void> {
        const workspaceRoot = this.workspaceService.getWorkspaceRoot();
        if (!workspaceRoot) {
            this.logger.error('No workspace root for new session', {
                component: 'ChatController',
            });
            this.emit('error', 'No workspace folder open');
            return;
        }

        try {
            const session = await this.sessionManager.createNewSession(workspaceRoot);
            this.conversationHistory.set(session.id, []);

            this.logger.info('New session created', {
                component: 'ChatController',
                sessionId: session.id,
            });

            this.emit('sessionChanged', session);
            this.emit('conversationHistory', []);
        } catch (error) {
            this.logger.error('Error creating new session', {
                component: 'ChatController',
                error: error instanceof Error ? error.message : String(error),
            });
            this.emit('error', error instanceof Error ? error.message : String(error));
        }
    }

    async switchSession(sessionId: string): Promise<void> {
        this.logger.info('Switching session', {
            component: 'ChatController',
            sessionId,
        });

        try {
            await this.sessionManager.setActiveSession(sessionId);

            const activeSession = this.sessionManager.getActiveSession();
            if (activeSession) {
                this.emit('sessionChanged', activeSession);
            }

            const history = this.conversationHistory.get(sessionId) || [];
            this.emit('conversationHistory', history);
        } catch (error) {
            this.logger.error('Error switching session', {
                component: 'ChatController',
                sessionId,
                error: error instanceof Error ? error.message : String(error),
            });
            this.emit('error', error instanceof Error ? error.message : String(error));
        }
    }

    getConversationHistory(sessionId?: string): ChatMessage[] {
        const id = sessionId ?? this.sessionManager.getActiveSession()?.id;
        if (!id) {
            return [];
        }
        return this.conversationHistory.get(id) || [];
    }

    private handleSessionUpdate(notification: SessionNotification): void {
        const sessionId = notification.sessionId;
        const update = notification.update;
        const activeSession = this.sessionManager.getActiveSession();
        
        // Only process updates for the active session
        if (!activeSession || activeSession.id !== sessionId) {
            this.logger.debug('Ignoring update for inactive session', {
                component: 'ChatController',
                updateSessionId: sessionId,
                activeSessionId: activeSession?.id,
            });
            return;
        }

        // Handle different update types based on the sessionUpdate discriminator
        if (isAgentMessageChunk(update)) {
            const content = update.content;
            if (content.type === 'text' && content.text) {
                this.logger.debug('Received agent message chunk', {
                    component: 'ChatController',
                    sessionId,
                    contentLength: content.text.length,
                });

                if (this.currentAssistantMessageId) {
                    this.appendToCurrentMessage(sessionId, content.text);
                    this.emit('streamingChunk', {
                        messageId: this.currentAssistantMessageId,
                        text: content.text,
                    });
                }
            }
        } else if (isToolCall(update)) {
            // Initial tool call notification
            this.logger.debug('Tool call received', {
                component: 'ChatController',
                sessionId,
                toolCallId: update.toolCallId,
                title: update.title,
                status: update.status,
            });

            const toolCallStatus: ToolCallStatus = {
                id: update.toolCallId,
                toolName: update.title,
                status: this.mapToolCallStatus(update.status),
            };
            this.pendingToolCalls.set(update.toolCallId, toolCallStatus);

            if (this.currentAssistantMessageId) {
                this.addToolCallToMessage(sessionId, this.currentAssistantMessageId, toolCallStatus);
                this.emit('toolCallUpdate', {
                    messageId: this.currentAssistantMessageId,
                    toolCall: toolCallStatus,
                });
            }
        } else if (isToolCallUpdate(update)) {
            // Tool call status update
            this.logger.debug('Tool call update', {
                component: 'ChatController',
                sessionId,
                toolCallId: update.toolCallId,
                status: update.status,
            });

            const toolCall = this.pendingToolCalls.get(update.toolCallId);
            if (toolCall) {
                toolCall.status = this.mapToolCallStatus(update.status);
                
                if (update.status === 'completed' || update.status === 'failed') {
                    this.pendingToolCalls.delete(update.toolCallId);
                }

                if (this.currentAssistantMessageId) {
                    this.updateToolCallInMessage(sessionId, this.currentAssistantMessageId, toolCall);
                    this.emit('toolCallUpdate', {
                        messageId: this.currentAssistantMessageId,
                        toolCall,
                    });
                }
            }
        }
    }

    private mapToolCallStatus(sdkStatus: string | undefined | null): 'pending' | 'running' | 'completed' | 'failed' {
        switch (sdkStatus) {
            case 'pending':
                return 'pending';
            case 'in_progress':
                return 'running';
            case 'completed':
                return 'completed';
            case 'failed':
                return 'failed';
            default:
                return 'pending';
        }
    }

    private buildContentBlocks(text: string, context?: MessageContext): ContentBlock[] {
        const blocks: ContentBlock[] = [];

        if (context?.selectionText && context.filePath) {
            const contextText = `File: ${context.filePath}${context.languageId ? ` (${context.languageId})` : ''}\nSelection:\n\`\`\`\n${context.selectionText}\n\`\`\`\n\n`;
            const contextBlock: TextBlock = {
                type: 'text',
                text: contextText,
            };
            blocks.push(contextBlock);
        }

        const textBlock: TextBlock = {
            type: 'text',
            text: text,
        };
        blocks.push(textBlock);

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

    private updateToolCallInMessage(
        sessionId: string,
        messageId: string,
        toolCall: ToolCallStatus
    ): void {
        const history = this.conversationHistory.get(sessionId);
        if (!history) {
            return;
        }

        const message = history.find((m) => m.id === messageId);
        if (message && message.toolCalls) {
            const index = message.toolCalls.findIndex((tc) => tc.id === toolCall.id);
            if (index !== -1) {
                message.toolCalls[index] = toolCall;
            }
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
