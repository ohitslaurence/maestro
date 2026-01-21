/**
 * Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
 * SPDX-License-Identifier: Proprietary
 */

import { HttpClient } from '@loom/http';
import type {
	AnalyticsClientOptions,
	AutocaptureConfig,
	BatchCapturePayload,
	CapturePayload,
	EventProperties,
	PersonProperties,
	PersistenceMode
} from './types';
import {
	DEFAULT_AUTOCAPTURE_CONFIG,
	DEFAULT_BATCH_CONFIG,
	SDK_NAME,
	SDK_VERSION
} from './types';
import {
	InvalidApiKeyError,
	InvalidBaseUrlError,
	ClientClosedError
} from './errors';
import {
	createStorage,
	DistinctIdManager
} from './storage';
import { BatchProcessor, type BatchSender } from './batch';

/**
 * Validate API key format.
 * Valid formats: loom_analytics_write_xxx or loom_analytics_rw_xxx
 */
function validateApiKey(apiKey: string): void {
	if (!apiKey || typeof apiKey !== 'string') {
		throw new InvalidApiKeyError('API key is required');
	}
	if (!apiKey.startsWith('loom_analytics_write_') && !apiKey.startsWith('loom_analytics_rw_')) {
		throw new InvalidApiKeyError(
			'API key must start with "loom_analytics_write_" or "loom_analytics_rw_"'
		);
	}
}

/**
 * Validate base URL format.
 */
function validateBaseUrl(baseUrl: string): void {
	if (!baseUrl || typeof baseUrl !== 'string') {
		throw new InvalidBaseUrlError('Base URL is required');
	}
	try {
		new URL(baseUrl);
	} catch {
		throw new InvalidBaseUrlError(`Invalid base URL: ${baseUrl}`);
	}
}

/**
 * Parse autocapture configuration.
 */
function parseAutocaptureConfig(
	autocapture?: boolean | Partial<AutocaptureConfig>
): AutocaptureConfig | null {
	if (autocapture === false) {
		return null;
	}
	if (autocapture === true || autocapture === undefined) {
		return DEFAULT_AUTOCAPTURE_CONFIG;
	}
	return { ...DEFAULT_AUTOCAPTURE_CONFIG, ...autocapture };
}

/**
 * Analytics client for tracking events, identifying users, and managing identity.
 *
 * @example
 * ```typescript
 * const analytics = new AnalyticsClient({
 *   apiKey: 'loom_analytics_write_xxx',
 *   baseUrl: 'https://loom.example.com',
 * });
 *
 * // Track an event
 * analytics.capture('button_clicked', { button_name: 'checkout' });
 *
 * // Identify a user
 * analytics.identify('user@example.com', { plan: 'pro' });
 *
 * // On logout
 * analytics.reset();
 * ```
 */
export class AnalyticsClient implements BatchSender {
	private readonly apiKey: string;
	private readonly httpClient: HttpClient;
	private readonly distinctIdManager: DistinctIdManager;
	private readonly batchProcessor: BatchProcessor;
	private readonly autocaptureConfig: AutocaptureConfig | null;
	private readonly debug: boolean;

	private isClosed = false;
	private pageviewHandler?: () => void;
	private pageleaveHandler?: () => void;

	constructor(options: AnalyticsClientOptions) {
		validateApiKey(options.apiKey);
		validateBaseUrl(options.baseUrl);

		this.apiKey = options.apiKey;
		this.debug = options.debug ?? false;

		// Create HTTP client
		this.httpClient = new HttpClient({
			baseUrl: options.baseUrl,
			timeoutMs: options.timeoutMs ?? 5000,
			userAgent: `${SDK_NAME}/${SDK_VERSION}`,
			defaultHeaders: {
				Authorization: `Bearer ${options.apiKey}`
			}
		});

		// Create storage and distinct_id manager
		const persistence: PersistenceMode = options.persistence ?? 'localStorage+cookie';
		const storage = createStorage(persistence, options.cookieName, options.cookieDomain);
		this.distinctIdManager = new DistinctIdManager(storage);

		// Initialize distinct_id
		this.distinctIdManager.initialize();

		// Create batch processor
		this.batchProcessor = new BatchProcessor(
			this,
			{
				...DEFAULT_BATCH_CONFIG,
				...options.batch
			},
			{
				onError: (error) => {
					if (this.debug) {
						console.error('[Analytics] Batch error:', error);
					}
				},
				onDrop: (events) => {
					if (this.debug) {
						console.warn('[Analytics] Dropped events due to queue overflow:', events.length);
					}
				}
			}
		);

		// Start batch processor
		this.batchProcessor.start();

		// Setup autocapture
		this.autocaptureConfig = parseAutocaptureConfig(options.autocapture);
		if (this.autocaptureConfig) {
			this.setupAutocapture();
		}
	}

	/**
	 * Setup autocapture event handlers.
	 */
	private setupAutocapture(): void {
		if (typeof window === 'undefined') {
			return;
		}

		if (this.autocaptureConfig?.pageview) {
			// Capture pageview on load
			this.capturePageview();

			// Capture pageview on history changes (SPA navigation)
			this.pageviewHandler = () => this.capturePageview();
			window.addEventListener('popstate', this.pageviewHandler);
		}

		if (this.autocaptureConfig?.pageleave) {
			this.pageleaveHandler = () => {
				this.capture('$pageleave', {
					$current_url: window.location.href,
					$pathname: window.location.pathname
				});
				// Force synchronous flush on page leave
				// Note: This is best-effort as the page may unload before the request completes
			};
			window.addEventListener('beforeunload', this.pageleaveHandler);
			window.addEventListener('pagehide', this.pageleaveHandler);
		}
	}

	/**
	 * Capture a $pageview event.
	 */
	private capturePageview(): void {
		if (typeof window === 'undefined') {
			return;
		}

		this.capture('$pageview', {
			$current_url: window.location.href,
			$pathname: window.location.pathname,
			$host: window.location.host,
			$referrer: document.referrer || undefined,
			$title: document.title
		});
	}

	/**
	 * Remove autocapture event handlers.
	 */
	private teardownAutocapture(): void {
		if (typeof window === 'undefined') {
			return;
		}

		if (this.pageviewHandler) {
			window.removeEventListener('popstate', this.pageviewHandler);
			this.pageviewHandler = undefined;
		}

		if (this.pageleaveHandler) {
			window.removeEventListener('beforeunload', this.pageleaveHandler);
			window.removeEventListener('pagehide', this.pageleaveHandler);
			this.pageleaveHandler = undefined;
		}
	}

	/**
	 * Capture an event.
	 *
	 * @param event - The event name (e.g., "button_clicked")
	 * @param properties - Optional event properties
	 */
	capture(event: string, properties?: EventProperties): void {
		if (this.isClosed) {
			if (this.debug) {
				console.warn('[Analytics] Client is closed, event not captured:', event);
			}
			return;
		}

		const payload: CapturePayload = {
			distinct_id: this.distinctIdManager.getDistinctId(),
			event,
			properties: {
				...properties,
				$lib: SDK_NAME,
				$lib_version: SDK_VERSION
			},
			timestamp: new Date().toISOString()
		};

		this.batchProcessor.enqueue(payload);

		if (this.debug) {
			console.log('[Analytics] Event captured:', event, properties);
		}
	}

	/**
	 * Identify a user, linking the current anonymous distinct_id to a user ID.
	 *
	 * @param userId - The user's real identifier (email, user_id, etc.)
	 * @param properties - Optional person properties to set
	 */
	async identify(userId: string, properties?: PersonProperties): Promise<void> {
		if (this.isClosed) {
			throw new ClientClosedError();
		}

		const distinctId = this.distinctIdManager.getDistinctId();

		try {
			await this.httpClient.post('/api/analytics/identify', {
				distinct_id: distinctId,
				user_id: userId,
				properties: properties ?? {}
			});

			// Update the distinct_id to the user_id
			this.distinctIdManager.setDistinctId(userId);

			// Capture $identify event
			this.capture('$identify', {
				$anon_distinct_id: distinctId,
				$user_id: userId
			});

			if (this.debug) {
				console.log('[Analytics] User identified:', userId);
			}
		} catch (error) {
			if (this.debug) {
				console.error('[Analytics] Identify failed:', error);
			}
			throw error;
		}
	}

	/**
	 * Alias two distinct_ids together.
	 *
	 * @param alias - The secondary identity to link to the current distinct_id
	 */
	async alias(alias: string): Promise<void> {
		if (this.isClosed) {
			throw new ClientClosedError();
		}

		const distinctId = this.distinctIdManager.getDistinctId();

		try {
			await this.httpClient.post('/api/analytics/alias', {
				distinct_id: distinctId,
				alias
			});

			if (this.debug) {
				console.log('[Analytics] Alias created:', alias);
			}
		} catch (error) {
			if (this.debug) {
				console.error('[Analytics] Alias failed:', error);
			}
			throw error;
		}
	}

	/**
	 * Set person properties.
	 *
	 * @param properties - Properties to set on the person
	 */
	async set(properties: PersonProperties): Promise<void> {
		if (this.isClosed) {
			throw new ClientClosedError();
		}

		const distinctId = this.distinctIdManager.getDistinctId();

		try {
			await this.httpClient.post('/api/analytics/set', {
				distinct_id: distinctId,
				properties
			});

			if (this.debug) {
				console.log('[Analytics] Properties set:', properties);
			}
		} catch (error) {
			if (this.debug) {
				console.error('[Analytics] Set properties failed:', error);
			}
			throw error;
		}
	}

	/**
	 * Reset the identity, generating a new anonymous distinct_id.
	 * Use this when a user logs out.
	 */
	reset(): void {
		if (this.isClosed) {
			return;
		}

		const newDistinctId = this.distinctIdManager.reset();

		if (this.debug) {
			console.log('[Analytics] Identity reset, new distinct_id:', newDistinctId);
		}
	}

	/**
	 * Get the current distinct_id.
	 */
	getDistinctId(): string {
		return this.distinctIdManager.getDistinctId();
	}

	/**
	 * Manually flush all queued events.
	 */
	async flush(): Promise<void> {
		await this.batchProcessor.flush();
	}

	/**
	 * Shutdown the client, flushing all pending events.
	 */
	async shutdown(): Promise<void> {
		if (this.isClosed) {
			return;
		}

		this.isClosed = true;
		this.teardownAutocapture();
		await this.batchProcessor.shutdown();

		if (this.debug) {
			console.log('[Analytics] Client shutdown');
		}
	}

	/**
	 * Check if the client has been closed.
	 */
	isClosed_(): boolean {
		return this.isClosed;
	}

	/**
	 * Get the number of events in the queue.
	 */
	getQueueSize(): number {
		return this.batchProcessor.getQueueSize();
	}

	/**
	 * Send a batch of events to the server.
	 * Implements the BatchSender interface.
	 */
	async sendBatch(events: CapturePayload[]): Promise<boolean> {
		if (events.length === 0) {
			return true;
		}

		try {
			const payload: BatchCapturePayload = { batch: events };
			await this.httpClient.post('/api/analytics/batch', payload);
			return true;
		} catch (error) {
			if (this.debug) {
				console.error('[Analytics] Batch send failed:', error);
			}
			return false;
		}
	}
}
