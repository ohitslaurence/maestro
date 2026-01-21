// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

/**
 * Loom VS Code Extension - Main Entry Point
 *
 * This extension provides AI-powered coding assistance through the Agent Client Protocol (ACP).
 * It spawns a `loom acp-agent` subprocess and communicates via stdio JSON-RPC.
 */

import * as vscode from 'vscode';
import { Logger } from './logging';
import { ConfigService } from './config/configService';
import { WorkspaceService } from './workspace/workspaceService';
import { AgentProcessManager } from './acp/agentProcessManager';
import { AcpClient } from './acp/acpClient';
import { SessionManager } from './sessions/sessionManager';
import { ChatController } from './chat/chatController';
import { LoomChatViewProvider } from './chat/chatViewProvider';

// Module-level singletons
let logger: Logger;
let configService: ConfigService;
let workspaceService: WorkspaceService;
let agentProcessManager: AgentProcessManager;
let acpClient: AcpClient;
let sessionManager: SessionManager;
let chatController: ChatController;
let chatViewProvider: LoomChatViewProvider;

/**
 * Extension activation entry point.
 * Called when the extension is first activated via activation events.
 */
export async function activate(context: vscode.ExtensionContext): Promise<void> {
  // Create logger first for all subsequent logging
  logger = new Logger('Loom');
  logger.info('Activating Loom extension...', {
    extensionPath: context.extensionPath,
    extensionMode: vscode.ExtensionMode[context.extensionMode],
  });

  try {
    // Initialize services in dependency order
    configService = new ConfigService();
    logger.debug('ConfigService initialized', {
      loomPath: configService.loomPath,
      logLevel: configService.logLevel,
    });

    workspaceService = new WorkspaceService();
    logger.debug('WorkspaceService initialized', {
      workspaceRoot: workspaceService.getWorkspaceRoot(),
    });

    // Initialize ACP components
    agentProcessManager = new AgentProcessManager(configService, workspaceService, logger);
    logger.debug('AgentProcessManager initialized');

    acpClient = new AcpClient(agentProcessManager, logger);
    logger.debug('AcpClient initialized');

    // Initialize session and chat components
    sessionManager = new SessionManager(context.workspaceState, acpClient, logger);
    logger.debug('SessionManager initialized', {
      sessionCount: sessionManager.getAllSessions().length,
      activeSessionId: sessionManager.getActiveSession()?.id,
    });

    chatController = new ChatController(acpClient, sessionManager, workspaceService, logger);
    logger.debug('ChatController initialized');

    // Initialize webview provider
    chatViewProvider = new LoomChatViewProvider(
      context.extensionUri,
      chatController,
      sessionManager,
      acpClient,
      logger
    );
    logger.debug('LoomChatViewProvider initialized');

    // Register webview provider
    context.subscriptions.push(
      vscode.window.registerWebviewViewProvider(
        LoomChatViewProvider.viewType,
        chatViewProvider
      )
    );
    logger.debug('Webview provider registered', { viewType: LoomChatViewProvider.viewType });

    // Register commands
    registerCommands(context);

    // Set initial context
    await vscode.commands.executeCommand('setContext', 'loom.isProcessing', false);

    // Auto-start the agent if configured
    if (configService.autoStart) {
      acpClient.ensureStarted().catch((error) => {
        logger.error('Failed to auto-start Loom agent', {
          error: error instanceof Error ? error.message : String(error),
          stack: error instanceof Error ? error.stack : undefined,
        });
      });
    }

    // Watch configuration changes
    context.subscriptions.push(
      vscode.workspace.onDidChangeConfiguration((e) => {
        if (e.affectsConfiguration('loom')) {
          logger.info('Configuration changed, reloading...');
          configService.reload();
          logger.setLevel(configService.logLevel);
        }
      })
    );

    // Add dispose handler for cleanup
    context.subscriptions.push({
      dispose: () => {
        logger.info('Disposing extension resources...');
        agentProcessManager?.stop();
      },
    });

    logger.info('Loom extension activated successfully');
  } catch (error) {
    logger.error('Failed to activate Loom extension', {
      error: error instanceof Error ? error.message : String(error),
      stack: error instanceof Error ? error.stack : undefined,
    });
    throw error;
  }
}

/**
 * Register all extension commands.
 */
function registerCommands(context: vscode.ExtensionContext): void {
  // Open chat panel
  context.subscriptions.push(
    vscode.commands.registerCommand('loom.openChat', () => {
      logger.debug('Command: loom.openChat');
      vscode.commands.executeCommand('loom.chatView.focus');
    })
  );

  // Create new session
  context.subscriptions.push(
    vscode.commands.registerCommand('loom.newSession', async () => {
      logger.debug('Command: loom.newSession');
      try {
        await chatController.newSession();
        logger.info('New session created via command');
      } catch (error) {
        logger.error('Failed to create new session', { error });
        vscode.window.showErrorMessage('Failed to create new session');
      }
    })
  );

  // Cancel current turn
  context.subscriptions.push(
    vscode.commands.registerCommand('loom.cancelCurrentTurn', async () => {
      logger.debug('Command: loom.cancelCurrentTurn');
      try {
        await chatController.cancelCurrentTurn();
        logger.info('Current turn cancelled via command');
      } catch (error) {
        logger.error('Failed to cancel current turn', { error });
      }
    })
  );

  // Restart agent
  context.subscriptions.push(
    vscode.commands.registerCommand('loom.restartAgent', async () => {
      logger.debug('Command: loom.restartAgent');
      try {
        await agentProcessManager.restart();
        vscode.window.showInformationMessage('Loom agent restarted');
        logger.info('Agent restarted via command');
      } catch (error) {
        logger.error('Failed to restart agent', { error });
        vscode.window.showErrorMessage('Failed to restart Loom agent');
      }
    })
  );

  // Show logs
  context.subscriptions.push(
    vscode.commands.registerCommand('loom.showLogs', () => {
      logger.debug('Command: loom.showLogs');
      logger.show();
    })
  );

  // Explain selection
  context.subscriptions.push(
    vscode.commands.registerCommand('loom.explainSelection', async () => {
      logger.debug('Command: loom.explainSelection');
      const selection = workspaceService.getActiveSelection();
      if (!selection) {
        vscode.window.showWarningMessage('No text selected');
        return;
      }

      logger.info('Explaining selection', {
        filePath: selection.filePath,
        lines: `${selection.startLine}-${selection.endLine}`,
        languageId: selection.languageId,
      });

      const prompt = `Explain this code:\n\n\`\`\`${selection.languageId}\n${selection.text}\n\`\`\``;
      await chatController.handleUserMessage(prompt, { includeSelection: false });
      vscode.commands.executeCommand('loom.chatView.focus');
    })
  );

  // Refactor selection
  context.subscriptions.push(
    vscode.commands.registerCommand('loom.refactorSelection', async () => {
      logger.debug('Command: loom.refactorSelection');
      const selection = workspaceService.getActiveSelection();
      if (!selection) {
        vscode.window.showWarningMessage('No text selected');
        return;
      }

      logger.info('Refactoring selection', {
        filePath: selection.filePath,
        lines: `${selection.startLine}-${selection.endLine}`,
        languageId: selection.languageId,
      });

      const prompt = `Refactor this code to improve readability, maintainability, and follow best practices:\n\n\`\`\`${selection.languageId}\n${selection.text}\n\`\`\``;
      await chatController.handleUserMessage(prompt, { includeSelection: false });
      vscode.commands.executeCommand('loom.chatView.focus');
    })
  );

  logger.debug('All commands registered');
}

/**
 * Extension deactivation entry point.
 * Called when the extension is deactivated.
 */
export function deactivate(): void {
  logger?.info('Deactivating Loom extension...');
  agentProcessManager?.stop();
  logger?.info('Loom extension deactivated');
}
