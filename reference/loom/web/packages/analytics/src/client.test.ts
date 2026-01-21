/**
 * Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
 * SPDX-License-Identifier: Proprietary
 */

import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { AnalyticsClient } from './client';
import { InvalidApiKeyError, InvalidBaseUrlError, ClientClosedError } from './errors';

// Mock fetch globally
const mockFetch = vi.fn();
global.fetch = mockFetch;

describe('AnalyticsClient', () => {
	beforeEach(() => {
		vi.useFakeTimers();
		mockFetch.mockReset();
		mockFetch.mockResolvedValue({
			ok: true,
			status: 200,
			json: async () => ({ success: true })
		});
	});

	afterEach(() => {
		vi.useRealTimers();
	});

	describe('constructor', () => {
		it('should throw InvalidApiKeyError for missing API key', () => {
			expect(
				() =>
					new AnalyticsClient({
						apiKey: '',
						baseUrl: 'https://example.com'
					})
			).toThrow(InvalidApiKeyError);
		});

		it('should throw InvalidApiKeyError for invalid API key format', () => {
			expect(
				() =>
					new AnalyticsClient({
						apiKey: 'invalid_key',
						baseUrl: 'https://example.com'
					})
			).toThrow(InvalidApiKeyError);
		});

		it('should accept valid write API key', () => {
			const client = new AnalyticsClient({
				apiKey: 'loom_analytics_write_test123',
				baseUrl: 'https://example.com',
				persistence: 'memory'
			});
			expect(client).toBeInstanceOf(AnalyticsClient);
		});

		it('should accept valid read-write API key', () => {
			const client = new AnalyticsClient({
				apiKey: 'loom_analytics_rw_test123',
				baseUrl: 'https://example.com',
				persistence: 'memory'
			});
			expect(client).toBeInstanceOf(AnalyticsClient);
		});

		it('should throw InvalidBaseUrlError for invalid base URL', () => {
			expect(
				() =>
					new AnalyticsClient({
						apiKey: 'loom_analytics_write_test123',
						baseUrl: 'not-a-url'
					})
			).toThrow(InvalidBaseUrlError);
		});

		it('should throw InvalidBaseUrlError for missing base URL', () => {
			expect(
				() =>
					new AnalyticsClient({
						apiKey: 'loom_analytics_write_test123',
						baseUrl: ''
					})
			).toThrow(InvalidBaseUrlError);
		});
	});

	describe('capture', () => {
		it('should enqueue events for batch processing', async () => {
			const client = new AnalyticsClient({
				apiKey: 'loom_analytics_write_test123',
				baseUrl: 'https://example.com',
				persistence: 'memory',
				autocapture: false
			});

			client.capture('button_clicked', { button_name: 'checkout' });

			expect(client.getQueueSize()).toBe(1);

			await client.shutdown();
		});

		it('should not capture events after shutdown', async () => {
			const client = new AnalyticsClient({
				apiKey: 'loom_analytics_write_test123',
				baseUrl: 'https://example.com',
				persistence: 'memory',
				autocapture: false
			});

			await client.shutdown();
			client.capture('button_clicked');

			expect(client.getQueueSize()).toBe(0);
		});

		it('should include SDK info in event properties', async () => {
			vi.useRealTimers(); // Use real timers for this test

			const client = new AnalyticsClient({
				apiKey: 'loom_analytics_write_test123',
				baseUrl: 'https://example.com',
				persistence: 'memory',
				autocapture: false,
				batch: { maxBatchSize: 1 } // Flush immediately
			});

			client.capture('test_event', { custom: 'prop' });

			// Wait a bit for the flush
			await new Promise((resolve) => setTimeout(resolve, 50));

			// Check that fetch was called with the batch
			expect(mockFetch).toHaveBeenCalled();
			const [, options] = mockFetch.mock.calls[0];
			const body = JSON.parse(options.body);

			expect(body.batch[0].properties.$lib).toBe('@loom/analytics');
			expect(body.batch[0].properties.$lib_version).toBe('0.1.0');
			expect(body.batch[0].properties.custom).toBe('prop');

			await client.shutdown();
		});
	});

	describe('getDistinctId', () => {
		it('should return a valid distinct_id', () => {
			const client = new AnalyticsClient({
				apiKey: 'loom_analytics_write_test123',
				baseUrl: 'https://example.com',
				persistence: 'memory',
				autocapture: false
			});

			const distinctId = client.getDistinctId();

			// Should be a UUIDv7
			expect(distinctId).toMatch(
				/^[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i
			);
		});

		it('should return the same distinct_id on subsequent calls', () => {
			const client = new AnalyticsClient({
				apiKey: 'loom_analytics_write_test123',
				baseUrl: 'https://example.com',
				persistence: 'memory',
				autocapture: false
			});

			const id1 = client.getDistinctId();
			const id2 = client.getDistinctId();

			expect(id1).toBe(id2);
		});
	});

	describe('identify', () => {
		it('should send identify request', async () => {
			const client = new AnalyticsClient({
				apiKey: 'loom_analytics_write_test123',
				baseUrl: 'https://example.com',
				persistence: 'memory',
				autocapture: false
			});

			const oldDistinctId = client.getDistinctId();
			await client.identify('user@example.com', { plan: 'pro' });

			// Check that identify was called
			expect(mockFetch).toHaveBeenCalledWith(
				'https://example.com/api/analytics/identify',
				expect.objectContaining({
					method: 'POST'
				})
			);

			// Distinct ID should now be the user ID
			expect(client.getDistinctId()).toBe('user@example.com');
			expect(client.getDistinctId()).not.toBe(oldDistinctId);

			await client.shutdown();
		});

		it('should throw ClientClosedError after shutdown', async () => {
			const client = new AnalyticsClient({
				apiKey: 'loom_analytics_write_test123',
				baseUrl: 'https://example.com',
				persistence: 'memory',
				autocapture: false
			});

			await client.shutdown();

			await expect(client.identify('user@example.com')).rejects.toThrow(ClientClosedError);
		});
	});

	describe('alias', () => {
		it('should send alias request', async () => {
			const client = new AnalyticsClient({
				apiKey: 'loom_analytics_write_test123',
				baseUrl: 'https://example.com',
				persistence: 'memory',
				autocapture: false
			});

			await client.alias('other_id');

			expect(mockFetch).toHaveBeenCalledWith(
				'https://example.com/api/analytics/alias',
				expect.objectContaining({
					method: 'POST'
				})
			);

			await client.shutdown();
		});
	});

	describe('set', () => {
		it('should send set properties request', async () => {
			const client = new AnalyticsClient({
				apiKey: 'loom_analytics_write_test123',
				baseUrl: 'https://example.com',
				persistence: 'memory',
				autocapture: false
			});

			await client.set({ plan: 'enterprise' });

			expect(mockFetch).toHaveBeenCalledWith(
				'https://example.com/api/analytics/set',
				expect.objectContaining({
					method: 'POST'
				})
			);

			await client.shutdown();
		});
	});

	describe('reset', () => {
		it('should generate a new distinct_id', async () => {
			const client = new AnalyticsClient({
				apiKey: 'loom_analytics_write_test123',
				baseUrl: 'https://example.com',
				persistence: 'memory',
				autocapture: false
			});

			const oldDistinctId = client.getDistinctId();
			client.reset();
			const newDistinctId = client.getDistinctId();

			expect(newDistinctId).not.toBe(oldDistinctId);
			// Should be a new UUIDv7
			expect(newDistinctId).toMatch(
				/^[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i
			);

			await client.shutdown();
		});
	});

	describe('flush', () => {
		it('should manually flush the queue', async () => {
			const client = new AnalyticsClient({
				apiKey: 'loom_analytics_write_test123',
				baseUrl: 'https://example.com',
				persistence: 'memory',
				autocapture: false,
				batch: { maxBatchSize: 100 } // Prevent auto-flush on size
			});

			client.capture('event1');
			client.capture('event2');

			expect(client.getQueueSize()).toBe(2);

			await client.flush();

			expect(mockFetch).toHaveBeenCalled();

			await client.shutdown();
		});
	});

	describe('shutdown', () => {
		it('should flush remaining events', async () => {
			const client = new AnalyticsClient({
				apiKey: 'loom_analytics_write_test123',
				baseUrl: 'https://example.com',
				persistence: 'memory',
				autocapture: false,
				batch: { maxBatchSize: 100 }
			});

			client.capture('event1');
			client.capture('event2');

			await client.shutdown();

			expect(mockFetch).toHaveBeenCalled();
			expect(client.isClosed_()).toBe(true);
		});

		it('should be idempotent', async () => {
			const client = new AnalyticsClient({
				apiKey: 'loom_analytics_write_test123',
				baseUrl: 'https://example.com',
				persistence: 'memory',
				autocapture: false
			});

			await client.shutdown();
			await client.shutdown(); // Should not throw

			expect(client.isClosed_()).toBe(true);
		});
	});

	describe('sendBatch', () => {
		it('should send batch to server', async () => {
			const client = new AnalyticsClient({
				apiKey: 'loom_analytics_write_test123',
				baseUrl: 'https://example.com',
				persistence: 'memory',
				autocapture: false
			});

			const success = await client.sendBatch([
				{
					distinct_id: 'user123',
					event: 'test_event',
					properties: {},
					timestamp: new Date().toISOString()
				}
			]);

			expect(success).toBe(true);
			expect(mockFetch).toHaveBeenCalledWith(
				'https://example.com/api/analytics/batch',
				expect.anything()
			);

			await client.shutdown();
		});

		it('should return false on failure', async () => {
			vi.useRealTimers(); // Use real timers for this test
			// Reject all calls (since HTTP client has retry logic)
			mockFetch.mockRejectedValue(new Error('Network error'));

			const client = new AnalyticsClient({
				apiKey: 'loom_analytics_write_test123',
				baseUrl: 'https://example.com',
				persistence: 'memory',
				autocapture: false
			});

			const success = await client.sendBatch([
				{
					distinct_id: 'user123',
					event: 'test_event',
					properties: {},
					timestamp: new Date().toISOString()
				}
			]);

			expect(success).toBe(false);

			await client.shutdown();

			// Reset mock for other tests
			mockFetch.mockReset();
			mockFetch.mockResolvedValue({
				ok: true,
				status: 200,
				json: async () => ({ success: true })
			});
		});

		it('should return true for empty batch', async () => {
			const client = new AnalyticsClient({
				apiKey: 'loom_analytics_write_test123',
				baseUrl: 'https://example.com',
				persistence: 'memory',
				autocapture: false
			});

			const success = await client.sendBatch([]);

			expect(success).toBe(true);
			expect(mockFetch).not.toHaveBeenCalled();

			await client.shutdown();
		});
	});
});
