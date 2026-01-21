/**
 * Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
 * SPDX-License-Identifier: Proprietary
 */

import { createMachine, assign } from 'xstate';
import type { ConnectionContext } from './types';

type ConnectionEvent =
	| { type: 'CONNECT'; sessionId: string }
	| { type: 'CONNECTED' }
	| { type: 'DISCONNECTED'; error?: string }
	| { type: 'RETRY' }
	| { type: 'GIVE_UP' };

const initialContext: ConnectionContext = {
	sessionId: null,
	retries: 0,
	maxRetries: 5,
	lastError: null,
};

export const connectionMachine = createMachine({
	id: 'connection',
	initial: 'disconnected',
	context: initialContext,
	types: {} as {
		context: ConnectionContext;
		events: ConnectionEvent;
	},
	states: {
		disconnected: {
			on: {
				CONNECT: {
					target: 'connecting',
					actions: assign({
						sessionId: ({ event }) => event.sessionId,
						retries: 0,
						lastError: null,
					}),
				},
			},
		},
		connecting: {
			on: {
				CONNECTED: {
					target: 'connected',
					actions: assign({
						retries: 0,
						lastError: null,
					}),
				},
				DISCONNECTED: {
					target: 'reconnecting',
					actions: assign({
						lastError: ({ event }) => event.error || 'Connection failed',
					}),
				},
			},
		},
		connected: {
			on: {
				DISCONNECTED: {
					target: 'reconnecting',
					actions: assign({
						lastError: ({ event }) => event.error || 'Connection lost',
					}),
				},
			},
		},
		reconnecting: {
			entry: assign({
				retries: ({ context }) => context.retries + 1,
			}),
			always: [
				{
					guard: ({ context }) => context.retries >= context.maxRetries,
					target: 'failed',
				},
			],
			on: {
				RETRY: {
					target: 'connecting',
				},
				CONNECTED: {
					target: 'connected',
					actions: assign({
						retries: 0,
						lastError: null,
					}),
				},
				GIVE_UP: {
					target: 'failed',
				},
			},
		},
		failed: {
			on: {
				CONNECT: {
					target: 'connecting',
					actions: assign({
						sessionId: ({ event }) => event.sessionId,
						retries: 0,
						lastError: null,
					}),
				},
			},
		},
	},
});

export type ConnectionMachine = typeof connectionMachine;
