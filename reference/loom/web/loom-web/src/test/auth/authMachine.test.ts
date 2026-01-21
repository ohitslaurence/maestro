/**
 * Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
 * SPDX-License-Identifier: Proprietary
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { test } from '@fast-check/vitest';
import * as fc from 'fast-check';
import { createActor, fromPromise } from 'xstate';
import { authMachine } from '$lib/auth/authMachine';
import type { CurrentUser } from '$lib/api/types';

function createMockUser(): CurrentUser {
	return {
		id: 'user-' + Math.random().toString(36).slice(2),
		display_name: 'Test User',
		email: 'test@example.com',
		avatar_url: null,
		locale: 'en',
		global_roles: ['user'],
		created_at: new Date().toISOString(),
	};
}

function createTestMachine(options: {
	loadUserResult?: CurrentUser | Error;
	logoutResult?: void | Error;
}) {
	const { loadUserResult, logoutResult } = options;

	return authMachine.provide({
		actors: {
			loadUser: fromPromise(async () => {
				if (loadUserResult instanceof Error) {
					throw loadUserResult;
				}
				return loadUserResult ?? createMockUser();
			}),
			performLogout: fromPromise(async () => {
				if (logoutResult instanceof Error) {
					throw logoutResult;
				}
			}),
		},
	});
}

const AUTH_STATES = ['idle', 'loading', 'authenticated', 'unauthenticated', 'loggingOut'] as const;

const authEventArb = fc.oneof(
	fc.record({ type: fc.constant('INIT' as const) }),
	fc.record({ type: fc.constant('REFRESH' as const) }),
	fc.record({ type: fc.constant('LOGOUT' as const) }),
	fc.record({
		type: fc.constant('LOGIN_SUCCESS' as const),
		user: fc.record({
			id: fc.string({ minLength: 1, maxLength: 20 }),
			display_name: fc.string({ minLength: 1, maxLength: 50 }),
			email: fc.option(fc.emailAddress(), { nil: null }),
			avatar_url: fc.constant(null),
			locale: fc.constant('en'),
			global_roles: fc.constant(['user']),
			created_at: fc.constant(new Date().toISOString()),
		}),
	}),
	fc.record({
		type: fc.constant('LOGIN_FAILURE' as const),
		error: fc.string({ minLength: 1, maxLength: 50 }),
	})
);

describe('authMachine', () => {
	/**
	 * **Property: Machine is always in a valid state**
	 *
	 * **Why this is important**: The UI relies on state matching to determine
	 * what to render. Invalid states would cause undefined behavior.
	 *
	 * **Invariant**: ∀ event sequences E, machine.value ∈ AUTH_STATES
	 */
	test.prop([fc.array(authEventArb, { minLength: 0, maxLength: 20 })])(
		'machine_is_always_in_valid_state',
		(events) => {
			const machine = createTestMachine({ loadUserResult: createMockUser() });
			const actor = createActor(machine);
			actor.start();

			for (const event of events) {
				try {
					actor.send(event as any);
				} catch {
					// Some events may be invalid for current state
				}
			}

			const state = actor.getSnapshot().value;
			expect(AUTH_STATES).toContain(state);
		}
	);

	/**
	 * **Property: User is non-null only in authenticated state**
	 *
	 * **Why this is important**: UI components check isAuthenticated to gate
	 * access. If user is set in wrong states, access control breaks.
	 *
	 * **Invariant**: context.user !== null ⟺ state === 'authenticated'
	 */
	test.prop([fc.array(authEventArb, { minLength: 0, maxLength: 15 })])(
		'user_only_set_when_authenticated',
		async (events) => {
			const mockUser = createMockUser();
			const machine = createTestMachine({ loadUserResult: mockUser });
			const actor = createActor(machine);
			actor.start();

			for (const event of events) {
				try {
					actor.send(event as any);
					await new Promise((r) => setTimeout(r, 1));
				} catch {
					// Ignore invalid transitions
				}
			}

			const snapshot = actor.getSnapshot();
			if (snapshot.value === 'authenticated') {
				expect(snapshot.context.user).not.toBeNull();
			}
			if (snapshot.context.user !== null) {
				expect(snapshot.value).toBe('authenticated');
			}
		}
	);

	// Unit tests
	it('starts in idle state', () => {
		const machine = createTestMachine({});
		const actor = createActor(machine);
		actor.start();
		expect(actor.getSnapshot().value).toBe('idle');
	});

	it('has null user and error in initial context', () => {
		const machine = createTestMachine({});
		const actor = createActor(machine);
		actor.start();
		const ctx = actor.getSnapshot().context;
		expect(ctx.user).toBeNull();
		expect(ctx.error).toBeNull();
	});

	it('transitions from idle to loading on INIT', () => {
		const machine = createTestMachine({});
		const actor = createActor(machine);
		actor.start();
		actor.send({ type: 'INIT' });
		expect(actor.getSnapshot().value).toBe('loading');
	});

	it('transitions to authenticated on successful user load', async () => {
		const mockUser = createMockUser();
		const machine = createTestMachine({ loadUserResult: mockUser });
		const actor = createActor(machine);
		actor.start();
		actor.send({ type: 'INIT' });

		await vi.waitFor(() => {
			expect(actor.getSnapshot().value).toBe('authenticated');
		});

		expect(actor.getSnapshot().context.user).toEqual(mockUser);
	});

	it('transitions to unauthenticated on failed user load', async () => {
		const machine = createTestMachine({ loadUserResult: new Error('Unauthorized') });
		const actor = createActor(machine);
		actor.start();
		actor.send({ type: 'INIT' });

		await vi.waitFor(() => {
			expect(actor.getSnapshot().value).toBe('unauthenticated');
		});

		expect(actor.getSnapshot().context.user).toBeNull();
	});

	it('transitions from authenticated to loggingOut on LOGOUT', async () => {
		const mockUser = createMockUser();
		const machine = createTestMachine({ loadUserResult: mockUser });
		const actor = createActor(machine);
		actor.start();
		actor.send({ type: 'INIT' });

		await vi.waitFor(() => {
			expect(actor.getSnapshot().value).toBe('authenticated');
		});

		actor.send({ type: 'LOGOUT' });
		expect(actor.getSnapshot().value).toBe('loggingOut');
	});

	it('transitions from loggingOut to unauthenticated after logout completes', async () => {
		const mockUser = createMockUser();
		const machine = createTestMachine({ loadUserResult: mockUser });
		const actor = createActor(machine);
		actor.start();
		actor.send({ type: 'INIT' });

		await vi.waitFor(() => {
			expect(actor.getSnapshot().value).toBe('authenticated');
		});

		actor.send({ type: 'LOGOUT' });

		await vi.waitFor(() => {
			expect(actor.getSnapshot().value).toBe('unauthenticated');
		});

		expect(actor.getSnapshot().context.user).toBeNull();
	});

	it('transitions from authenticated to loading on REFRESH', async () => {
		const mockUser = createMockUser();
		const machine = createTestMachine({ loadUserResult: mockUser });
		const actor = createActor(machine);
		actor.start();
		actor.send({ type: 'INIT' });

		await vi.waitFor(() => {
			expect(actor.getSnapshot().value).toBe('authenticated');
		});

		actor.send({ type: 'REFRESH' });
		expect(actor.getSnapshot().value).toBe('loading');
	});

	it('transitions from unauthenticated to authenticated on LOGIN_SUCCESS', async () => {
		const machine = createTestMachine({ loadUserResult: new Error('Unauthorized') });
		const actor = createActor(machine);
		actor.start();
		actor.send({ type: 'INIT' });

		await vi.waitFor(() => {
			expect(actor.getSnapshot().value).toBe('unauthenticated');
		});

		const newUser = createMockUser();
		actor.send({ type: 'LOGIN_SUCCESS', user: newUser });

		expect(actor.getSnapshot().value).toBe('authenticated');
		expect(actor.getSnapshot().context.user).toEqual(newUser);
	});

	it('transitions from unauthenticated to loading on REFRESH', async () => {
		const machine = createTestMachine({ loadUserResult: new Error('Unauthorized') });
		const actor = createActor(machine);
		actor.start();
		actor.send({ type: 'INIT' });

		await vi.waitFor(() => {
			expect(actor.getSnapshot().value).toBe('unauthenticated');
		});

		actor.send({ type: 'REFRESH' });
		expect(actor.getSnapshot().value).toBe('loading');
	});

	it('clears user on logout even if logout API fails', async () => {
		const mockUser = createMockUser();
		const machine = createTestMachine({
			loadUserResult: mockUser,
			logoutResult: new Error('Network error'),
		});
		const actor = createActor(machine);
		actor.start();
		actor.send({ type: 'INIT' });

		await vi.waitFor(() => {
			expect(actor.getSnapshot().value).toBe('authenticated');
		});

		actor.send({ type: 'LOGOUT' });

		await vi.waitFor(() => {
			expect(actor.getSnapshot().value).toBe('unauthenticated');
		});

		expect(actor.getSnapshot().context.user).toBeNull();
	});
});
