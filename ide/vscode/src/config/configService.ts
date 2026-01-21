// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

import * as vscode from 'vscode';

export interface LoomConfig {
    loomPath: string;
    additionalArgs: string[];
    logLevel: string;
    autoStart: boolean;
    serverUrl: string | undefined;
}

export class ConfigService {
    private config: vscode.WorkspaceConfiguration;

    constructor() {
        this.config = vscode.workspace.getConfiguration('loom');
    }

    get loomPath(): string {
        return this.config.get<string>('loomPath', 'loom');
    }

    get additionalArgs(): string[] {
        return this.config.get<string[]>('additionalArgs', []);
    }

    get logLevel(): string {
        return this.config.get<string>('logLevel', 'info');
    }

    get autoStart(): boolean {
        return this.config.get<boolean>('autoStart', false);
    }

    get serverUrl(): string | undefined {
        return this.config.get<string>('serverUrl');
    }

    reload(): void {
        this.config = vscode.workspace.getConfiguration('loom');
    }

    getLoomCommand(): string[] {
        const args = ['acp-agent', ...this.additionalArgs];
        const serverUrl = this.serverUrl;
        if (serverUrl) {
            args.push('--server-url', serverUrl);
        }
        return args;
    }
}
