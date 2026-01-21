// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

import { EventEmitter } from 'events';
import { Readable, Writable } from 'stream';
import {
    ClientSideConnection,
    ndJsonStream,
    PROTOCOL_VERSION,
    type Client,
    type Agent,
    type SessionNotification,
    type InitializeResponse,
    type NewSessionResponse,
    type PromptResponse,
    type RequestPermissionRequest,
    type RequestPermissionResponse,
} from '@agentclientprotocol/sdk';
import { Logger } from '../logging';
import { AcpError, AgentCapabilities, ContentBlock, SessionId } from './types';

interface ProcessManager extends EventEmitter {
    start(): Promise<void>;
    getStdin(): NodeJS.WritableStream | null;
    getStdout(): NodeJS.ReadableStream | null;
    isActive(): boolean;
}

export class AcpClient extends EventEmitter {
    private connection: ClientSideConnection | null = null;
    private processManager: ProcessManager;
    private logger: Logger;
    private _isInitialized: boolean = false;
    private _agentCapabilities: AgentCapabilities | null = null;

    constructor(processManager: ProcessManager, logger: Logger) {
        super();
        this.processManager = processManager;
        this.logger = logger;

        this.setupProcessManagerEvents();
    }

    get isInitialized(): boolean {
        return this._isInitialized;
    }

    get agentCapabilities(): AgentCapabilities | null {
        return this._agentCapabilities;
    }

    private setupProcessManagerEvents(): void {
        this.processManager.on('ready', () => {
            this.logger.debug('Process manager ready, setting up connection', {
                component: 'AcpClient',
            });
            this.setupConnection();
        });

        this.processManager.on('exit', (code: number | null) => {
            this.logger.info('Process exited, marking disconnected', {
                component: 'AcpClient',
                exitCode: code,
            });
            this._isInitialized = false;
            this.connection = null;
            this.emit('disconnected');
        });

        this.processManager.on('error', (error: Error) => {
            this.logger.error('Process manager error', {
                component: 'AcpClient',
                error: error.message,
            });
            const acpError: AcpError = {
                code: 'PROCESS_ERROR',
                message: error.message,
                cause: error,
            };
            this.emit('error', acpError);
        });
    }

    private setupConnection(): void {
        const stdin = this.processManager.getStdin();
        const stdout = this.processManager.getStdout();
        if (!stdin || !stdout) {
            this.logger.error('Failed to get stdio streams from process manager', {
                component: 'AcpClient',
            });
            return;
        }

        // Convert Node.js streams to web streams for the ACP SDK
        const input = Writable.toWeb(stdin as Writable);
        const output = Readable.toWeb(stdout as Readable);
        const stream = ndJsonStream(input, output);

        // Create the client handler that processes incoming agent requests
        const clientHandler: Client = {
            requestPermission: async (params: RequestPermissionRequest): Promise<RequestPermissionResponse> => {
                this.logger.debug('Permission requested', {
                    component: 'AcpClient',
                    toolCallId: params.toolCall.toolCallId,
                    title: params.toolCall.title,
                });
                // For now, auto-approve with the first "allow" option
                // TODO: Implement proper permission UI in the webview
                const allowOption = params.options.find(opt => 
                    opt.kind === 'allow_once' || opt.kind === 'allow_always'
                );
                if (allowOption) {
                    return {
                        outcome: {
                            outcome: 'selected',
                            optionId: allowOption.optionId,
                        },
                    };
                }
                // If no allow option, select the first option
                return {
                    outcome: {
                        outcome: 'selected',
                        optionId: params.options[0].optionId,
                    },
                };
            },
            sessionUpdate: async (notification: SessionNotification): Promise<void> => {
                const update = notification.update;
                this.logger.debug('Received session update', {
                    component: 'AcpClient',
                    sessionId: notification.sessionId,
                    updateType: update.sessionUpdate,
                });
                this.emit('sessionUpdate', notification);
            },
        };

        this.connection = new ClientSideConnection((_agent: Agent) => clientHandler, stream);

        this.logger.debug('ClientSideConnection established', {
            component: 'AcpClient',
        });
    }

    async ensureStarted(): Promise<void> {
        this.logger.debug('Ensuring ACP client is started', {
            component: 'AcpClient',
            isActive: this.processManager.isActive(),
            isInitialized: this._isInitialized,
        });

        if (!this.processManager.isActive()) {
            await this.processManager.start();
        }

        if (!this._isInitialized) {
            await this.initialize();
        }
    }

    async initialize(): Promise<InitializeResponse> {
        if (!this.connection) {
            throw new Error('Connection not established');
        }

        this.logger.info('Sending initialize request', {
            component: 'AcpClient',
            protocolVersion: PROTOCOL_VERSION,
            clientName: 'loom-vscode',
            clientVersion: '1.0.0',
        });

        const response = await this.connection.initialize({
            protocolVersion: PROTOCOL_VERSION,
            clientInfo: {
                name: 'loom-vscode',
                version: '1.0.0',
            },
            clientCapabilities: {
                fs: {
                    readTextFile: false,
                    writeTextFile: false,
                },
            },
        });

        this._isInitialized = true;
        this._agentCapabilities = response.agentCapabilities ?? null;

        this.logger.info('Initialize response received', {
            component: 'AcpClient',
            protocolVersion: response.protocolVersion,
            agentName: response.agentInfo?.name,
            agentVersion: response.agentInfo?.version,
            capabilities: response.agentCapabilities,
        });

        this.emit('initialized', response.agentCapabilities);

        return response;
    }

    async newSession(cwd: string): Promise<SessionId> {
        await this.ensureStarted();
        if (!this.connection) {
            throw new Error('Connection not established');
        }

        this.logger.debug('Creating new session', {
            component: 'AcpClient',
            cwd,
        });

        const response: NewSessionResponse = await this.connection.newSession({
            cwd,
            mcpServers: [],
        });
        const sessionId = response.sessionId;

        this.logger.info('New session created', {
            component: 'AcpClient',
            cwd,
            sessionId,
        });

        return sessionId;
    }

    async loadSession(sessionId: string, cwd: string): Promise<void> {
        await this.ensureStarted();
        if (!this.connection) {
            throw new Error('Connection not established');
        }

        this.logger.debug('Loading session', {
            component: 'AcpClient',
            sessionId,
        });

        try {
            await this.connection.loadSession({
                sessionId,
                cwd,
                mcpServers: [],
            });

            this.logger.info('Session loaded successfully', {
                component: 'AcpClient',
                sessionId,
                success: true,
            });
        } catch (error) {
            this.logger.error('Failed to load session', {
                component: 'AcpClient',
                sessionId,
                success: false,
                error: error instanceof Error ? error.message : String(error),
            });
            throw error;
        }
    }

    async prompt(sessionId: string, content: ContentBlock[]): Promise<PromptResponse> {
        await this.ensureStarted();
        if (!this.connection) {
            throw new Error('Connection not established');
        }

        this.logger.debug('Sending prompt', {
            component: 'AcpClient',
            sessionId,
            contentBlockCount: content.length,
        });

        const response = await this.connection.prompt({
            sessionId,
            prompt: content,
        });

        this.logger.info('Prompt response received', {
            component: 'AcpClient',
            sessionId,
            contentBlockCount: content.length,
            stopReason: response.stopReason,
        });

        return response;
    }

    async cancel(sessionId: string): Promise<void> {
        await this.ensureStarted();
        if (!this.connection) {
            throw new Error('Connection not established');
        }

        this.logger.info('Cancelling session', {
            component: 'AcpClient',
            sessionId,
        });

        await this.connection.cancel({ sessionId });

        this.logger.debug('Session cancelled', {
            component: 'AcpClient',
            sessionId,
        });
    }
}

// Re-export types for consumers
export type { SessionNotification };
