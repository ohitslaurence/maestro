/**
 * Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
 * SPDX-License-Identifier: Proprietary
 */

import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { fc, test as fcTest } from '@fast-check/vitest';
import { BatchProcessor, type BatchSender } from './batch';
import type { CapturePayload } from './types';

/**
 * Mock batch sender for testing.
 */
class MockBatchSender implements BatchSender {
	batches: CapturePayload[][] = [];
	shouldSucceed = true;
	delay = 0;

	async sendBatch(events: CapturePayload[]): Promise<boolean> {
		if (this.delay > 0) {
			await new Promise((resolve) => setTimeout(resolve, this.delay));
		}
		this.batches.push([...events]);
		return this.shouldSucceed;
	}

	clear(): void {
		this.batches = [];
	}
}

/**
 * Create a test capture payload.
 */
function createPayload(event: string): CapturePayload {
	return {
		distinct_id: 'user123',
		event,
		properties: {},
		timestamp: new Date().toISOString()
	};
}

describe('BatchProcessor', () => {
	let sender: MockBatchSender;

	beforeEach(() => {
		vi.useFakeTimers();
		sender = new MockBatchSender();
	});

	afterEach(() => {
		vi.useRealTimers();
	});

	it('should enqueue events', () => {
		const processor = new BatchProcessor(sender, { maxBatchSize: 10 });

		processor.enqueue(createPayload('event1'));
		processor.enqueue(createPayload('event2'));

		expect(processor.getQueueSize()).toBe(2);
	});

	it('should flush on maxBatchSize', async () => {
		const processor = new BatchProcessor(sender, { maxBatchSize: 3 });

		processor.enqueue(createPayload('event1'));
		processor.enqueue(createPayload('event2'));
		processor.enqueue(createPayload('event3'));

		// Allow flush to complete
		await vi.runAllTimersAsync();

		expect(sender.batches.length).toBe(1);
		expect(sender.batches[0].length).toBe(3);
	});

	it('should flush on timer interval', async () => {
		const processor = new BatchProcessor(sender, {
			maxBatchSize: 100,
			flushIntervalMs: 1000
		});

		processor.start();
		processor.enqueue(createPayload('event1'));

		// Advance timer
		await vi.advanceTimersByTimeAsync(1000);

		expect(sender.batches.length).toBe(1);
		expect(sender.batches[0].length).toBe(1);

		processor.stop();
	});

	it('should flush manually', async () => {
		const processor = new BatchProcessor(sender, { maxBatchSize: 100 });

		processor.enqueue(createPayload('event1'));
		processor.enqueue(createPayload('event2'));

		await processor.flush();

		expect(sender.batches.length).toBe(1);
		expect(sender.batches[0].length).toBe(2);
	});

	it('should handle flush failure by re-queuing', async () => {
		sender.shouldSucceed = false;
		const processor = new BatchProcessor(sender, { maxBatchSize: 100 });

		processor.enqueue(createPayload('event1'));
		await processor.flush();

		// Events should still be in queue after failure
		expect(processor.getQueueSize()).toBe(1);
	});

	it('should drop oldest events on queue overflow', () => {
		const droppedEvents: unknown[] = [];
		const processor = new BatchProcessor(
			sender,
			{ maxQueueSize: 3, maxBatchSize: 100 },
			{
				onDrop: (events) => droppedEvents.push(...events)
			}
		);

		processor.enqueue(createPayload('event1'));
		processor.enqueue(createPayload('event2'));
		processor.enqueue(createPayload('event3'));
		processor.enqueue(createPayload('event4')); // This should cause event1 to be dropped

		expect(processor.getQueueSize()).toBe(3);
		expect(droppedEvents.length).toBe(1);
	});

	it('should shutdown gracefully', async () => {
		const processor = new BatchProcessor(sender, { maxBatchSize: 100 });
		processor.start();

		processor.enqueue(createPayload('event1'));
		processor.enqueue(createPayload('event2'));

		await processor.shutdown();

		expect(processor.hasShutdown()).toBe(true);
		expect(processor.isRunning()).toBe(false);
		expect(sender.batches.length).toBe(1);
		expect(sender.batches[0].length).toBe(2);
	});

	it('should not enqueue after shutdown', async () => {
		const processor = new BatchProcessor(sender, { maxBatchSize: 100 });
		await processor.shutdown();

		processor.enqueue(createPayload('event1'));
		expect(processor.getQueueSize()).toBe(0);
	});

	it('should clear the queue', () => {
		const processor = new BatchProcessor(sender, { maxBatchSize: 100 });

		processor.enqueue(createPayload('event1'));
		processor.enqueue(createPayload('event2'));
		processor.clear();

		expect(processor.getQueueSize()).toBe(0);
	});

	it('should handle empty flush', async () => {
		const processor = new BatchProcessor(sender, { maxBatchSize: 100 });
		await processor.flush();
		expect(sender.batches.length).toBe(0);
	});

	it('should batch events up to maxBatchSize', async () => {
		const processor = new BatchProcessor(sender, { maxBatchSize: 2 });

		// Enqueue 5 events
		for (let i = 0; i < 5; i++) {
			processor.enqueue(createPayload(`event${i}`));
		}

		// Allow all flushes to complete
		await vi.runAllTimersAsync();
		await processor.flush();

		// Should have 3 batches: [2, 2, 1]
		expect(sender.batches.length).toBe(3);
		expect(sender.batches[0].length).toBe(2);
		expect(sender.batches[1].length).toBe(2);
		expect(sender.batches[2].length).toBe(1);
	});

	fcTest.prop([fc.array(fc.string({ minLength: 1, maxLength: 50 }), { minLength: 1, maxLength: 20 })])(
		'should process all enqueued events',
		async (eventNames) => {
			vi.useRealTimers(); // Use real timers for this test

			const testSender = new MockBatchSender();
			const processor = new BatchProcessor(testSender, { maxBatchSize: 100 });

			for (const name of eventNames) {
				processor.enqueue(createPayload(name));
			}

			await processor.flush();

			const totalEvents = testSender.batches.reduce((sum, batch) => sum + batch.length, 0);
			expect(totalEvents).toBe(eventNames.length);
		}
	);
});
