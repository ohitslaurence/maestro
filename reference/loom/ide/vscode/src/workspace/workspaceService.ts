// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

import * as vscode from 'vscode';
import * as path from 'path';

export interface Selection {
    text: string;
    filePath: string;
    languageId: string;
    startLine: number;
    endLine: number;
}

export class WorkspaceService {
    getWorkspaceRoot(): string | undefined {
        const activeEditor = vscode.window.activeTextEditor;
        if (activeEditor) {
            const workspaceFolder = vscode.workspace.getWorkspaceFolder(activeEditor.document.uri);
            if (workspaceFolder) {
                return workspaceFolder.uri.fsPath;
            }
        }
        const folders = vscode.workspace.workspaceFolders;
        if (folders && folders.length > 0) {
            return folders[0].uri.fsPath;
        }
        return undefined;
    }

    getActiveFilePath(): string | undefined {
        const activeEditor = vscode.window.activeTextEditor;
        return activeEditor?.document.uri.fsPath;
    }

    getActiveSelection(): Selection | undefined {
        const activeEditor = vscode.window.activeTextEditor;
        if (!activeEditor) {
            return undefined;
        }
        const selection = activeEditor.selection;
        if (selection.isEmpty) {
            return undefined;
        }
        const document = activeEditor.document;
        return {
            text: document.getText(selection),
            filePath: document.uri.fsPath,
            languageId: document.languageId,
            startLine: selection.start.line + 1,
            endLine: selection.end.line + 1,
        };
    }

    getRelativePath(absolutePath: string): string {
        const workspaceRoot = this.getWorkspaceRoot();
        if (workspaceRoot && absolutePath.startsWith(workspaceRoot)) {
            return path.relative(workspaceRoot, absolutePath);
        }
        return absolutePath;
    }

    async openFile(filePath: string, line?: number): Promise<void> {
        const uri = vscode.Uri.file(filePath);
        const document = await vscode.workspace.openTextDocument(uri);
        const editor = await vscode.window.showTextDocument(document);
        if (line !== undefined && line > 0) {
            const position = new vscode.Position(line - 1, 0);
            editor.selection = new vscode.Selection(position, position);
            editor.revealRange(new vscode.Range(position, position), vscode.TextEditorRevealType.InCenter);
        }
    }

    async insertText(text: string): Promise<void> {
        const activeEditor = vscode.window.activeTextEditor;
        if (!activeEditor) {
            return;
        }
        await activeEditor.edit((editBuilder) => {
            editBuilder.insert(activeEditor.selection.active, text);
        });
    }
}
