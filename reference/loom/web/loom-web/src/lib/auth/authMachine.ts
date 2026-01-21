/**
 * Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
 * SPDX-License-Identifier: Proprietary
 */

import { setup, assign, fromPromise } from 'xstate';
import type { CurrentUser } from '$lib/api/types';
import { getApiClient } from '$lib/api/client';

export interface AuthContext {
	user: CurrentUser | null;
	error: string | null;
}

export type AuthEvent =
	| { type: 'INIT' }
	| { type: 'REFRESH' }
	| { type: 'LOGOUT' }
	| { type: 'LOGIN_SUCCESS'; user: CurrentUser }
	| { type: 'LOGIN_FAILURE'; error: string };

export const authMachine = setup({
	types: {
		context: {} as AuthContext,
		events: {} as AuthEvent,
	},
	actors: {
		loadUser: fromPromise(async () => {
			const client = getApiClient();
			return client.getCurrentUser();
		}),
		performLogout: fromPromise(async () => {
			const client = getApiClient();
			await client.logout();
		}),
	},
}).createMachine({
	id: 'auth',
	initial: 'idle',
	context: {
		user: null,
		error: null,
	},
	states: {
		idle: {
			on: {
				INIT: 'loading',
			},
		},
		loading: {
			invoke: {
				id: 'loadUser',
				src: 'loadUser',
				onDone: {
					target: 'authenticated',
					actions: assign({
						user: ({ event }) => event.output,
						error: () => null,
					}),
				},
				onError: {
					target: 'unauthenticated',
					actions: assign({
						user: () => null,
						error: () => null,
					}),
				},
			},
		},
		authenticated: {
			on: {
				LOGOUT: 'loggingOut',
				REFRESH: 'loading',
			},
		},
		unauthenticated: {
			on: {
				REFRESH: 'loading',
				LOGIN_SUCCESS: {
					target: 'authenticated',
					actions: assign({
						user: ({ event }) => event.user,
						error: () => null,
					}),
				},
			},
		},
		loggingOut: {
			invoke: {
				id: 'performLogout',
				src: 'performLogout',
				onDone: {
					target: 'unauthenticated',
					actions: assign({
						user: () => null,
						error: () => null,
					}),
				},
				onError: {
					target: 'unauthenticated',
					actions: assign({
						user: () => null,
						error: () => null,
					}),
				},
			},
		},
	},
});
