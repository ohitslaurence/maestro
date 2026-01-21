/**
 * Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
 * SPDX-License-Identifier: Proprietary
 */

import { describe, it, expect, vi } from 'vitest';
import { fc, test as fcTest } from '@fast-check/vitest';
import {
	retry,
	calculateDelay,
	isRetryableError,
	DEFAULT_RETRY_CONFIG,
	type RetryConfig
} from './retry';
import { HttpError, TimeoutError, NetworkError, RateLimitError } from './errors';

describe('calculateDelay', () => {
	it('should calculate exponential delay', () => {
		const config: RetryConfig = {
			...DEFAULT_RETRY_CONFIG,
			jitter: false,
			baseDelayMs: 100,
			backoffFactor: 2.0
		};

		expect(calculateDelay(config, 0)).toBe(100);
		expect(calculateDelay(config, 1)).toBe(200);
		expect(calculateDelay(config, 2)).toBe(400);
		expect(calculateDelay(config, 3)).toBe(800);
	});

	it('should cap delay at maxDelayMs', () => {
		const config: RetryConfig = {
			...DEFAULT_RETRY_CONFIG,
			jitter: false,
			baseDelayMs: 1000,
			backoffFactor: 10.0,
			maxDelayMs: 5000
		};

		// 1000 * 10^2 = 100000, but capped at 5000
		expect(calculateDelay(config, 2)).toBe(5000);
	});

	it('should add jitter when enabled', () => {
		const config: RetryConfig = {
			...DEFAULT_RETRY_CONFIG,
			jitter: true,
			baseDelayMs: 100
		};

		const delays = Array.from({ length: 10 }, () => calculateDelay(config, 1));

		// With jitter, not all delays should be identical
		const uniqueDelays = new Set(delays);
		expect(uniqueDelays.size).toBeGreaterThan(1);
	});

	it('should not add jitter when disabled', () => {
		const config: RetryConfig = {
			...DEFAULT_RETRY_CONFIG,
			jitter: false,
			baseDelayMs: 100
		};

		const delays = Array.from({ length: 10 }, () => calculateDelay(config, 1));

		// Without jitter, all delays should be identical
		const uniqueDelays = new Set(delays);
		expect(uniqueDelays.size).toBe(1);
	});
});

describe('isRetryableError', () => {
	it('should return true for TimeoutError', () => {
		const error = new TimeoutError('timeout', 5000);
		expect(isRetryableError(error)).toBe(true);
	});

	it('should return true for NetworkError', () => {
		const error = new NetworkError('network failed');
		expect(isRetryableError(error)).toBe(true);
	});

	it('should return true for RateLimitError', () => {
		const error = new RateLimitError('rate limited');
		expect(isRetryableError(error)).toBe(true);
	});

	it('should return true for HttpError with retryable status', () => {
		const error = new HttpError('server error', { statusCode: 503 });
		expect(isRetryableError(error)).toBe(true);
	});

	it('should return false for HttpError with non-retryable status', () => {
		const error = new HttpError('not found', { statusCode: 404 });
		expect(isRetryableError(error)).toBe(false);
	});

	it('should return false for regular Error', () => {
		const error = new Error('regular error');
		expect(isRetryableError(error)).toBe(false);
	});

	it('should return false for non-error objects', () => {
		expect(isRetryableError(null)).toBe(false);
		expect(isRetryableError(undefined)).toBe(false);
		expect(isRetryableError('string error')).toBe(false);
	});
});

describe('retry', () => {
	it('should return result on first success', async () => {
		const fn = vi.fn().mockResolvedValue('success');

		const result = await retry(fn, { maxAttempts: 3 });

		expect(result).toBe('success');
		expect(fn).toHaveBeenCalledTimes(1);
	});

	it('should retry on retryable error and succeed', async () => {
		const fn = vi
			.fn()
			.mockRejectedValueOnce(new NetworkError('network failed'))
			.mockRejectedValueOnce(new TimeoutError('timeout', 5000))
			.mockResolvedValue('success');

		const result = await retry(fn, {
			maxAttempts: 5,
			baseDelayMs: 1,
			jitter: false
		});

		expect(result).toBe('success');
		expect(fn).toHaveBeenCalledTimes(3);
	});

	it('should not retry on non-retryable error', async () => {
		const fn = vi.fn().mockRejectedValue(new HttpError('not found', { statusCode: 404 }));

		await expect(retry(fn, { maxAttempts: 3 })).rejects.toThrow('not found');
		expect(fn).toHaveBeenCalledTimes(1);
	});

	it('should throw after max attempts exhausted', async () => {
		const fn = vi.fn().mockRejectedValue(new NetworkError('network failed'));

		await expect(
			retry(fn, { maxAttempts: 3, baseDelayMs: 1, jitter: false })
		).rejects.toThrow('network failed');
		expect(fn).toHaveBeenCalledTimes(3);
	});

	it('should use default config when not provided', async () => {
		const fn = vi.fn().mockResolvedValue('success');

		await retry(fn);

		expect(fn).toHaveBeenCalledTimes(1);
	});
});

// Property-based tests
describe('calculateDelay property tests', () => {
	fcTest.prop([
		fc.integer({ min: 1, max: 1000 }),
		fc.integer({ min: 1000, max: 100000 }),
		fc.integer({ min: 0, max: 10 }),
		fc.double({ min: 1.0, max: 5.0, noNaN: true })
	])(
		'delay should never exceed maxDelayMs (with buffer for jitter)',
		(baseDelayMs, maxDelayMs, attempt, backoffFactor) => {
			const config: RetryConfig = {
				maxAttempts: 3,
				baseDelayMs,
				maxDelayMs,
				backoffFactor,
				jitter: true
			};

			const delay = calculateDelay(config, attempt);

			// With jitter factor up to 1.5, max is maxDelayMs * 1.5
			expect(delay).toBeLessThanOrEqual(maxDelayMs * 1.5);
		}
	);

	fcTest.prop([
		fc.integer({ min: 1, max: 1000 }),
		fc.integer({ min: 0, max: 10 }),
		fc.double({ min: 1.0, max: 5.0, noNaN: true })
	])('delay without jitter should be deterministic', (baseDelayMs, attempt, backoffFactor) => {
		const config: RetryConfig = {
			maxAttempts: 3,
			baseDelayMs,
			maxDelayMs: 100000,
			backoffFactor,
			jitter: false
		};

		const delay1 = calculateDelay(config, attempt);
		const delay2 = calculateDelay(config, attempt);

		expect(delay1).toBe(delay2);
	});

	fcTest.prop([fc.integer({ min: 1, max: 1000 }), fc.double({ min: 1.1, max: 5.0, noNaN: true })])(
		'delay should increase with attempt number',
		(baseDelayMs, backoffFactor) => {
			const config: RetryConfig = {
				maxAttempts: 3,
				baseDelayMs,
				maxDelayMs: 1000000,
				backoffFactor,
				jitter: false
			};

			const delay0 = calculateDelay(config, 0);
			const delay1 = calculateDelay(config, 1);
			const delay2 = calculateDelay(config, 2);

			expect(delay1).toBeGreaterThan(delay0);
			expect(delay2).toBeGreaterThan(delay1);
		}
	);
});

describe('isRetryableError property tests', () => {
	fcTest.prop([fc.integer({ min: 400, max: 599 })])('should correctly identify retryable status codes', (statusCode) => {
		const error = new HttpError('error', { statusCode });
		const result = isRetryableError(error);

		const retryableStatuses = [408, 429, 500, 502, 503, 504];
		const expectedRetryable = retryableStatuses.includes(statusCode);

		expect(result).toBe(expectedRetryable);
	});
});
