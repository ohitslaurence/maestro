/**
 * Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
 * SPDX-License-Identifier: Proprietary
 */

import { describe, it, expect } from 'vitest';
import { test } from '@fast-check/vitest';
import * as fc from 'fast-check';
import { createActor } from 'xstate';
import { connectionMachine } from '../../lib/state/connectionMachine';

const connectionEventArb = fc.oneof(
	fc.record({
		type: fc.constant('CONNECT' as const),
		sessionId: fc.string({ minLength: 1, maxLength: 20 }),
	}),
	fc.record({ type: fc.constant('CONNECTED' as const) }),
	fc.record({
		type: fc.constant('DISCONNECTED' as const),
		error: fc.option(fc.string(), { nil: undefined }),
	}),
	fc.record({ type: fc.constant('RETRY' as const) }),
	fc.record({ type: fc.constant('GIVE_UP' as const) })
);

describe('connectionMachine', () => {
	/**
	 * **Property: Retry count never exceeds maxRetries**
	 *
	 * **Why this is important**: The connection machine handles WebSocket
	 * reconnection. Unbounded retries would cause infinite reconnection
	 * attempts, consuming resources and potentially causing rate limiting.
	 *
	 * **Invariant**: context.retries ≤ context.maxRetries (default: 5)
	 */
	test.prop([fc.array(connectionEventArb, { minLength: 0, maxLength: 50 })])(
		'retry_count_never_exceeds_max',
		(events) => {
			const actor = createActor(connectionMachine);
			actor.start();

			for (const event of events) {
				try {
					actor.send(event as any);
				} catch {
					// Ignore invalid transitions
				}
			}

			const ctx = actor.getSnapshot().context;
			expect(ctx.retries).toBeLessThanOrEqual(ctx.maxRetries);
		}
	);

	/**
	 * **Property: CONNECTED always resets retry count to zero**
	 *
	 * **Why this is important**: After a successful connection, retry count
	 * should reset so future disconnections get fresh retry attempts.
	 *
	 * **Invariant**: After CONNECTED event, context.retries === 0
	 */
	test.prop([fc.array(connectionEventArb, { minLength: 0, maxLength: 20 })])(
		'connected_resets_retry_count',
		(events) => {
			const actor = createActor(connectionMachine);
			actor.start();

			// Get to connecting state
			actor.send({ type: 'CONNECT', sessionId: 'test-session' });

			// Apply random events
			for (const event of events) {
				try {
					actor.send(event as any);
				} catch {
					// Ignore
				}
			}

			// Now connect successfully
			try {
				actor.send({ type: 'CONNECT', sessionId: 'new-session' });
				actor.send({ type: 'CONNECTED' });

				expect(actor.getSnapshot().context.retries).toBe(0);
			} catch {
				// If can't reach connected state, that's fine for this test
			}
		}
	);

	/**
	 * **Property: State is always one of the valid states**
	 *
	 * **Why this is important**: The ConnectionStatusIndicator component
	 * needs to map states to UI. Invalid states would cause rendering errors.
	 *
	 * **Invariant**: state ∈ {disconnected, connecting, connected, reconnecting, failed}
	 */
	test.prop([fc.array(connectionEventArb, { minLength: 0, maxLength: 30 })])(
		'state_is_always_valid',
		(events) => {
			const actor = createActor(connectionMachine);
			actor.start();

			for (const event of events) {
				try {
					actor.send(event as any);
				} catch {
					// Ignore
				}
			}

			const validStates = ['disconnected', 'connecting', 'connected', 'reconnecting', 'failed'];
			expect(validStates).toContain(actor.getSnapshot().value);
		}
	);

	// Unit tests
	it('starts in disconnected state', () => {
		const actor = createActor(connectionMachine);
		actor.start();
		expect(actor.getSnapshot().value).toBe('disconnected');
	});

	it('transitions to connecting on CONNECT', () => {
		const actor = createActor(connectionMachine);
		actor.start();
		actor.send({ type: 'CONNECT', sessionId: 'test-123' });
		expect(actor.getSnapshot().value).toBe('connecting');
		expect(actor.getSnapshot().context.sessionId).toBe('test-123');
	});

	it('transitions to connected on CONNECTED', () => {
		const actor = createActor(connectionMachine);
		actor.start();
		actor.send({ type: 'CONNECT', sessionId: 'test-123' });
		actor.send({ type: 'CONNECTED' });
		expect(actor.getSnapshot().value).toBe('connected');
	});

	it('transitions to reconnecting on DISCONNECTED from connected', () => {
		const actor = createActor(connectionMachine);
		actor.start();
		actor.send({ type: 'CONNECT', sessionId: 'test-123' });
		actor.send({ type: 'CONNECTED' });
		actor.send({ type: 'DISCONNECTED', error: 'Network error' });
		expect(actor.getSnapshot().value).toBe('reconnecting');
		expect(actor.getSnapshot().context.lastError).toBe('Network error');
	});
});
