/**
 * Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
 * SPDX-License-Identifier: Proprietary
 *
 * WebSocket client state machine with first-message authentication.
 *
 * Pure machine: no direct side-effects. All I/O/timers/UI events are done by
 * actors/services using the actions and states defined here.
 *
 * State diagram:
 *
 *   idle → connecting → authenticating → connected
 *            ↓              ↓               ↓
 *       reconnecting ←──────┴───────────────┘ (on network failure)
 *            ↓
 *         failed (after maxRetries)
 *
 * Auth failures (AUTH_ERROR, AUTH_TIMEOUT) go directly to idle (no reconnect).
 */

import { createMachine, assign } from 'xstate';
import type { WsClientContext, WsClientEvent } from './wsClientMachine.types';

const MAX_RETRIES_DEFAULT = 5;
const BASE_BACKOFF_MS_DEFAULT = 1000;

const initialContext: WsClientContext = {
	sessionId: null,
	sessionToken: null,
	userId: null,
	retries: 0,
	maxRetries: MAX_RETRIES_DEFAULT,
	baseBackoffMs: BASE_BACKOFF_MS_DEFAULT,
	backoffMs: 0,
	lastError: null,
	lastCloseCode: null,
};

function computeBackoffMs(base: number, attempt: number): number {
	return base * Math.pow(2, Math.max(0, attempt - 1));
}

export const wsClientMachine = createMachine({
	id: 'wsClient',
	initial: 'idle',
	context: initialContext,
	types: {} as {
		context: WsClientContext;
		events: WsClientEvent;
	},
	states: {
		idle: {
			entry: ['notifyStatusDisconnected'],
			on: {
				CONNECT: {
					target: 'connecting',
					actions: assign(({ event }) => ({
						sessionId: event.sessionId,
						sessionToken: event.sessionToken,
						userId: null,
						retries: 0,
						backoffMs: 0,
						lastError: null,
						lastCloseCode: null,
					})),
				},
			},
		},

		connecting: {
			entry: ['notifyStatusConnecting', 'notifyOpenSocket'],
			on: {
				SOCKET_OPENED: {
					target: 'authenticating',
				},
				SOCKET_ERROR: {
					target: 'reconnecting',
					actions: assign(({ event }) => ({
						lastError: event.error ?? 'WebSocket error while connecting',
					})),
				},
				SOCKET_CLOSED: {
					target: 'reconnecting',
					actions: assign(({ event }) => ({
						lastError: event.reason ?? 'WebSocket closed while connecting',
						lastCloseCode: event.code,
					})),
				},
				DISCONNECT: {
					target: 'idle',
					actions: ['notifyCloseSocket'],
				},
			},
		},

		authenticating: {
			entry: ['notifySendAuthMessage', 'notifyAuthTimerStart'],
			exit: ['notifyAuthTimerStop'],
			on: {
				AUTH_OK: {
					target: 'connected',
					actions: assign(({ event }) => ({
						userId: event.userId,
						retries: 0,
						lastError: null,
					})),
				},
				AUTH_ERROR: {
					target: 'idle',
					actions: [
						assign(({ event }) => ({
							lastError: event.message,
							userId: null,
							retries: 0,
							backoffMs: 0,
						})),
						'notifyAuthFailed',
						'notifyCloseSocket',
					],
				},
				AUTH_TIMEOUT: {
					target: 'idle',
					actions: [
						assign(() => ({
							lastError: 'Authentication timeout',
							userId: null,
							retries: 0,
							backoffMs: 0,
						})),
						'notifyAuthFailed',
						'notifyCloseSocket',
					],
				},
				SOCKET_ERROR: {
					target: 'reconnecting',
					actions: assign(({ event }) => ({
						lastError: event.error ?? 'WebSocket error during auth',
					})),
				},
				SOCKET_CLOSED: {
					target: 'reconnecting',
					actions: assign(({ event }) => ({
						lastError: event.reason ?? 'WebSocket closed during auth',
						lastCloseCode: event.code,
					})),
				},
				DISCONNECT: {
					target: 'idle',
					actions: ['notifyCloseSocket'],
				},
			},
		},

		connected: {
			entry: ['notifyStatusConnected', 'notifyHeartbeatStart'],
			exit: ['notifyHeartbeatStop'],
			on: {
				INCOMING_MESSAGE: {
					actions: ['notifyMessage'],
				},
				HEARTBEAT_TIMEOUT: {
					target: 'reconnecting',
					actions: assign(() => ({
						lastError: 'Heartbeat timeout',
					})),
				},
				SOCKET_ERROR: {
					target: 'reconnecting',
					actions: assign(({ event }) => ({
						lastError: event.error ?? 'WebSocket error while connected',
					})),
				},
				SOCKET_CLOSED: {
					target: 'reconnecting',
					actions: assign(({ event }) => ({
						lastError: event.reason ?? 'WebSocket closed while connected',
						lastCloseCode: event.code,
					})),
				},
				DISCONNECT: {
					target: 'idle',
					actions: ['notifyCloseSocket'],
				},
			},
		},

		reconnecting: {
			entry: [
				assign(({ context }) => {
					const nextRetries = Math.min(context.retries + 1, context.maxRetries);
					const backoffMs = computeBackoffMs(context.baseBackoffMs, nextRetries);
					return {
						retries: nextRetries,
						backoffMs,
					};
				}),
				'notifyStatusReconnecting',
				'notifyScheduleReconnect',
			],
			always: [
				{
					guard: ({ context }) => context.retries >= context.maxRetries,
					target: 'failed',
				},
			],
			on: {
				RETRY_DELAY_ELAPSED: {
					target: 'connecting',
				},
				DISCONNECT: {
					target: 'idle',
					actions: ['notifyCloseSocket'],
				},
			},
		},

		failed: {
			entry: ['notifyStatusError', 'notifyPermanentFailure'],
			on: {
				CONNECT: {
					target: 'connecting',
					actions: assign(({ event }) => ({
						sessionId: event.sessionId,
						sessionToken: event.sessionToken,
						userId: null,
						retries: 0,
						backoffMs: 0,
						lastError: null,
						lastCloseCode: null,
					})),
				},
				DISCONNECT: {
					target: 'idle',
				},
			},
		},
	},
});

export type WsClientMachine = typeof wsClientMachine;
