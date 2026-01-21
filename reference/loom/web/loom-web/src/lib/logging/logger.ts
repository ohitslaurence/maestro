/**
 * Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
 * SPDX-License-Identifier: Proprietary
 */

export type LogLevel = 'debug' | 'info' | 'warn' | 'error';

export interface LogContext {
	threadId?: string;
	sessionId?: string;
	agentState?: string;
	component?: string;
	event?: string;
	duration?: number;
	[key: string]: unknown;
}

interface LogEntry {
	ts: string;
	level: LogLevel;
	message: string;
	context: LogContext;
}

const LOG_LEVELS: Record<LogLevel, number> = {
	debug: 0,
	info: 1,
	warn: 2,
	error: 3,
};

function getMinLogLevel(): LogLevel {
	if (typeof window === 'undefined') return 'info';

	const envLevel = (import.meta.env?.VITE_LOG_LEVEL as LogLevel) || 'info';
	return envLevel;
}

function shouldLog(level: LogLevel): boolean {
	const minLevel = getMinLogLevel();
	return LOG_LEVELS[level] >= LOG_LEVELS[minLevel];
}

function createLogEntry(level: LogLevel, message: string, context: LogContext): LogEntry {
	return {
		ts: new Date().toISOString(),
		level,
		message,
		context: sanitizeContext(context),
	};
}

function sanitizeContext(context: LogContext): LogContext {
	const sanitized: LogContext = {};

	for (const [key, value] of Object.entries(context)) {
		if (value === undefined || value === null) continue;

		// Don't log potentially sensitive data
		if (key.toLowerCase().includes('password') || key.toLowerCase().includes('secret')) {
			sanitized[key] = '[REDACTED]';
		} else if (typeof value === 'object') {
			try {
				// Ensure it's serializable
				JSON.stringify(value);
				sanitized[key] = value;
			} catch {
				sanitized[key] = '[Unserializable]';
			}
		} else {
			sanitized[key] = value;
		}
	}

	return sanitized;
}

function formatDevLog(entry: LogEntry): string {
	const contextStr =
		Object.keys(entry.context).length > 0 ? ` ${JSON.stringify(entry.context)}` : '';
	return `[${entry.ts}] ${entry.level.toUpperCase()}: ${entry.message}${contextStr}`;
}

export function log(level: LogLevel, message: string, context: LogContext = {}): void {
	if (!shouldLog(level)) return;

	const entry = createLogEntry(level, message, context);

	const isDev = import.meta.env?.DEV ?? true;

	if (isDev) {
		const formatted = formatDevLog(entry);
		switch (level) {
			case 'debug':
				console.log(formatted);
				break;
			case 'info':
				console.info(formatted);
				break;
			case 'warn':
				console.warn(formatted);
				break;
			case 'error':
				console.error(formatted);
				break;
		}
	} else {
		// Production: structured JSON logging
		const method = level === 'debug' ? 'log' : level;
		console[method](JSON.stringify(entry));
	}
}

export const logger = {
	debug: (message: string, context?: LogContext) => log('debug', message, context),
	info: (message: string, context?: LogContext) => log('info', message, context),
	warn: (message: string, context?: LogContext) => log('warn', message, context),
	error: (message: string, context?: LogContext) => log('error', message, context),

	// Scoped logger with pre-filled context
	withContext: (baseContext: LogContext) => ({
		debug: (message: string, context?: LogContext) =>
			log('debug', message, { ...baseContext, ...context }),
		info: (message: string, context?: LogContext) =>
			log('info', message, { ...baseContext, ...context }),
		warn: (message: string, context?: LogContext) =>
			log('warn', message, { ...baseContext, ...context }),
		error: (message: string, context?: LogContext) =>
			log('error', message, { ...baseContext, ...context }),
	}),
};

// Performance timing helper
export function withTiming<T>(label: string, fn: () => T, context: LogContext = {}): T {
	const start = performance.now();
	try {
		const result = fn();
		const duration = performance.now() - start;
		logger.debug(`${label} completed`, { ...context, duration: Math.round(duration) });
		return result;
	} catch (error) {
		const duration = performance.now() - start;
		logger.error(`${label} failed`, {
			...context,
			duration: Math.round(duration),
			error: String(error),
		});
		throw error;
	}
}

// Async timing helper
export async function withTimingAsync<T>(
	label: string,
	fn: () => Promise<T>,
	context: LogContext = {}
): Promise<T> {
	const start = performance.now();
	try {
		const result = await fn();
		const duration = performance.now() - start;
		logger.debug(`${label} completed`, { ...context, duration: Math.round(duration) });
		return result;
	} catch (error) {
		const duration = performance.now() - start;
		logger.error(`${label} failed`, {
			...context,
			duration: Math.round(duration),
			error: String(error),
		});
		throw error;
	}
}
