/**
 * Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
 * SPDX-License-Identifier: Proprietary
 */

/**
 * Feature flags client for evaluating flags against the Loom server.
 */

import { HttpClient } from '@loom/http';
import { FlagCache } from './cache';
import { SseConnection, type SseConfig, DEFAULT_SSE_CONFIG } from './sse';
import {
	InvalidSdkKeyError,
	InvalidBaseUrlError,
	AuthenticationError,
	RateLimitedError,
	InitializationTimeoutError,
	ClientClosedError,
	FlagNotFoundError,
	ServerError,
	OfflineNoCacheError,
	ConnectionError
} from './errors';
import type {
	EvaluationContext,
	EvaluationResult,
	BulkEvaluationResult,
	FlagStreamEvent,
	VariantValue
} from './types';
import { getVariantBool, getVariantString, getVariantJson } from './types';

/**
 * Options for creating a FlagsClient.
 */
export interface FlagsClientOptions {
	/** SDK key for authentication (required) */
	sdkKey: string;
	/** Base URL for the Loom server (required) */
	baseUrl: string;
	/** Timeout for initialization in ms (default: 10000) */
	initTimeoutMs?: number;
	/** Timeout for individual requests in ms (default: 5000) */
	requestTimeoutMs?: number;
	/** Whether to enable SSE streaming (default: true) */
	enableStreaming?: boolean;
	/** SSE configuration */
	sseConfig?: Partial<SseConfig>;
	/** Whether to use offline mode when disconnected (default: true) */
	offlineMode?: boolean;
}

/**
 * Event types emitted by the FlagsClient.
 */
export type FlagsClientEventType = 'flag.updated' | 'flag.archived' | 'flag.restored' | 'killswitch.activated' | 'killswitch.deactivated' | 'connected' | 'disconnected' | 'error';

/**
 * Event handler for FlagsClient events.
 */
export type FlagsClientEventHandler<T = unknown> = (data: T) => void;

/**
 * Client for evaluating feature flags against the Loom server.
 *
 * The client maintains a local cache of flag states and can optionally
 * receive real-time updates via SSE streaming.
 *
 * @example
 * ```typescript
 * const client = new FlagsClient({
 *   sdkKey: 'loom_sdk_client_prod_xxx',
 *   baseUrl: 'https://loom.example.com',
 * });
 *
 * await client.initialize();
 *
 * const enabled = await client.getBool('feature.new_flow', context, false);
 * ```
 */
export class FlagsClient {
	private readonly sdkKey: string;
	private readonly baseUrl: string;
	private readonly initTimeoutMs: number;
	private readonly requestTimeoutMs: number;
	private readonly enableStreaming: boolean;
	private readonly sseConfig: SseConfig;
	private readonly offlineMode: boolean;

	private readonly httpClient: HttpClient;
	private readonly cache: FlagCache;
	private readonly sseConnection: SseConnection;
	private closed = false;

	// Event handlers
	private eventHandlers: Map<FlagsClientEventType, FlagsClientEventHandler[]> = new Map();

	constructor(options: FlagsClientOptions) {
		// Validate SDK key
		if (!options.sdkKey || !options.sdkKey.startsWith('loom_sdk_')) {
			throw new InvalidSdkKeyError();
		}

		// Validate base URL
		if (!options.baseUrl) {
			throw new InvalidBaseUrlError();
		}

		this.sdkKey = options.sdkKey;
		this.baseUrl = options.baseUrl.replace(/\/$/, ''); // Remove trailing slash
		this.initTimeoutMs = options.initTimeoutMs ?? 10000;
		this.requestTimeoutMs = options.requestTimeoutMs ?? 5000;
		this.enableStreaming = options.enableStreaming ?? true;
		this.sseConfig = { ...DEFAULT_SSE_CONFIG, ...options.sseConfig };
		this.offlineMode = options.offlineMode ?? true;

		this.httpClient = new HttpClient({
			baseUrl: this.baseUrl,
			timeoutMs: this.requestTimeoutMs,
			defaultHeaders: {
				Authorization: `Bearer ${this.sdkKey}`
			}
		});

		this.cache = new FlagCache();
		this.sseConnection = new SseConnection();

		// Set up SSE event forwarding
		this.sseConnection.onEvent((event) => this.handleSseEvent(event));
		this.sseConnection.onConnected(() => this.emit('connected', undefined));
		this.sseConnection.onDisconnected(() => this.emit('disconnected', undefined));
		this.sseConnection.onError((error) => this.emit('error', error));
	}

	/**
	 * Register an event handler.
	 */
	on<T = unknown>(event: FlagsClientEventType, handler: FlagsClientEventHandler<T>): void {
		const handlers = this.eventHandlers.get(event) ?? [];
		handlers.push(handler as FlagsClientEventHandler);
		this.eventHandlers.set(event, handlers);
	}

	/**
	 * Unregister an event handler.
	 */
	off<T = unknown>(event: FlagsClientEventType, handler: FlagsClientEventHandler<T>): void {
		const handlers = this.eventHandlers.get(event) ?? [];
		const index = handlers.indexOf(handler as FlagsClientEventHandler);
		if (index >= 0) {
			handlers.splice(index, 1);
		}
	}

	private emit<T>(event: FlagsClientEventType, data: T): void {
		const handlers = this.eventHandlers.get(event) ?? [];
		for (const handler of handlers) {
			try {
				handler(data);
			} catch (e) {
				console.error(`Error in event handler for ${event}:`, e);
			}
		}
	}

	private handleSseEvent(event: FlagStreamEvent): void {
		switch (event.event) {
			case 'flag.updated':
				this.emit('flag.updated', event.data);
				break;
			case 'flag.archived':
				this.emit('flag.archived', event.data);
				break;
			case 'flag.restored':
				this.emit('flag.restored', event.data);
				break;
			case 'killswitch.activated':
				this.emit('killswitch.activated', event.data);
				break;
			case 'killswitch.deactivated':
				this.emit('killswitch.deactivated', event.data);
				break;
		}
	}

	/**
	 * Initializes the client by fetching current flag states.
	 *
	 * This must be called before using the client.
	 */
	async initialize(): Promise<void> {
		const controller = new AbortController();
		const timeout = setTimeout(() => controller.abort(), this.initTimeoutMs);

		try {
			const response = await this.httpClient.post(
				'/api/flags/evaluate',
				{ context: { environment: '' } },
				{ signal: controller.signal, retry: false }
			);

			if (response.status === 401) {
				throw new AuthenticationError();
			}

			if (!response.ok) {
				const body = await response.text();
				throw new ServerError(response.status, response.statusText, body);
			}

			const result = (await response.json()) as BulkEvaluationResult;

			// Convert evaluation results to flag states and initialize cache
			const flags = result.results.map((r) => ({
				key: r.flagKey,
				id: '', // We don't have the ID from evaluation
				enabled: r.reason.type !== 'Disabled',
				defaultVariant: r.variant,
				defaultValue: r.value,
				archived: false
			}));

			this.cache.initialize(flags, []);

			// Start SSE streaming if enabled
			if (this.enableStreaming) {
				this.sseConnection.start(
					`${this.baseUrl}/api/flags/stream`,
					this.sdkKey,
					this.cache,
					this.sseConfig
				);
			}
		} catch (e) {
			if (e instanceof Error && e.name === 'AbortError') {
				throw new InitializationTimeoutError();
			}
			throw e;
		} finally {
			clearTimeout(timeout);
		}
	}

	/**
	 * Evaluates a boolean flag.
	 *
	 * @param flagKey - The flag key to evaluate
	 * @param context - The evaluation context
	 * @param defaultValue - Default value if flag is not found or evaluation fails
	 * @returns The boolean value of the flag, or the default if not found
	 */
	async getBool(
		flagKey: string,
		context: EvaluationContext,
		defaultValue: boolean
	): Promise<boolean> {
		this.checkClosed();

		const result = await this.evaluateFlag(flagKey, context);
		return getVariantBool(result.value, defaultValue);
	}

	/**
	 * Evaluates a string flag.
	 *
	 * @param flagKey - The flag key to evaluate
	 * @param context - The evaluation context
	 * @param defaultValue - Default value if flag is not found or evaluation fails
	 * @returns The string value of the flag, or the default if not found
	 */
	async getString(
		flagKey: string,
		context: EvaluationContext,
		defaultValue: string
	): Promise<string> {
		this.checkClosed();

		const result = await this.evaluateFlag(flagKey, context);
		return getVariantString(result.value, defaultValue);
	}

	/**
	 * Evaluates a JSON flag.
	 *
	 * @param flagKey - The flag key to evaluate
	 * @param context - The evaluation context
	 * @param defaultValue - Default value if flag is not found or evaluation fails
	 * @returns The JSON value of the flag, or the default if not found
	 */
	async getJson<T>(
		flagKey: string,
		context: EvaluationContext,
		defaultValue: T
	): Promise<T> {
		this.checkClosed();

		const result = await this.evaluateFlag(flagKey, context);
		return getVariantJson(result.value, defaultValue);
	}

	/**
	 * Evaluates all flags for the given context.
	 *
	 * @param context - The evaluation context
	 * @returns A bulk result containing all flag evaluations
	 */
	async getAll(context: EvaluationContext): Promise<BulkEvaluationResult> {
		this.checkClosed();

		try {
			return await this.evaluateAllServer(context);
		} catch (e) {
			if (this.offlineMode && this.shouldUseCache(e)) {
				return this.evaluateAllCached();
			}
			throw e;
		}
	}

	/**
	 * Returns true if the SSE connection is currently active.
	 */
	isStreaming(): boolean {
		return this.sseConnection.isConnected();
	}

	/**
	 * Returns true if the cache has been initialized.
	 */
	isInitialized(): boolean {
		return this.cache.isInitialized();
	}

	/**
	 * Returns the number of cached flags.
	 */
	cachedFlagCount(): number {
		return this.cache.flagCount();
	}

	/**
	 * Closes the client and stops any background tasks.
	 */
	close(): void {
		this.closed = true;
		this.sseConnection.stop();
	}

	private checkClosed(): void {
		if (this.closed) {
			throw new ClientClosedError();
		}
	}

	private shouldUseCache(error: unknown): boolean {
		if (error instanceof ConnectionError) return true;
		if (error instanceof ServerError && error.statusCode >= 500) return true;
		if (error instanceof RateLimitedError) return true;
		return false;
	}

	private async evaluateFlag(
		flagKey: string,
		context: EvaluationContext
	): Promise<EvaluationResult> {
		// Check cache first for kill switch
		const ksKey = this.cache.isFlagKilled(flagKey);
		if (ksKey) {
			const flag = this.cache.getFlag(flagKey);
			if (flag) {
				return {
					flagKey,
					variant: flag.defaultVariant,
					value: flag.defaultValue,
					reason: { type: 'KillSwitch', killSwitchId: '' }
				};
			}
		}

		// Try server-side evaluation
		try {
			return await this.evaluateFlagServer(flagKey, context);
		} catch (e) {
			if (this.offlineMode && this.shouldUseCache(e)) {
				return this.evaluateFlagCached(flagKey);
			}
			throw e;
		}
	}

	private async evaluateFlagServer(
		flagKey: string,
		context: EvaluationContext
	): Promise<EvaluationResult> {
		const response = await this.httpClient.post(`/api/flags/${flagKey}/evaluate`, {
			context
		});

		if (response.status === 404) {
			throw new FlagNotFoundError(flagKey);
		}

		if (response.status === 429) {
			const retryAfter = response.headers.get('Retry-After');
			throw new RateLimitedError(retryAfter ? parseInt(retryAfter, 10) : undefined);
		}

		if (!response.ok) {
			const body = await response.text();
			throw new ServerError(response.status, response.statusText, body);
		}

		return response.json() as Promise<EvaluationResult>;
	}

	private evaluateFlagCached(flagKey: string): EvaluationResult {
		if (!this.cache.isInitialized()) {
			throw new OfflineNoCacheError();
		}

		const flag = this.cache.getFlag(flagKey);
		if (!flag) {
			throw new FlagNotFoundError(flagKey);
		}

		const reason = flag.archived || !flag.enabled ? { type: 'Disabled' as const } : { type: 'Default' as const };

		return {
			flagKey,
			variant: flag.defaultVariant,
			value: flag.defaultValue,
			reason
		};
	}

	private async evaluateAllServer(context: EvaluationContext): Promise<BulkEvaluationResult> {
		const response = await this.httpClient.post('/api/flags/evaluate', { context });

		if (!response.ok) {
			const body = await response.text();
			throw new ServerError(response.status, response.statusText, body);
		}

		return response.json() as Promise<BulkEvaluationResult>;
	}

	private evaluateAllCached(): BulkEvaluationResult {
		if (!this.cache.isInitialized()) {
			throw new OfflineNoCacheError();
		}

		const flags = this.cache.getAllFlags();
		const results: EvaluationResult[] = flags
			.filter((f) => !f.archived)
			.map((f) => ({
				flagKey: f.key,
				variant: f.defaultVariant,
				value: f.defaultValue,
				reason: f.enabled ? { type: 'Default' as const } : { type: 'Disabled' as const }
			}));

		return {
			results,
			evaluatedAt: new Date().toISOString()
		};
	}
}
