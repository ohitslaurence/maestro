/**
 * Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
 * SPDX-License-Identifier: Proprietary
 */

/**
 * Product Analytics SDK for Loom.
 *
 * This package provides a client library for tracking events, identifying users,
 * and managing identity resolution with the Loom analytics system.
 *
 * @example
 * ```typescript
 * import { AnalyticsClient } from '@loom/analytics';
 *
 * const analytics = new AnalyticsClient({
 *   apiKey: 'loom_analytics_write_xxx',
 *   baseUrl: 'https://loom.example.com',
 * });
 *
 * // Track an event
 * analytics.capture('button_clicked', { button_name: 'checkout' });
 *
 * // Identify a user (links anonymous to authenticated)
 * await analytics.identify('user@example.com', { plan: 'pro' });
 *
 * // On logout, reset to new anonymous identity
 * analytics.reset();
 *
 * // On app shutdown
 * await analytics.shutdown();
 * ```
 */

// Client
export { AnalyticsClient } from './client';

// Types
export type {
	AnalyticsClientOptions,
	EventProperties,
	PersonProperties,
	PersistenceMode,
	BatchConfig,
	AutocaptureConfig,
	CapturePayload,
	BatchCapturePayload,
	IdentifyPayload,
	AliasPayload,
	SetPayload,
	CaptureResponse,
	IdentifyResponse,
	QueuedEvent,
	LibraryInfo
} from './types';

export {
	DEFAULT_BATCH_CONFIG,
	DEFAULT_AUTOCAPTURE_CONFIG,
	COOKIE_SETTINGS,
	LOCALSTORAGE_KEY,
	SDK_VERSION,
	SDK_NAME
} from './types';

// Storage
export {
	generateDistinctId,
	isValidDistinctId,
	DistinctIdManager,
	MemoryStorage,
	CookieStorage,
	LocalStorageStorage,
	CombinedStorage,
	createStorage,
	type DistinctIdStorage
} from './storage';

// Batch
export { BatchProcessor, type BatchSender, type BatchEventListener } from './batch';

// Errors
export {
	AnalyticsError,
	InvalidApiKeyError,
	InvalidBaseUrlError,
	AuthenticationError,
	RateLimitedError,
	ClientClosedError,
	CaptureError,
	IdentifyError,
	StorageError,
	ValidationError,
	NetworkError,
	ServerError
} from './errors';
