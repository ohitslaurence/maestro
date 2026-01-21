/**
 * Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
 * SPDX-License-Identifier: Proprietary
 */

import { describe, it, expect } from 'vitest';
import {
	DEFAULT_BATCH_CONFIG,
	DEFAULT_AUTOCAPTURE_CONFIG,
	COOKIE_SETTINGS,
	LOCALSTORAGE_KEY,
	SDK_VERSION,
	SDK_NAME
} from './types';

describe('Default configurations', () => {
	it('should have valid DEFAULT_BATCH_CONFIG', () => {
		expect(DEFAULT_BATCH_CONFIG.flushIntervalMs).toBe(10000);
		expect(DEFAULT_BATCH_CONFIG.maxBatchSize).toBe(10);
		expect(DEFAULT_BATCH_CONFIG.maxQueueSize).toBe(1000);
	});

	it('should have valid DEFAULT_AUTOCAPTURE_CONFIG', () => {
		expect(DEFAULT_AUTOCAPTURE_CONFIG.pageview).toBe(true);
		expect(DEFAULT_AUTOCAPTURE_CONFIG.pageleave).toBe(true);
	});

	it('should have valid COOKIE_SETTINGS', () => {
		expect(COOKIE_SETTINGS.defaultName).toBe('loom_analytics_distinct_id');
		expect(COOKIE_SETTINGS.maxAge).toBe(365 * 24 * 60 * 60);
		expect(COOKIE_SETTINGS.sameSite).toBe('Lax');
		expect(COOKIE_SETTINGS.path).toBe('/');
	});

	it('should have valid LOCALSTORAGE_KEY', () => {
		expect(LOCALSTORAGE_KEY).toBe('loom_analytics_distinct_id');
	});

	it('should have valid SDK constants', () => {
		expect(SDK_VERSION).toBe('0.1.0');
		expect(SDK_NAME).toBe('@loom/analytics');
	});
});
