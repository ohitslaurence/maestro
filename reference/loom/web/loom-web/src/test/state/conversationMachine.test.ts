/**
 * Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
 * SPDX-License-Identifier: Proprietary
 */

import { describe, it, expect } from 'vitest';
import { test } from '@fast-check/vitest';
import * as fc from 'fast-check';
import { createActor } from 'xstate';
import { conversationMachine } from '../../lib/state/conversationMachine';
import type { Thread, AgentStateKind } from '../../lib/api/types';

const AGENT_STATE_KINDS: AgentStateKind[] = [
	'idle',
	'thinking',
	'streaming',
	'tool_pending',
	'tool_executing',
	'waiting_input',
	'error',
];

function createMockThread(): Thread {
	return {
		id: 'T-test-' + Math.random().toString(36).slice(2),
		title: null,
		created_at: new Date().toISOString(),
		updated_at: new Date().toISOString(),
		message_count: 0,
		metadata: {},
	};
}

const conversationEventArb = fc.oneof(
	fc.record({
		type: fc.constant('USER_INPUT' as const),
		content: fc.string({ minLength: 1, maxLength: 100 }),
	}),
	fc.record({
		type: fc.constant('LLM_TEXT_DELTA' as const),
		content: fc.string({ minLength: 1, maxLength: 50 }),
	}),
	fc.record({ type: fc.constant('LLM_COMPLETED' as const), response: fc.constant({}) }),
	fc.record({
		type: fc.constant('LLM_ERROR' as const),
		error: fc.string({ minLength: 1, maxLength: 50 }),
	}),
	fc.record({ type: fc.constant('RETRY' as const) }),
	fc.record({ type: fc.constant('SHUTDOWN_REQUESTED' as const) })
);

describe('conversationMachine', () => {
	/**
	 * **Property: Agent state is always a valid AgentStateKind**
	 *
	 * **Why this is important**: The UI displays agent state to users. If an
	 * invalid state is reached, the AgentStateBadge component will fail to
	 * render correctly, leaving users confused about what the agent is doing.
	 *
	 * **Invariant**: ∀ event sequences E, context.currentAgentState ∈ AgentStateKind
	 */
	test.prop([fc.array(conversationEventArb, { minLength: 0, maxLength: 30 })])(
		'agent_state_is_always_valid',
		(events) => {
			const actor = createActor(conversationMachine);
			actor.start();

			// Load a thread first
			actor.send({ type: 'LOAD_THREAD', threadId: 'T-test' });
			actor.send({ type: 'THREAD_LOADED', thread: createMockThread() });

			// Apply random events
			for (const event of events) {
				try {
					actor.send(event as any);
				} catch {
					// Some events may be invalid for current state, that's ok
				}
			}

			const state = actor.getSnapshot().context.currentAgentState;
			expect(AGENT_STATE_KINDS).toContain(state);
		}
	);

	/**
	 * **Property: SHUTDOWN_REQUESTED always transitions to shuttingDown**
	 *
	 * **Why this is important**: Users must be able to gracefully exit at any
	 * point in the conversation. If shutdown can be blocked, users may be
	 * stuck with an unresponsive UI.
	 *
	 * **Invariant**: ∀ reachable loaded states s, after SHUTDOWN_REQUESTED,
	 * machine state is 'shuttingDown' and context.currentAgentState is 'idle'
	 */
	test.prop([fc.array(conversationEventArb, { minLength: 0, maxLength: 20 })])(
		'shutdown_always_succeeds_from_loaded_state',
		(events) => {
			const actor = createActor(conversationMachine);
			actor.start();

			actor.send({ type: 'LOAD_THREAD', threadId: 'T-test' });
			actor.send({ type: 'THREAD_LOADED', thread: createMockThread() });

			// Apply random events
			for (const event of events) {
				try {
					actor.send(event as any);
				} catch {
					// Ignore invalid transitions
				}
			}

			// Now send shutdown
			actor.send({ type: 'SHUTDOWN_REQUESTED' });

			const snapshot = actor.getSnapshot();
			expect(snapshot.value).toBe('shuttingDown');
			expect(snapshot.context.currentAgentState).toBe('idle');
			}
	);

	/**
	 * **Property: Retry count is bounded by maxRetries**
	 *
	 * **Why this is important**: Unbounded retries cause infinite loops and
	 * resource exhaustion. This mirrors the backend's retry bound invariant
	 * from loom-core's AgentConfig.
	 *
	 * **Invariant**: context.retries ≤ context.maxRetries (default: 3)
	 */
	test.prop([fc.integer({ min: 5, max: 20 })])('retry_count_is_bounded', (errorCount) => {
		const actor = createActor(conversationMachine);
		actor.start();

		actor.send({ type: 'LOAD_THREAD', threadId: 'T-test' });
		actor.send({ type: 'THREAD_LOADED', thread: createMockThread() });
		actor.send({ type: 'USER_INPUT', content: 'test' });

		// Send many errors followed by retries
		for (let i = 0; i < errorCount; i++) {
			actor.send({ type: 'LLM_ERROR', error: 'test error' });
			actor.send({ type: 'RETRY' });
		}

		const ctx = actor.getSnapshot().context;
		expect(ctx.retries).toBeLessThanOrEqual(ctx.maxRetries);
	});

	/**
	 * **Property: Messages array only grows (never loses messages)**
	 *
	 * **Why this is important**: Message history is critical for user context.
	 * If messages are ever lost during state transitions, users lose their
	 * conversation history.
	 *
	 * **Invariant**: |messages after events| >= |messages before events|
	 */
	test.prop([
		fc.array(
			fc.oneof(
				fc.record({
					type: fc.constant('USER_INPUT' as const),
					content: fc.string({ minLength: 1, maxLength: 50 }),
				}),
				fc.record({
					type: fc.constant('LLM_TEXT_DELTA' as const),
					content: fc.string({ minLength: 1, maxLength: 50 }),
				}),
				fc.record({ type: fc.constant('LLM_COMPLETED' as const), response: fc.constant({}) })
			),
			{ minLength: 1, maxLength: 10 }
		),
	])('messages_array_only_grows', (events) => {
		const actor = createActor(conversationMachine);
		actor.start();

		actor.send({ type: 'LOAD_THREAD', threadId: 'T-test' });
		actor.send({ type: 'THREAD_LOADED', thread: createMockThread() });

		let prevMessageCount = actor.getSnapshot().context.messages.length;

		for (const event of events) {
			try {
				actor.send(event as any);
			} catch {
				// Ignore
			}
			const currentCount = actor.getSnapshot().context.messages.length;
			expect(currentCount).toBeGreaterThanOrEqual(prevMessageCount);
			prevMessageCount = currentCount;
		}
	});

	// Unit tests
	it('starts in idle state', () => {
		const actor = createActor(conversationMachine);
		actor.start();
		expect(actor.getSnapshot().value).toBe('idle');
	});

	it('transitions to loading on LOAD_THREAD', () => {
		const actor = createActor(conversationMachine);
		actor.start();
		actor.send({ type: 'LOAD_THREAD', threadId: 'T-123' });
		expect(actor.getSnapshot().value).toBe('loading');
	});

	it('transitions to loaded.waitingForUserInput on THREAD_LOADED', () => {
		const actor = createActor(conversationMachine);
		actor.start();
		actor.send({ type: 'LOAD_THREAD', threadId: 'T-123' });
		actor.send({ type: 'THREAD_LOADED', thread: createMockThread() });

		const snapshot = actor.getSnapshot();
		expect(snapshot.value).toEqual({ loaded: 'waitingForUserInput' });
		expect(snapshot.context.currentAgentState).toBe('waiting_input');
	});
});
