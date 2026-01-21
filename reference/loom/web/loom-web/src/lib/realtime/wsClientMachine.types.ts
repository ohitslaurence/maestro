/**
 * Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
 * SPDX-License-Identifier: Proprietary
 */

import type { RealtimeMessage, ConnectionStatus } from './types';

export interface WsClientContext {
	sessionId: string | null;
	sessionToken: string | null;
	userId: string | null;
	retries: number;
	maxRetries: number;
	baseBackoffMs: number;
	backoffMs: number;
	lastError: string | null;
	lastCloseCode: number | null;
}

export type WsUiEvent =
	| { type: 'CONNECT'; sessionId: string; sessionToken: string }
	| { type: 'DISCONNECT' };

export type WsLifecycleEvent =
	| { type: 'SOCKET_OPENED' }
	| { type: 'SOCKET_CLOSED'; code: number; reason?: string }
	| { type: 'SOCKET_ERROR'; error: string };

export type WsAuthEvent =
	| { type: 'AUTH_OK'; userId: string }
	| { type: 'AUTH_ERROR'; message: string }
	| { type: 'AUTH_TIMEOUT' };

export type WsHeartbeatEvent =
	| { type: 'HEARTBEAT_TIMEOUT' }
	| { type: 'RETRY_DELAY_ELAPSED' };

export type WsMessageEvent = { type: 'INCOMING_MESSAGE'; message: RealtimeMessage };

export type WsClientEvent =
	| WsUiEvent
	| WsLifecycleEvent
	| WsAuthEvent
	| WsHeartbeatEvent
	| WsMessageEvent;

export type WsClientOutput =
	| { type: 'ui.status'; status: ConnectionStatus; retries: number }
	| { type: 'ui.message'; message: RealtimeMessage }
	| { type: 'ui.auth_failed'; message: string }
	| { type: 'ui.permanent_failure'; lastError: string | null };

export type WsClientState =
	| 'idle'
	| 'connecting'
	| 'authenticating'
	| 'connected'
	| 'reconnecting'
	| 'failed';

export function statusFromState(stateValue: WsClientState): ConnectionStatus {
	switch (stateValue) {
		case 'idle':
			return 'disconnected';
		case 'connecting':
		case 'authenticating':
			return 'connecting';
		case 'connected':
			return 'connected';
		case 'reconnecting':
			return 'reconnecting';
		case 'failed':
			return 'error';
	}
}
