/**
 * Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
 * SPDX-License-Identifier: Proprietary
 */

/**
 * Error types for the feature flags SDK.
 */

/**
 * Base error for flags SDK errors.
 */
export class FlagsError extends Error {
	constructor(message: string, options?: { cause?: unknown }) {
		super(message, { cause: options?.cause });
		this.name = 'FlagsError';
	}

	/**
	 * Returns true if this error is retryable.
	 */
	isRetryable(): boolean {
		return false;
	}

	/**
	 * Returns true if the client should use cached values for this error.
	 */
	shouldUseCache(): boolean {
		return false;
	}
}

/**
 * SDK key is missing or invalid.
 */
export class InvalidSdkKeyError extends FlagsError {
	constructor() {
		super('Invalid or missing SDK key');
		this.name = 'InvalidSdkKeyError';
	}
}

/**
 * Base URL is missing or invalid.
 */
export class InvalidBaseUrlError extends FlagsError {
	constructor() {
		super('Invalid or missing base URL');
		this.name = 'InvalidBaseUrlError';
	}
}

/**
 * Failed to connect to the server.
 */
export class ConnectionError extends FlagsError {
	constructor(message: string, options?: { cause?: unknown }) {
		super(`Failed to connect to server: ${message}`, options);
		this.name = 'ConnectionError';
	}

	override isRetryable(): boolean {
		return true;
	}

	override shouldUseCache(): boolean {
		return true;
	}
}

/**
 * SDK key authentication failed.
 */
export class AuthenticationError extends FlagsError {
	constructor() {
		super('SDK key authentication failed');
		this.name = 'AuthenticationError';
	}
}

/**
 * Rate limited error.
 */
export class RateLimitedError extends FlagsError {
	readonly retryAfterSecs?: number;

	constructor(retryAfterSecs?: number) {
		super(`Rate limited${retryAfterSecs ? `. Retry after ${retryAfterSecs} seconds` : ''}`);
		this.name = 'RateLimitedError';
		this.retryAfterSecs = retryAfterSecs;
	}

	override isRetryable(): boolean {
		return true;
	}

	override shouldUseCache(): boolean {
		return true;
	}
}

/**
 * Client initialization timed out.
 */
export class InitializationTimeoutError extends FlagsError {
	constructor() {
		super('Client initialization timed out');
		this.name = 'InitializationTimeoutError';
	}
}

/**
 * Client has been closed.
 */
export class ClientClosedError extends FlagsError {
	constructor() {
		super('Client has been closed');
		this.name = 'ClientClosedError';
	}
}

/**
 * Flag not found.
 */
export class FlagNotFoundError extends FlagsError {
	readonly flagKey: string;

	constructor(flagKey: string) {
		super(`Flag not found: ${flagKey}`);
		this.name = 'FlagNotFoundError';
		this.flagKey = flagKey;
	}
}

/**
 * Server returned an error response.
 */
export class ServerError extends FlagsError {
	readonly statusCode: number;
	readonly body?: string;

	constructor(statusCode: number, message: string, body?: string) {
		super(`Server returned an error: ${statusCode} - ${message}`);
		this.name = 'ServerError';
		this.statusCode = statusCode;
		this.body = body;
	}

	override isRetryable(): boolean {
		return this.statusCode >= 500 && this.statusCode < 600;
	}

	override shouldUseCache(): boolean {
		return this.statusCode >= 500 && this.statusCode < 600;
	}
}

/**
 * SSE connection error.
 */
export class SseConnectionError extends FlagsError {
	constructor(message: string, options?: { cause?: unknown }) {
		super(`SSE connection failed: ${message}`, options);
		this.name = 'SseConnectionError';
	}

	override isRetryable(): boolean {
		return true;
	}
}

/**
 * SSE stream error.
 */
export class SseStreamError extends FlagsError {
	constructor(message: string, options?: { cause?: unknown }) {
		super(`SSE stream error: ${message}`, options);
		this.name = 'SseStreamError';
	}

	override isRetryable(): boolean {
		return true;
	}
}

/**
 * Client is offline and no cached data available.
 */
export class OfflineNoCacheError extends FlagsError {
	constructor() {
		super('Client is offline and no cached data is available');
		this.name = 'OfflineNoCacheError';
	}
}
