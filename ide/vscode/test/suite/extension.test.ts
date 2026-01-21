// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

import * as assert from 'assert';
import * as vscode from 'vscode';

suite('Extension Test Suite', () => {
    vscode.window.showInformationMessage('Starting extension tests.');

    test('Extension should be present', () => {
        const extension = vscode.extensions.getExtension('ghuntley.loom-vscode');
        assert.ok(extension, 'Extension should be installed');
    });

    test('Extension should have correct display name', () => {
        const extension = vscode.extensions.getExtension('ghuntley.loom-vscode');
        assert.strictEqual(
            extension?.packageJSON.displayName,
            'Loom AI Coding Assistant',
            'Display name should match'
        );
    });

    test('All commands should be registered', async () => {
        const commands = await vscode.commands.getCommands(true);
        
        const expectedCommands = [
            'loom.openChat',
            'loom.newSession',
            'loom.cancelCurrentTurn',
            'loom.restartAgent',
            'loom.showLogs',
            'loom.explainSelection',
            'loom.refactorSelection',
        ];

        for (const cmd of expectedCommands) {
            assert.ok(
                commands.includes(cmd),
                `Command ${cmd} should be registered`
            );
        }
    });

    test('Configuration should have all expected properties', () => {
        const config = vscode.workspace.getConfiguration('loom');
        
        const expectedProperties = [
            'loomPath',
            'additionalArgs',
            'logLevel',
            'autoStart',
            'serverUrl',
        ];

        for (const prop of expectedProperties) {
            const value = config.inspect(prop);
            assert.ok(
                value !== undefined,
                `Configuration property loom.${prop} should exist`
            );
        }
    });

    test('Default configuration values should be correct', () => {
        const config = vscode.workspace.getConfiguration('loom');
        
        assert.strictEqual(
            config.get('loomPath'),
            '',
            'Default loomPath should be empty string'
        );
        
        assert.deepStrictEqual(
            config.get('additionalArgs'),
            [],
            'Default additionalArgs should be empty array'
        );
        
        assert.strictEqual(
            config.get('logLevel'),
            'info',
            'Default logLevel should be info'
        );
        
        assert.strictEqual(
            config.get('autoStart'),
            true,
            'Default autoStart should be true'
        );
    });

    test('Chat view should be contributable', () => {
        const extension = vscode.extensions.getExtension('ghuntley.loom-vscode');
        const contributes = extension?.packageJSON.contributes;
        
        assert.ok(contributes?.views?.loom, 'Should have loom views container');
        
        const chatView = contributes?.views?.loom?.find(
            (v: { id: string }) => v.id === 'loom.chatView'
        );
        assert.ok(chatView, 'Should have loom.chatView');
        assert.strictEqual(chatView?.type, 'webview', 'Chat view should be webview type');
    });
});
