/**
 * JSON logging utility (§7)
 * Logs to stderr in JSON format for daemon capture.
 */

type LogLevel = 'debug' | 'info' | 'warn' | 'error';

interface LogEntry {
  level: LogLevel;
  ts: number;
  msg: string;
  [key: string]: unknown;
}

function log(level: LogLevel, msg: string, data?: Record<string, unknown>): void {
  const entry: LogEntry = {
    level,
    ts: Date.now(),
    msg,
    ...data,
  };
  console.error(JSON.stringify(entry));
}

export const logger = {
  debug: (msg: string, data?: Record<string, unknown>) => log('debug', msg, data),
  info: (msg: string, data?: Record<string, unknown>) => log('info', msg, data),
  warn: (msg: string, data?: Record<string, unknown>) => log('warn', msg, data),
  error: (msg: string, data?: Record<string, unknown>) => log('error', msg, data),
};
