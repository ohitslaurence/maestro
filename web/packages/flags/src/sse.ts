/**
 * Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
 * SPDX-License-Identifier: Proprietary
 */

/**
 * SSE (Server-Sent Events) connection for real-time flag updates.
 *
 * This module manages the SSE connection to the server for receiving
 * real-time updates to flag configurations and kill switch states.
 */

import type { FlagCache } from './cache';
import type { FlagStreamEvent, VariantValue } from './types';
import { SseConnectionError, SseStreamError } from './errors';

/**
 * Configuration for SSE connection behavior.
 */
export interface SseConfig {
	/** Base delay for reconnection attempts in ms (default: 1000) */
	reconnectBaseDelayMs: number;
	/** Maximum delay for reconnection attempts in ms (default: 30000) */
	reconnectMaxDelayMs: number;
	/** Maximum number of reconnection attempts (0 = unlimited) */
	maxReconnectAttempts: number;
	/** Whether to use exponential backoff for reconnection (default: true) */
	useExponentialBackoff: boolean;
}

/**
 * Default SSE configuration.
 */
export const DEFAULT_SSE_CONFIG: SseConfig = {
	reconnectBaseDelayMs: 1000,
	reconnectMaxDelayMs: 30000,
	maxReconnectAttempts: 0, // Unlimited
	useExponentialBackoff: true
};

/**
 * Event handler types.
 */
export type SseEventHandler = (event: FlagStreamEvent) => void;
export type SseErrorHandler = (error: Error) => void;
export type SseConnectedHandler = () => void;
export type SseDisconnectedHandler = () => void;

/**
 * Manages an SSE connection for real-time flag updates.
 */
export class SseConnection {
	private eventSource: EventSource | null = null;
	private connected = false;
	private reconnectAttempts = 0;
	private eventsReceived = 0;
	private cache: FlagCache | null = null;
	private config: SseConfig = DEFAULT_SSE_CONFIG;
	private streamUrl = '';
	private sdkKey = '';
	private reconnectTimeout: ReturnType<typeof setTimeout> | null = null;
	private aborted = false;

	// Event handlers
	private onEventHandlers: SseEventHandler[] = [];
	private onErrorHandlers: SseErrorHandler[] = [];
	private onConnectedHandlers: SseConnectedHandler[] = [];
	private onDisconnectedHandlers: SseDisconnectedHandler[] = [];

	/**
	 * Returns true if the SSE connection is currently active.
	 */
	isConnected(): boolean {
		return this.connected;
	}

	/**
	 * Returns the number of reconnection attempts since the connection was started.
	 */
	getReconnectAttempts(): number {
		return this.reconnectAttempts;
	}

	/**
	 * Returns the number of events received since the connection was started.
	 */
	getEventsReceived(): number {
		return this.eventsReceived;
	}

	/**
	 * Register an event handler.
	 */
	onEvent(handler: SseEventHandler): void {
		this.onEventHandlers.push(handler);
	}

	/**
	 * Register an error handler.
	 */
	onError(handler: SseErrorHandler): void {
		this.onErrorHandlers.push(handler);
	}

	/**
	 * Register a connected handler.
	 */
	onConnected(handler: SseConnectedHandler): void {
		this.onConnectedHandlers.push(handler);
	}

	/**
	 * Register a disconnected handler.
	 */
	onDisconnected(handler: SseDisconnectedHandler): void {
		this.onDisconnectedHandlers.push(handler);
	}

	/**
	 * Starts the SSE connection.
	 *
	 * The connection will automatically reconnect on failure with exponential backoff.
	 */
	start(
		streamUrl: string,
		sdkKey: string,
		cache: FlagCache,
		config: Partial<SseConfig> = {}
	): void {
		this.streamUrl = streamUrl;
		this.sdkKey = sdkKey;
		this.cache = cache;
		this.config = { ...DEFAULT_SSE_CONFIG, ...config };
		this.aborted = false;
		this.connect();
	}

	/**
	 * Stops the SSE connection.
	 */
	stop(): void {
		this.aborted = true;
		if (this.reconnectTimeout) {
			clearTimeout(this.reconnectTimeout);
			this.reconnectTimeout = null;
		}
		if (this.eventSource) {
			this.eventSource.close();
			this.eventSource = null;
		}
		this.connected = false;
	}

	private connect(): void {
		if (this.aborted) return;

		// Note: EventSource doesn't support custom headers directly.
		// We need to pass the SDK key via query parameter or use a polyfill.
		// For browser compatibility, we'll use a query parameter approach.
		const url = new URL(this.streamUrl);
		url.searchParams.set('sdk_key', this.sdkKey);

		try {
			this.eventSource = new EventSource(url.toString());

			this.eventSource.onopen = () => {
				this.connected = true;
				this.reconnectAttempts = 0;
				this.onConnectedHandlers.forEach((h) => h());
			};

			this.eventSource.onmessage = (event) => {
				this.eventsReceived++;
				try {
					this.processEvent(event.data);
				} catch (e) {
					const error = e instanceof Error ? e : new Error(String(e));
					this.onErrorHandlers.forEach((h) => h(error));
				}
			};

			this.eventSource.onerror = () => {
				this.connected = false;
				this.eventSource?.close();
				this.eventSource = null;
				this.onDisconnectedHandlers.forEach((h) => h());

				if (!this.aborted) {
					this.scheduleReconnect();
				}
			};
		} catch (e) {
			const error = new SseConnectionError(e instanceof Error ? e.message : String(e));
			this.onErrorHandlers.forEach((h) => h(error));
			this.scheduleReconnect();
		}
	}

	private scheduleReconnect(): void {
		if (this.aborted) return;

		// Check max reconnect attempts
		if (
			this.config.maxReconnectAttempts > 0 &&
			this.reconnectAttempts >= this.config.maxReconnectAttempts
		) {
			const error = new SseConnectionError('Max reconnection attempts reached');
			this.onErrorHandlers.forEach((h) => h(error));
			return;
		}

		// Calculate backoff delay
		let delay: number;
		if (this.config.useExponentialBackoff) {
			const factor = Math.pow(2, Math.min(this.reconnectAttempts, 10));
			delay = Math.min(
				this.config.reconnectBaseDelayMs * factor,
				this.config.reconnectMaxDelayMs
			);
		} else {
			delay = this.config.reconnectBaseDelayMs;
		}

		this.reconnectAttempts++;
		this.reconnectTimeout = setTimeout(() => {
			this.reconnectTimeout = null;
			this.connect();
		}, delay);
	}

	private processEvent(data: string): void {
		if (!data || !this.cache) return;

		let event: FlagStreamEvent;
		try {
			event = JSON.parse(data) as FlagStreamEvent;
		} catch (e) {
			throw new SseStreamError(`Failed to parse SSE event: ${e}`);
		}

		// Notify handlers
		this.onEventHandlers.forEach((h) => h(event));

		// Update cache based on event type
		switch (event.event) {
			case 'init':
				this.cache.initialize(event.data.flags, event.data.killSwitches);
				break;

			case 'flag.updated':
				this.cache.updateFlagEnabled(event.data.flagKey, event.data.enabled);
				this.cache.updateFlagVariant(
					event.data.flagKey,
					event.data.defaultVariant,
					event.data.defaultValue
				);
				break;

			case 'flag.archived':
				this.cache.archiveFlag(event.data.flagKey);
				break;

			case 'flag.restored':
				this.cache.restoreFlag(event.data.flagKey, event.data.enabled);
				break;

			case 'killswitch.activated':
				this.cache.activateKillSwitch(event.data.killSwitchKey, event.data.reason);
				break;

			case 'killswitch.deactivated':
				this.cache.deactivateKillSwitch(event.data.killSwitchKey);
				break;

			case 'heartbeat':
				// Just acknowledge the heartbeat
				break;
		}
	}
}
