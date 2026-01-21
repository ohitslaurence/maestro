/**
 * Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
 * SPDX-License-Identifier: Proprietary
 */

/**
 * Event properties that can be attached to any event.
 */
export type EventProperties = Record<string, unknown>;

/**
 * Person properties that persist on the user profile.
 */
export type PersonProperties = Record<string, unknown>;

/**
 * Payload for capturing a single event.
 */
export interface CapturePayload {
	/** The distinct_id of the user */
	distinct_id: string;
	/** The event name (e.g., "button_clicked", "$pageview") */
	event: string;
	/** Event-specific properties */
	properties?: EventProperties;
	/** Timestamp for the event (defaults to server time) */
	timestamp?: string;
}

/**
 * Payload for batch capturing events.
 */
export interface BatchCapturePayload {
	/** Array of events to capture */
	batch: CapturePayload[];
}

/**
 * Payload for identifying a user.
 */
export interface IdentifyPayload {
	/** Current distinct_id (often anonymous) */
	distinct_id: string;
	/** The "real" user identifier (email, user_id, etc.) */
	user_id: string;
	/** Person properties to set/update */
	properties?: PersonProperties;
}

/**
 * Payload for aliasing two distinct_ids.
 */
export interface AliasPayload {
	/** The primary identity */
	distinct_id: string;
	/** The secondary identity to link */
	alias: string;
}

/**
 * Payload for setting person properties.
 */
export interface SetPayload {
	/** The distinct_id of the user */
	distinct_id: string;
	/** Properties to set */
	properties: PersonProperties;
}

/**
 * Storage persistence mode for the SDK.
 */
export type PersistenceMode =
	| 'localStorage+cookie' // Persist in both (recommended for browsers)
	| 'localStorage' // Persist in localStorage only
	| 'cookie' // Persist in cookie only (cross-subdomain)
	| 'memory'; // In-memory only (no persistence)

/**
 * Configuration for batch processing.
 */
export interface BatchConfig {
	/** Flush interval in milliseconds (default: 10000) */
	flushIntervalMs: number;
	/** Maximum events per batch (default: 10) */
	maxBatchSize: number;
	/** Maximum queue size before dropping oldest (default: 1000) */
	maxQueueSize: number;
}

/**
 * Configuration for autocapture.
 */
export interface AutocaptureConfig {
	/** Capture $pageview on page load (default: true) */
	pageview: boolean;
	/** Capture $pageleave on page unload (default: true) */
	pageleave: boolean;
}

/**
 * Options for creating an AnalyticsClient.
 */
export interface AnalyticsClientOptions {
	/** Analytics API key (write-only or read-write) */
	apiKey: string;
	/** Base URL of the Loom server */
	baseUrl: string;
	/** Storage persistence mode (default: 'localStorage+cookie') */
	persistence?: PersistenceMode;
	/** Batch configuration */
	batch?: Partial<BatchConfig>;
	/** Autocapture configuration (default: { pageview: true, pageleave: true }) */
	autocapture?: boolean | Partial<AutocaptureConfig>;
	/** Request timeout in milliseconds (default: 5000) */
	timeoutMs?: number;
	/** Whether to log debug information (default: false) */
	debug?: boolean;
	/** Cookie domain for cross-subdomain tracking */
	cookieDomain?: string;
	/** Cookie name for distinct_id (default: 'loom_analytics_distinct_id') */
	cookieName?: string;
}

/**
 * Internal queued event for batch processing.
 */
export interface QueuedEvent {
	/** The event payload */
	payload: CapturePayload;
	/** When the event was queued */
	queuedAt: number;
}

/**
 * API response for capture operations.
 */
export interface CaptureResponse {
	/** Whether the operation succeeded */
	success: boolean;
	/** Error message if failed */
	error?: string;
}

/**
 * API response for identify operations.
 */
export interface IdentifyResponse {
	/** Whether the operation succeeded */
	success: boolean;
	/** The person ID after identify */
	person_id?: string;
	/** Error message if failed */
	error?: string;
}

/**
 * SDK library information added to events.
 */
export interface LibraryInfo {
	/** SDK name */
	$lib: string;
	/** SDK version */
	$lib_version: string;
}

/**
 * Default batch configuration values.
 */
export const DEFAULT_BATCH_CONFIG: BatchConfig = {
	flushIntervalMs: 10000, // 10 seconds
	maxBatchSize: 10,
	maxQueueSize: 1000
};

/**
 * Default autocapture configuration values.
 */
export const DEFAULT_AUTOCAPTURE_CONFIG: AutocaptureConfig = {
	pageview: true,
	pageleave: true
};

/**
 * Cookie settings.
 */
export const COOKIE_SETTINGS = {
	defaultName: 'loom_analytics_distinct_id',
	maxAge: 365 * 24 * 60 * 60, // 1 year in seconds
	sameSite: 'Lax' as const,
	path: '/'
};

/**
 * LocalStorage key for distinct_id.
 */
export const LOCALSTORAGE_KEY = 'loom_analytics_distinct_id';

/**
 * SDK version.
 */
export const SDK_VERSION = '0.1.0';

/**
 * SDK name.
 */
export const SDK_NAME = '@loom/analytics';
