/**
 * Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
 * SPDX-License-Identifier: Proprietary
 */

/**
 * Feature Flags SDK for Loom.
 *
 * This package provides a client library for evaluating feature flags against the
 * Loom server. It supports real-time updates via SSE, local caching, and offline mode.
 *
 * @example
 * ```typescript
 * import { FlagsClient } from '@loom/flags';
 *
 * const client = new FlagsClient({
 *   sdkKey: 'loom_sdk_client_prod_xxx',
 *   baseUrl: 'https://loom.example.com',
 * });
 *
 * await client.initialize();
 *
 * // Evaluate flag
 * const enabled = await client.getBool('checkout.new_flow', context, false);
 *
 * // React to updates
 * client.on('flag.updated', (event) => {
 *   console.log(`Flag ${event.flagKey} updated`);
 * });
 * ```
 */

export { FlagsClient, type FlagsClientOptions } from './client';
export { FlagCache } from './cache';
export { SseConnection, type SseConfig } from './sse';
export {
	FlagsError,
	InvalidSdkKeyError,
	InvalidBaseUrlError,
	ConnectionError,
	AuthenticationError,
	RateLimitedError,
	InitializationTimeoutError,
	ClientClosedError,
	FlagNotFoundError
} from './errors';
export {
	type EvaluationContext,
	type EvaluationResult,
	type EvaluationReason,
	type GeoContext,
	type VariantValue,
	type FlagState,
	type KillSwitchState,
	type FlagStreamEvent,
	type InitData,
	type FlagUpdatedData,
	type FlagArchivedData,
	type FlagRestoredData,
	type KillSwitchActivatedData,
	type KillSwitchDeactivatedData,
	type HeartbeatData,
	type BulkEvaluationResult
} from './types';
