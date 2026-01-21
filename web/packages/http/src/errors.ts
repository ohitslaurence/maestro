/**
 * Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
 * SPDX-License-Identifier: Proprietary
 */

/**
 * Base error for HTTP-related errors.
 */
export class HttpError extends Error {
	readonly statusCode?: number;
	readonly statusText?: string;
	readonly body?: string;

	constructor(
		message: string,
		options?: {
			statusCode?: number;
			statusText?: string;
			body?: string;
			cause?: unknown;
		}
	) {
		super(message, { cause: options?.cause });
		this.name = 'HttpError';
		this.statusCode = options?.statusCode;
		this.statusText = options?.statusText;
		this.body = options?.body;
	}

	/**
	 * Returns true if the error is retryable based on status code.
	 */
	isRetryable(): boolean {
		if (!this.statusCode) return false;

		// Retry on these status codes
		const retryableStatuses = [
			408, // Request Timeout
			429, // Too Many Requests
			500, // Internal Server Error
			502, // Bad Gateway
			503, // Service Unavailable
			504 // Gateway Timeout
		];

		return retryableStatuses.includes(this.statusCode);
	}
}

/**
 * Error thrown when a request times out.
 */
export class TimeoutError extends Error {
	readonly timeoutMs: number;

	constructor(message: string, timeoutMs: number) {
		super(message);
		this.name = 'TimeoutError';
		this.timeoutMs = timeoutMs;
	}

	isRetryable(): boolean {
		return true;
	}
}

/**
 * Error thrown when a network error occurs.
 */
export class NetworkError extends Error {
	constructor(message: string, options?: { cause?: unknown }) {
		super(message, { cause: options?.cause });
		this.name = 'NetworkError';
	}

	isRetryable(): boolean {
		return true;
	}
}

/**
 * Error thrown when rate limited (429).
 */
export class RateLimitError extends HttpError {
	readonly retryAfterSecs?: number;

	constructor(
		message: string,
		options?: {
			retryAfterSecs?: number;
			body?: string;
		}
	) {
		super(message, { statusCode: 429, statusText: 'Too Many Requests', body: options?.body });
		this.name = 'RateLimitError';
		this.retryAfterSecs = options?.retryAfterSecs;
	}
}
