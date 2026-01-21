/**
 * Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
 * SPDX-License-Identifier: Proprietary
 */

/**
 * HTTP client with consistent User-Agent and retry support.
 */

import { HttpError, TimeoutError, NetworkError, RateLimitError } from './errors';
import { retry, type RetryConfig, DEFAULT_RETRY_CONFIG } from './retry';

/**
 * Options for creating an HttpClient.
 */
export interface HttpClientOptions {
	/** Base URL for all requests */
	baseUrl?: string;
	/** Request timeout in milliseconds (default: 30000) */
	timeoutMs?: number;
	/** Custom User-Agent string (default: loom-sdk/{version}) */
	userAgent?: string;
	/** Retry configuration */
	retryConfig?: Partial<RetryConfig>;
	/** Default headers to include in all requests */
	defaultHeaders?: Record<string, string>;
}

/**
 * Options for individual requests.
 */
export interface RequestOptions {
	/** Request headers */
	headers?: Record<string, string>;
	/** Request timeout in milliseconds */
	timeoutMs?: number;
	/** Whether to retry on failure (default: true) */
	retry?: boolean;
	/** Retry configuration for this request */
	retryConfig?: Partial<RetryConfig>;
	/** AbortSignal for cancellation */
	signal?: AbortSignal;
}

/**
 * HTTP client with retry, timeout, and User-Agent support.
 *
 * @example
 * ```typescript
 * const client = new HttpClient({
 *   baseUrl: 'https://api.example.com',
 *   timeoutMs: 5000,
 * });
 *
 * const response = await client.get('/users');
 * const user = await client.post('/users', { name: 'Alice' });
 * ```
 */
export class HttpClient {
	private readonly baseUrl: string;
	private readonly timeoutMs: number;
	private readonly userAgent: string;
	private readonly retryConfig: RetryConfig;
	private readonly defaultHeaders: Record<string, string>;

	constructor(options: HttpClientOptions = {}) {
		this.baseUrl = options.baseUrl?.replace(/\/$/, '') ?? '';
		this.timeoutMs = options.timeoutMs ?? 30000;
		this.userAgent = options.userAgent ?? `loom-sdk/${this.getVersion()}`;
		this.retryConfig = { ...DEFAULT_RETRY_CONFIG, ...options.retryConfig };
		this.defaultHeaders = {
			'Content-Type': 'application/json',
			...options.defaultHeaders
		};
	}

	private getVersion(): string {
		// In browser, we can't easily get package version, so use a placeholder
		return '0.1.0';
	}

	/**
	 * Build full URL from path.
	 */
	private buildUrl(path: string): string {
		if (path.startsWith('http://') || path.startsWith('https://')) {
			return path;
		}
		return `${this.baseUrl}${path.startsWith('/') ? '' : '/'}${path}`;
	}

	/**
	 * Build headers for a request.
	 */
	private buildHeaders(options?: RequestOptions): Headers {
		const headers = new Headers();

		// Add default headers
		for (const [key, value] of Object.entries(this.defaultHeaders)) {
			headers.set(key, value);
		}

		// Add User-Agent
		headers.set('User-Agent', this.userAgent);

		// Add request-specific headers
		if (options?.headers) {
			for (const [key, value] of Object.entries(options.headers)) {
				headers.set(key, value);
			}
		}

		return headers;
	}

	/**
	 * Execute a fetch request with timeout.
	 */
	private async fetchWithTimeout(
		url: string,
		init: RequestInit,
		timeoutMs: number,
		externalSignal?: AbortSignal
	): Promise<Response> {
		const controller = new AbortController();
		const timeout = setTimeout(() => controller.abort(), timeoutMs);

		// Combine external signal with timeout signal
		const signals = [controller.signal];
		if (externalSignal) {
			signals.push(externalSignal);
		}

		try {
			const response = await fetch(url, {
				...init,
				signal: externalSignal
					? // Use the external signal if provided, with our timeout
						controller.signal
					: controller.signal
			});

			// Check if external signal was aborted
			if (externalSignal?.aborted) {
				throw new Error('Request aborted');
			}

			return response;
		} catch (error) {
			if (error instanceof Error && error.name === 'AbortError') {
				if (controller.signal.aborted) {
					throw new TimeoutError(`Request timed out after ${timeoutMs}ms`, timeoutMs);
				}
				throw error;
			}
			throw new NetworkError('Network request failed', { cause: error });
		} finally {
			clearTimeout(timeout);
		}
	}

	/**
	 * Process response and throw appropriate errors.
	 */
	private async processResponse(response: Response): Promise<Response> {
		if (response.ok) {
			return response;
		}

		const body = await response.text().catch(() => '');

		if (response.status === 429) {
			const retryAfter = response.headers.get('Retry-After');
			throw new RateLimitError('Rate limited', {
				retryAfterSecs: retryAfter ? parseInt(retryAfter, 10) : undefined,
				body
			});
		}

		throw new HttpError(`HTTP ${response.status}: ${response.statusText}`, {
			statusCode: response.status,
			statusText: response.statusText,
			body
		});
	}

	/**
	 * Execute a request with optional retry.
	 */
	private async request(
		method: string,
		path: string,
		body?: unknown,
		options?: RequestOptions
	): Promise<Response> {
		const url = this.buildUrl(path);
		const headers = this.buildHeaders(options);
		const timeoutMs = options?.timeoutMs ?? this.timeoutMs;

		const init: RequestInit = {
			method,
			headers
		};

		if (body !== undefined) {
			init.body = JSON.stringify(body);
		}

		const doRequest = async () => {
			const response = await this.fetchWithTimeout(url, init, timeoutMs, options?.signal);
			return this.processResponse(response);
		};

		if (options?.retry === false) {
			return doRequest();
		}

		const retryConfig = { ...this.retryConfig, ...options?.retryConfig };
		return retry(doRequest, retryConfig);
	}

	/**
	 * Make a GET request.
	 */
	async get(path: string, options?: RequestOptions): Promise<Response> {
		return this.request('GET', path, undefined, options);
	}

	/**
	 * Make a POST request.
	 */
	async post(path: string, body?: unknown, options?: RequestOptions): Promise<Response> {
		return this.request('POST', path, body, options);
	}

	/**
	 * Make a PUT request.
	 */
	async put(path: string, body?: unknown, options?: RequestOptions): Promise<Response> {
		return this.request('PUT', path, body, options);
	}

	/**
	 * Make a PATCH request.
	 */
	async patch(path: string, body?: unknown, options?: RequestOptions): Promise<Response> {
		return this.request('PATCH', path, body, options);
	}

	/**
	 * Make a DELETE request.
	 */
	async delete(path: string, options?: RequestOptions): Promise<Response> {
		return this.request('DELETE', path, undefined, options);
	}

	/**
	 * Make a GET request and parse JSON response.
	 */
	async getJson<T>(path: string, options?: RequestOptions): Promise<T> {
		const response = await this.get(path, options);
		return response.json() as Promise<T>;
	}

	/**
	 * Make a POST request and parse JSON response.
	 */
	async postJson<T>(path: string, body?: unknown, options?: RequestOptions): Promise<T> {
		const response = await this.post(path, body, options);
		return response.json() as Promise<T>;
	}

	/**
	 * Make a PUT request and parse JSON response.
	 */
	async putJson<T>(path: string, body?: unknown, options?: RequestOptions): Promise<T> {
		const response = await this.put(path, body, options);
		return response.json() as Promise<T>;
	}

	/**
	 * Make a PATCH request and parse JSON response.
	 */
	async patchJson<T>(path: string, body?: unknown, options?: RequestOptions): Promise<T> {
		const response = await this.patch(path, body, options);
		return response.json() as Promise<T>;
	}

	/**
	 * Make a DELETE request and parse JSON response.
	 */
	async deleteJson<T>(path: string, options?: RequestOptions): Promise<T> {
		const response = await this.delete(path, options);
		return response.json() as Promise<T>;
	}
}
