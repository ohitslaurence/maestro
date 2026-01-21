/**
 * Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
 * SPDX-License-Identifier: Proprietary
 */

import type {
	BatchConfig,
	CapturePayload,
	QueuedEvent
} from './types';
import { DEFAULT_BATCH_CONFIG } from './types';

/**
 * Interface for sending batches to the server.
 */
export interface BatchSender {
	/**
	 * Send a batch of events to the server.
	 * Returns true if successful, false otherwise.
	 */
	sendBatch(events: CapturePayload[]): Promise<boolean>;
}

/**
 * Event listener for batch processor events.
 */
export type BatchEventListener = {
	/** Called when events are flushed */
	onFlush?: (events: CapturePayload[], success: boolean) => void;
	/** Called when events are dropped due to queue overflow */
	onDrop?: (events: QueuedEvent[]) => void;
	/** Called when an error occurs */
	onError?: (error: Error) => void;
};

/**
 * Batch processor for queuing and flushing events.
 *
 * Events are queued in memory and flushed either:
 * - When the queue reaches maxBatchSize
 * - On a timer interval (flushIntervalMs)
 * - When flush() is called manually
 * - When shutdown() is called
 */
export class BatchProcessor {
	private readonly config: BatchConfig;
	private readonly sender: BatchSender;
	private readonly listener?: BatchEventListener;

	private queue: QueuedEvent[] = [];
	private flushTimer: ReturnType<typeof setInterval> | null = null;
	private isFlushing = false;
	private isShutdown = false;

	constructor(
		sender: BatchSender,
		config: Partial<BatchConfig> = {},
		listener?: BatchEventListener
	) {
		this.sender = sender;
		this.config = { ...DEFAULT_BATCH_CONFIG, ...config };
		this.listener = listener;
	}

	/**
	 * Start the background flush timer.
	 */
	start(): void {
		if (this.flushTimer !== null) {
			return; // Already started
		}

		this.flushTimer = setInterval(() => {
			this.flush().catch((error) => {
				this.listener?.onError?.(error instanceof Error ? error : new Error(String(error)));
			});
		}, this.config.flushIntervalMs);
	}

	/**
	 * Stop the background flush timer.
	 */
	stop(): void {
		if (this.flushTimer !== null) {
			clearInterval(this.flushTimer);
			this.flushTimer = null;
		}
	}

	/**
	 * Add an event to the queue.
	 */
	enqueue(payload: CapturePayload): void {
		if (this.isShutdown) {
			return;
		}

		const event: QueuedEvent = {
			payload,
			queuedAt: Date.now()
		};

		this.queue.push(event);

		// Check if we need to drop old events
		if (this.queue.length > this.config.maxQueueSize) {
			const dropped = this.queue.splice(0, this.queue.length - this.config.maxQueueSize);
			this.listener?.onDrop?.(dropped);
		}

		// Trigger immediate flush if we've reached batch size
		if (this.queue.length >= this.config.maxBatchSize) {
			this.flush().catch((error) => {
				this.listener?.onError?.(error instanceof Error ? error : new Error(String(error)));
			});
		}
	}

	/**
	 * Flush all queued events to the server.
	 */
	async flush(): Promise<void> {
		if (this.isFlushing || this.queue.length === 0) {
			return;
		}

		this.isFlushing = true;

		try {
			while (this.queue.length > 0) {
				// Take up to maxBatchSize events
				const batch = this.queue.splice(0, this.config.maxBatchSize);
				const payloads = batch.map((e) => e.payload);

				try {
					const success = await this.sender.sendBatch(payloads);
					this.listener?.onFlush?.(payloads, success);

					if (!success) {
						// Re-queue failed events at the front
						this.queue.unshift(...batch);
						break;
					}
				} catch (error) {
					// Re-queue failed events at the front
					this.queue.unshift(...batch);
					this.listener?.onError?.(error instanceof Error ? error : new Error(String(error)));
					break;
				}
			}
		} finally {
			this.isFlushing = false;
		}
	}

	/**
	 * Shutdown the processor, flushing all remaining events.
	 */
	async shutdown(): Promise<void> {
		this.isShutdown = true;
		this.stop();
		await this.flush();
	}

	/**
	 * Get the number of events currently in the queue.
	 */
	getQueueSize(): number {
		return this.queue.length;
	}

	/**
	 * Check if the processor is running.
	 */
	isRunning(): boolean {
		return this.flushTimer !== null;
	}

	/**
	 * Check if the processor has been shutdown.
	 */
	hasShutdown(): boolean {
		return this.isShutdown;
	}

	/**
	 * Clear all queued events without sending them.
	 */
	clear(): void {
		this.queue = [];
	}
}
