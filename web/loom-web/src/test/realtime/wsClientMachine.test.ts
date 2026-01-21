/**
 * Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
 * SPDX-License-Identifier: Proprietary
 */

import { describe, it, expect } from 'vitest';
import { test } from '@fast-check/vitest';
import * as fc from 'fast-check';
import { createActor } from 'xstate';
import { wsClientMachine } from '../../lib/realtime/wsClientMachine';
import type { WsClientEvent } from '../../lib/realtime/wsClientMachine.types';

const wsClientEventArb: fc.Arbitrary<WsClientEvent> = fc.oneof(
	fc.record({
		type: fc.constant('CONNECT' as const),
		sessionId: fc.string({ minLength: 1, maxLength: 20 }),
		sessionToken: fc.string({ minLength: 1, maxLength: 50 }),
	}),
	fc.record({ type: fc.constant('DISCONNECT' as const) }),
	fc.record({ type: fc.constant('SOCKET_OPENED' as const) }),
	fc.record({
		type: fc.constant('SOCKET_CLOSED' as const),
		code: fc.integer({ min: 1000, max: 4999 }),
		reason: fc.option(fc.string({ maxLength: 50 }), { nil: undefined }),
	}),
	fc.record({
		type: fc.constant('SOCKET_ERROR' as const),
		error: fc.string({ minLength: 1, maxLength: 50 }),
	}),
	fc.record({
		type: fc.constant('AUTH_OK' as const),
		userId: fc.string({ minLength: 1, maxLength: 20 }),
	}),
	fc.record({
		type: fc.constant('AUTH_ERROR' as const),
		message: fc.string({ minLength: 1, maxLength: 50 }),
	}),
	fc.record({ type: fc.constant('AUTH_TIMEOUT' as const) }),
	fc.record({ type: fc.constant('HEARTBEAT_TIMEOUT' as const) }),
	fc.record({ type: fc.constant('RETRY_DELAY_ELAPSED' as const) }),
	fc.record({
		type: fc.constant('INCOMING_MESSAGE' as const),
		message: fc.record({
			type: fc.constant('user_message' as const),
			content: fc.string(),
			timestamp: fc.string(),
		}),
	})
);

const VALID_STATES = ['idle', 'connecting', 'authenticating', 'connected', 'reconnecting', 'failed'];

describe('wsClientMachine', () => {
	/**
	 * **Property: Retry count never exceeds maxRetries**
	 *
	 * **Why this is important**: The machine handles WebSocket reconnection.
	 * Unbounded retries would cause infinite reconnection attempts, consuming
	 * resources and potentially causing rate limiting.
	 *
	 * **Invariant**: context.retries ≤ context.maxRetries
	 */
	test.prop([fc.array(wsClientEventArb, { minLength: 0, maxLength: 100 })])(
		'retry_count_never_exceeds_max',
		(events) => {
			const actor = createActor(wsClientMachine);
			actor.start();

			for (const event of events) {
				try {
					actor.send(event);
				} catch {
					// Ignore invalid transitions
				}
			}

			const ctx = actor.getSnapshot().context;
			expect(ctx.retries).toBeLessThanOrEqual(ctx.maxRetries);
		}
	);

	/**
	 * **Property: Auth failures from authenticating state never trigger reconnection**
	 *
	 * **Why this is important**: Requirement 5 - after AUTH_ERROR or AUTH_TIMEOUT
	 * when in authenticating state, we must not auto-reconnect. Auth failures
	 * indicate credential/session problems, not transient network issues.
	 *
	 * **Invariant**: If we're in authenticating and receive AUTH_ERROR/AUTH_TIMEOUT,
	 * we go to idle with retries=0, not reconnecting.
	 */
	test.prop([fc.array(wsClientEventArb, { minLength: 1, maxLength: 50 })])(
		'auth_failures_do_not_reconnect',
		(events) => {
			const actor = createActor(wsClientMachine);
			actor.start();

			for (let i = 0; i < events.length; i++) {
				const event = events[i];
				const stateBefore = actor.getSnapshot().value;

				try {
					actor.send(event);
				} catch {
					// Ignore
				}

				const stateAfter = actor.getSnapshot().value;

				// If we were in authenticating and got an auth failure, we should go to idle
				if (stateBefore === 'authenticating' && (event.type === 'AUTH_ERROR' || event.type === 'AUTH_TIMEOUT')) {
					expect(stateAfter).toBe('idle');
					expect(actor.getSnapshot().context.retries).toBe(0);
				}
			}
		}
	);

	/**
	 * **Property: Connected implies authenticated and no pending retries**
	 *
	 * **Why this is important**: We must only consider the connection "connected"
	 * after AUTH_OK. This ensures the UI doesn't show a connected state before
	 * authentication is complete.
	 *
	 * **Invariant**: If state === 'connected' then userId !== null and retries === 0
	 */
	test.prop([fc.array(wsClientEventArb, { minLength: 0, maxLength: 50 })])(
		'connected_implies_auth_ok',
		(events) => {
			const actor = createActor(wsClientMachine);
			actor.start();

			for (const event of events) {
				try {
					actor.send(event);
				} catch {
					// Ignore
				}
			}

			const snap = actor.getSnapshot();
			if (snap.value === 'connected') {
				expect(snap.context.userId).not.toBeNull();
				expect(snap.context.retries).toBe(0);
				expect(snap.context.lastError).toBeNull();
			}
		}
	);

	/**
	 * **Property: State is always one of the valid states**
	 *
	 * **Why this is important**: The UI needs to map states to ConnectionStatus.
	 * Invalid states would cause rendering errors.
	 *
	 * **Invariant**: state ∈ {idle, connecting, authenticating, connected, reconnecting, failed}
	 */
	test.prop([fc.array(wsClientEventArb, { minLength: 0, maxLength: 50 })])(
		'state_is_always_valid',
		(events) => {
			const actor = createActor(wsClientMachine);
			actor.start();

			for (const event of events) {
				try {
					actor.send(event);
				} catch {
					// Ignore
				}
			}

			expect(VALID_STATES).toContain(actor.getSnapshot().value);
		}
	);

	/**
	 * **Property: Backoff increases with retry count**
	 *
	 * **Why this is important**: Exponential backoff prevents overwhelming
	 * the server during recovery.
	 *
	 * **Invariant**: In reconnecting state, backoffMs = baseBackoffMs * 2^(retries-1)
	 */
	test.prop([fc.array(wsClientEventArb, { minLength: 0, maxLength: 50 })])(
		'backoff_grows_exponentially',
		(events) => {
			const actor = createActor(wsClientMachine);
			actor.start();

			for (const event of events) {
				try {
					actor.send(event);
				} catch {
					// Ignore
				}
			}

			const ctx = actor.getSnapshot().context;
			if (ctx.retries > 0) {
				const expectedBackoff = ctx.baseBackoffMs * Math.pow(2, ctx.retries - 1);
				expect(ctx.backoffMs).toBe(expectedBackoff);
			}
		}
	);

	/**
	 * **Property: sessionId and sessionToken are set only via CONNECT**
	 *
	 * **Why this is important**: Credentials should only change through
	 * explicit user action.
	 *
	 * **Invariant**: After any event sequence, if sessionId is set, it matches
	 * one of the CONNECT events' sessionId.
	 */
	test.prop([fc.array(wsClientEventArb, { minLength: 1, maxLength: 30 })])(
		'session_credentials_only_from_connect',
		(events) => {
			const actor = createActor(wsClientMachine);
			actor.start();

			const connectSessionIds = events
				.filter((e) => e.type === 'CONNECT')
				.map((e) => (e as { sessionId: string }).sessionId);

			for (const event of events) {
				try {
					actor.send(event);
				} catch {
					// Ignore
				}
			}

			const { sessionId } = actor.getSnapshot().context;
			if (sessionId !== null) {
				expect(connectSessionIds).toContain(sessionId);
			}
		}
	);

	// Unit tests for specific transitions
	describe('unit tests', () => {
		it('starts in idle state', () => {
			const actor = createActor(wsClientMachine);
			actor.start();
			expect(actor.getSnapshot().value).toBe('idle');
		});

		it('transitions idle → connecting on CONNECT', () => {
			const actor = createActor(wsClientMachine);
			actor.start();
			actor.send({ type: 'CONNECT', sessionId: 'sess-123', sessionToken: 'token-abc' });
			expect(actor.getSnapshot().value).toBe('connecting');
			expect(actor.getSnapshot().context.sessionId).toBe('sess-123');
			expect(actor.getSnapshot().context.sessionToken).toBe('token-abc');
		});

		it('transitions connecting → authenticating on SOCKET_OPENED', () => {
			const actor = createActor(wsClientMachine);
			actor.start();
			actor.send({ type: 'CONNECT', sessionId: 'sess-123', sessionToken: 'token-abc' });
			actor.send({ type: 'SOCKET_OPENED' });
			expect(actor.getSnapshot().value).toBe('authenticating');
		});

		it('transitions authenticating → connected on AUTH_OK', () => {
			const actor = createActor(wsClientMachine);
			actor.start();
			actor.send({ type: 'CONNECT', sessionId: 'sess-123', sessionToken: 'token-abc' });
			actor.send({ type: 'SOCKET_OPENED' });
			actor.send({ type: 'AUTH_OK', userId: 'user-456' });
			expect(actor.getSnapshot().value).toBe('connected');
			expect(actor.getSnapshot().context.userId).toBe('user-456');
		});

		it('transitions authenticating → idle on AUTH_ERROR (no reconnect)', () => {
			const actor = createActor(wsClientMachine);
			actor.start();
			actor.send({ type: 'CONNECT', sessionId: 'sess-123', sessionToken: 'token-abc' });
			actor.send({ type: 'SOCKET_OPENED' });
			actor.send({ type: 'AUTH_ERROR', message: 'Invalid token' });
			expect(actor.getSnapshot().value).toBe('idle');
			expect(actor.getSnapshot().context.retries).toBe(0);
			expect(actor.getSnapshot().context.lastError).toBe('Invalid token');
		});

		it('transitions authenticating → idle on AUTH_TIMEOUT (no reconnect)', () => {
			const actor = createActor(wsClientMachine);
			actor.start();
			actor.send({ type: 'CONNECT', sessionId: 'sess-123', sessionToken: 'token-abc' });
			actor.send({ type: 'SOCKET_OPENED' });
			actor.send({ type: 'AUTH_TIMEOUT' });
			expect(actor.getSnapshot().value).toBe('idle');
			expect(actor.getSnapshot().context.retries).toBe(0);
		});

		it('transitions connected → reconnecting on SOCKET_CLOSED', () => {
			const actor = createActor(wsClientMachine);
			actor.start();
			actor.send({ type: 'CONNECT', sessionId: 'sess-123', sessionToken: 'token-abc' });
			actor.send({ type: 'SOCKET_OPENED' });
			actor.send({ type: 'AUTH_OK', userId: 'user-456' });
			actor.send({ type: 'SOCKET_CLOSED', code: 1006, reason: 'Connection lost' });
			expect(actor.getSnapshot().value).toBe('reconnecting');
			expect(actor.getSnapshot().context.retries).toBe(1);
		});

		it('transitions connected → reconnecting on HEARTBEAT_TIMEOUT', () => {
			const actor = createActor(wsClientMachine);
			actor.start();
			actor.send({ type: 'CONNECT', sessionId: 'sess-123', sessionToken: 'token-abc' });
			actor.send({ type: 'SOCKET_OPENED' });
			actor.send({ type: 'AUTH_OK', userId: 'user-456' });
			actor.send({ type: 'HEARTBEAT_TIMEOUT' });
			expect(actor.getSnapshot().value).toBe('reconnecting');
		});

		it('transitions reconnecting → connecting on RETRY_DELAY_ELAPSED', () => {
			const actor = createActor(wsClientMachine);
			actor.start();
			actor.send({ type: 'CONNECT', sessionId: 'sess-123', sessionToken: 'token-abc' });
			actor.send({ type: 'SOCKET_OPENED' });
			actor.send({ type: 'AUTH_OK', userId: 'user-456' });
			actor.send({ type: 'SOCKET_CLOSED', code: 1006 });
			actor.send({ type: 'RETRY_DELAY_ELAPSED' });
			expect(actor.getSnapshot().value).toBe('connecting');
		});

		it('transitions reconnecting → failed after maxRetries', () => {
			const actor = createActor(wsClientMachine);
			actor.start();
			actor.send({ type: 'CONNECT', sessionId: 'sess-123', sessionToken: 'token-abc' });

			// Simulate 5 failed connection attempts
			for (let i = 0; i < 5; i++) {
				actor.send({ type: 'SOCKET_ERROR', error: 'Network error' });
				if (i < 4) {
					actor.send({ type: 'RETRY_DELAY_ELAPSED' });
				}
			}

			expect(actor.getSnapshot().value).toBe('failed');
		});

		it('can recover from failed state with new CONNECT', () => {
			const actor = createActor(wsClientMachine);
			actor.start();
			actor.send({ type: 'CONNECT', sessionId: 'sess-123', sessionToken: 'token-abc' });

			// Force into failed state
			for (let i = 0; i < 5; i++) {
				actor.send({ type: 'SOCKET_ERROR', error: 'Network error' });
				if (i < 4) {
					actor.send({ type: 'RETRY_DELAY_ELAPSED' });
				}
			}

			expect(actor.getSnapshot().value).toBe('failed');

			// Recover
			actor.send({ type: 'CONNECT', sessionId: 'sess-new', sessionToken: 'token-new' });
			expect(actor.getSnapshot().value).toBe('connecting');
			expect(actor.getSnapshot().context.retries).toBe(0);
		});

		it('DISCONNECT from any state goes to idle', () => {
			const actor = createActor(wsClientMachine);
			actor.start();
			actor.send({ type: 'CONNECT', sessionId: 'sess-123', sessionToken: 'token-abc' });
			actor.send({ type: 'SOCKET_OPENED' });
			actor.send({ type: 'AUTH_OK', userId: 'user-456' });
			expect(actor.getSnapshot().value).toBe('connected');

			actor.send({ type: 'DISCONNECT' });
			expect(actor.getSnapshot().value).toBe('idle');
		});
	});
});
