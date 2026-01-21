// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

import * as vscode from "vscode";

/**
 * Log levels in order of severity (lowest to highest).
 */
export type LogLevel = "trace" | "debug" | "info" | "warn" | "error";

/**
 * Numeric priority for each log level.
 */
const LOG_LEVEL_PRIORITY: Record<LogLevel, number> = {
  trace: 0,
  debug: 1,
  info: 2,
  warn: 3,
  error: 4,
};

/**
 * A structured logging class that wraps VS Code's OutputChannel.
 *
 * Provides leveled logging with structured data support, formatted as:
 * `[ISO_TIMESTAMP] [LEVEL] [ComponentName] Message {...data}`
 *
 * @example
 * ```typescript
 * const logger = new Logger("MyComponent");
 * logger.info("User logged in", { userId: "123", method: "oauth" });
 * // Output: [2025-01-01T12:00:00.000Z] [INFO] [MyComponent] User logged in {"userId":"123","method":"oauth"}
 * ```
 */
export class Logger {
  private readonly outputChannel: vscode.OutputChannel;
  private readonly componentName: string;
  private overrideLevel?: LogLevel;

  /**
   * Creates a new Logger instance.
   *
   * @param componentName - The name of the component for log identification.
   * @param outputChannel - Optional existing OutputChannel. If not provided,
   *                        a new channel named "Loom" will be created.
   */
  constructor(componentName: string, outputChannel?: vscode.OutputChannel) {
    this.componentName = componentName;
    this.outputChannel = outputChannel ?? vscode.window.createOutputChannel("Loom");
  }

  /**
   * Gets the currently configured log level from VS Code settings.
   *
   * @returns The configured log level, defaults to "info" if not set.
   */
  private getConfiguredLogLevel(): LogLevel {
    if (this.overrideLevel) {
      return this.overrideLevel;
    }
    const config = vscode.workspace.getConfiguration("loom");
    const level = config.get<string>("logLevel");
    if (level && level in LOG_LEVEL_PRIORITY) {
      return level as LogLevel;
    }
    return "info";
  }

  /**
   * Sets an override log level that takes precedence over configuration.
   *
   * @param level - The log level to set, or a string that will be parsed.
   */
  setLevel(level: LogLevel | string): void {
    if (level in LOG_LEVEL_PRIORITY) {
      this.overrideLevel = level as LogLevel;
    }
  }

  /**
   * Determines if a message at the given level should be logged.
   *
   * @param level - The level of the message to check.
   * @returns True if the message should be logged, false otherwise.
   */
  private shouldLog(level: LogLevel): boolean {
    const configuredLevel = this.getConfiguredLogLevel();
    return LOG_LEVEL_PRIORITY[level] >= LOG_LEVEL_PRIORITY[configuredLevel];
  }

  /**
   * Formats a log entry with timestamp, level, component, message, and optional data.
   *
   * @param level - The log level.
   * @param message - The log message.
   * @param data - Optional structured data to include.
   * @returns The formatted log string.
   */
  private formatLogEntry(level: LogLevel, message: string, data?: Record<string, unknown>): string {
    const timestamp = new Date().toISOString();
    const levelStr = level.toUpperCase().padEnd(5);
    let entry = `[${timestamp}] [${levelStr}] [${this.componentName}] ${message}`;

    if (data !== undefined && Object.keys(data).length > 0) {
      try {
        entry += ` ${JSON.stringify(data)}`;
      } catch {
        entry += ` [Error serializing data]`;
      }
    }

    return entry;
  }

  /**
   * Writes a log entry to the output channel.
   *
   * @param level - The log level.
   * @param message - The log message.
   * @param data - Optional structured data to include.
   */
  private log(level: LogLevel, message: string, data?: Record<string, unknown>): void {
    if (!this.shouldLog(level)) {
      return;
    }
    const entry = this.formatLogEntry(level, message, data);
    this.outputChannel.appendLine(entry);
  }

  /**
   * Logs an error message.
   *
   * @param message - The error message.
   * @param data - Optional structured data to include.
   */
  error(message: string, data?: Record<string, unknown>): void {
    this.log("error", message, data);
  }

  /**
   * Logs a warning message.
   *
   * @param message - The warning message.
   * @param data - Optional structured data to include.
   */
  warn(message: string, data?: Record<string, unknown>): void {
    this.log("warn", message, data);
  }

  /**
   * Logs an informational message.
   *
   * @param message - The info message.
   * @param data - Optional structured data to include.
   */
  info(message: string, data?: Record<string, unknown>): void {
    this.log("info", message, data);
  }

  /**
   * Logs a debug message.
   *
   * @param message - The debug message.
   * @param data - Optional structured data to include.
   */
  debug(message: string, data?: Record<string, unknown>): void {
    this.log("debug", message, data);
  }

  /**
   * Logs a trace message.
   *
   * @param message - The trace message.
   * @param data - Optional structured data to include.
   */
  trace(message: string, data?: Record<string, unknown>): void {
    this.log("trace", message, data);
  }

  /**
   * Shows the output channel in the VS Code UI.
   *
   * @param preserveFocus - If true, the editor will not take focus.
   */
  show(preserveFocus?: boolean): void {
    this.outputChannel.show(preserveFocus);
  }

  /**
   * Disposes of the output channel and releases resources.
   */
  dispose(): void {
    this.outputChannel.dispose();
  }
}
