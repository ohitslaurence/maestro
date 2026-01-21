/**
 * Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
 * SPDX-License-Identifier: Proprietary
 */

/**
 * Base error class for all analytics SDK errors.
 */
export class AnalyticsError extends Error {
	constructor(message: string, options?: { cause?: unknown }) {
		super(message, { cause: options?.cause });
		this.name = 'AnalyticsError';
	}

	/**
	 * Returns true if the error is retryable.
	 */
	isRetryable(): boolean {
		return false;
	}
}

/**
 * Error thrown when the API key is invalid.
 */
export class InvalidApiKeyError extends AnalyticsError {
	constructor(message = 'Invalid API key format') {
		super(message);
		this.name = 'InvalidApiKeyError';
	}
}

/**
 * Error thrown when the base URL is invalid.
 */
export class InvalidBaseUrlError extends AnalyticsError {
	constructor(message = 'Invalid base URL') {
		super(message);
		this.name = 'InvalidBaseUrlError';
	}
}

/**
 * Error thrown when authentication fails.
 */
export class AuthenticationError extends AnalyticsError {
	constructor(message = 'Authentication failed') {
		super(message);
		this.name = 'AuthenticationError';
	}
}

/**
 * Error thrown when rate limited.
 */
export class RateLimitedError extends AnalyticsError {
	readonly retryAfterSecs?: number;

	constructor(message = 'Rate limited', retryAfterSecs?: number) {
		super(message);
		this.name = 'RateLimitedError';
		this.retryAfterSecs = retryAfterSecs;
	}

	isRetryable(): boolean {
		return true;
	}
}

/**
 * Error thrown when the client is closed.
 */
export class ClientClosedError extends AnalyticsError {
	constructor(message = 'Analytics client has been closed') {
		super(message);
		this.name = 'ClientClosedError';
	}
}

/**
 * Error thrown when a capture operation fails.
 */
export class CaptureError extends AnalyticsError {
	readonly eventName: string;

	constructor(eventName: string, message?: string, options?: { cause?: unknown }) {
		super(message ?? `Failed to capture event: ${eventName}`, options);
		this.name = 'CaptureError';
		this.eventName = eventName;
	}

	isRetryable(): boolean {
		return true;
	}
}

/**
 * Error thrown when an identify operation fails.
 */
export class IdentifyError extends AnalyticsError {
	readonly distinctId: string;
	readonly userId: string;

	constructor(distinctId: string, userId: string, message?: string, options?: { cause?: unknown }) {
		super(message ?? `Failed to identify user: ${userId}`, options);
		this.name = 'IdentifyError';
		this.distinctId = distinctId;
		this.userId = userId;
	}

	isRetryable(): boolean {
		return true;
	}
}

/**
 * Error thrown when storage operations fail.
 */
export class StorageError extends AnalyticsError {
	constructor(message: string, options?: { cause?: unknown }) {
		super(message, options);
		this.name = 'StorageError';
	}
}

/**
 * Error thrown when event validation fails.
 */
export class ValidationError extends AnalyticsError {
	readonly field: string;

	constructor(field: string, message: string) {
		super(message);
		this.name = 'ValidationError';
		this.field = field;
	}
}

/**
 * Error thrown when a network operation fails.
 */
export class NetworkError extends AnalyticsError {
	constructor(message = 'Network request failed', options?: { cause?: unknown }) {
		super(message, options);
		this.name = 'NetworkError';
	}

	isRetryable(): boolean {
		return true;
	}
}

/**
 * Error thrown when a server returns an error.
 */
export class ServerError extends AnalyticsError {
	readonly statusCode: number;

	constructor(statusCode: number, message?: string, options?: { cause?: unknown }) {
		super(message ?? `Server error: ${statusCode}`, options);
		this.name = 'ServerError';
		this.statusCode = statusCode;
	}

	isRetryable(): boolean {
		// Retry on 5xx errors
		return this.statusCode >= 500 && this.statusCode < 600;
	}
}
