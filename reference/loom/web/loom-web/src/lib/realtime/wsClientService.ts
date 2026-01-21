/**
 * Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
 * SPDX-License-Identifier: Proprietary
 *
 * WebSocket client service - handles side effects for wsClientMachine.
 * This includes WebSocket I/O, timers, and UI notifications.
 */

import { createActor, type Actor } from 'xstate';
import { wsClientMachine, type WsClientMachine } from './wsClientMachine';
import type { WsClientContext, WsClientEvent, WsClientState, WsClientOutput, statusFromState } from './wsClientMachine.types';
import type { RealtimeMessage, ConnectionStatus, LlmEvent, LlmEventWire, ToolEvent, ToolEventWire } from './types';
import { logger } from '../logging';
import { getApiClient } from '../api';

const AUTH_TIMEOUT_MS = 5000;
const PING_INTERVAL_MS = 30000;
const MAX_MISSED_PONGS = 2;

function buildWsUrl(serverUrl: string, sessionId: string): string {
	const baseUrl = serverUrl || (typeof window !== 'undefined' ? window.location.origin : '');
	const url = new URL(`/api/ws/sessions/${sessionId}`, baseUrl);
	url.protocol = url.protocol === 'https:' ? 'wss:' : 'ws:';
	return url.toString();
}

export type LlmEventHandler = (event: LlmEvent) => void;
export type ToolEventHandler = (event: ToolEvent) => void;
export type MessageHandler = (msg: RealtimeMessage) => void;
export type StatusHandler = (status: ConnectionStatus) => void;

export class LoomWebSocketClient {
	private actor: Actor<WsClientMachine>;
	private ws: WebSocket | null = null;
	private authTimer: ReturnType<typeof setTimeout> | null = null;
	private heartbeatInterval: ReturnType<typeof setInterval> | null = null;
	private reconnectTimer: ReturnType<typeof setTimeout> | null = null;
	private missedPongs = 0;

	private messageHandlers = new Set<MessageHandler>();
	private llmEventHandlers = new Set<LlmEventHandler>();
	private toolEventHandlers = new Set<ToolEventHandler>();
	private statusHandlers = new Set<StatusHandler>();

	constructor(private serverUrl: string) {
		this.actor = createActor(
			wsClientMachine.provide({
				actions: {
					notifyStatusDisconnected: () => this.emitStatus('disconnected'),
					notifyStatusConnecting: () => this.emitStatus('connecting'),
					notifyStatusConnected: () => this.emitStatus('connected'),
					notifyStatusReconnecting: () => this.emitStatus('reconnecting'),
					notifyStatusError: () => this.emitStatus('error'),
					notifyOpenSocket: () => this.openSocket(),
					notifyCloseSocket: () => this.closeSocket(),
					notifySendAuthMessage: () => this.sendAuthMessage(),
					notifyAuthTimerStart: () => this.startAuthTimer(),
					notifyAuthTimerStop: () => this.stopAuthTimer(),
					notifyHeartbeatStart: () => this.startHeartbeat(),
					notifyHeartbeatStop: () => this.stopHeartbeat(),
					notifyScheduleReconnect: ({ context }) => this.scheduleReconnect(context.backoffMs),
					notifyMessage: ({ event }) => {
						if (event.type === 'INCOMING_MESSAGE') {
							this.handleMessage(event.message);
						}
					},
					notifyAuthFailed: ({ context }) => {
						logger.error('WebSocket auth failed', { error: context.lastError });
					},
					notifyPermanentFailure: ({ context }) => {
						logger.error('WebSocket connection permanently failed', { error: context.lastError });
					},
				},
			})
		);
		this.actor.start();
	}

	async connect(sessionId: string): Promise<void> {
		try {
			const api = getApiClient(this.serverUrl);
			const { token } = await api.getWsToken();
			this.actor.send({ type: 'CONNECT', sessionId, sessionToken: token });
		} catch (error) {
			logger.error('Failed to get WebSocket token', { error: String(error) });
			this.emitStatus('error');
		}
	}

	disconnect(): void {
		this.actor.send({ type: 'DISCONNECT' });
	}

	send(msg: RealtimeMessage): void {
		if (this.ws?.readyState === WebSocket.OPEN) {
			this.ws.send(JSON.stringify(msg));
		} else {
			logger.warn('Cannot send message, WebSocket not connected');
		}
	}

	sendMessage(content: string): void {
		this.send({
			type: 'user_message',
			content,
			timestamp: new Date().toISOString(),
		});
	}

	onMessage(handler: MessageHandler): () => void {
		this.messageHandlers.add(handler);
		return () => this.messageHandlers.delete(handler);
	}

	onLlmEvent(handler: LlmEventHandler): () => void {
		this.llmEventHandlers.add(handler);
		return () => this.llmEventHandlers.delete(handler);
	}

	onToolEvent(handler: ToolEventHandler): () => void {
		this.toolEventHandlers.add(handler);
		return () => this.toolEventHandlers.delete(handler);
	}

	onStatus(handler: StatusHandler): () => void {
		this.statusHandlers.add(handler);
		return () => this.statusHandlers.delete(handler);
	}

	getStatus(): ConnectionStatus {
		const state = this.actor.getSnapshot().value as WsClientState;
		switch (state) {
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

	private emitStatus(status: ConnectionStatus): void {
		this.statusHandlers.forEach((h) => h(status));
	}

	private openSocket(): void {
		const ctx = this.actor.getSnapshot().context;
		if (!ctx.sessionId) return;

		const url = buildWsUrl(this.serverUrl, ctx.sessionId);
		logger.info('Opening WebSocket', { url, sessionId: ctx.sessionId });

		try {
			this.ws = new WebSocket(url);

			this.ws.onopen = () => {
				logger.debug('WebSocket opened');
				this.actor.send({ type: 'SOCKET_OPENED' });
			};

			this.ws.onclose = (e) => {
				logger.info('WebSocket closed', { code: e.code, reason: e.reason });
				this.actor.send({ type: 'SOCKET_CLOSED', code: e.code, reason: e.reason });
			};

			this.ws.onerror = () => {
				logger.error('WebSocket error');
				this.actor.send({ type: 'SOCKET_ERROR', error: 'WebSocket error' });
			};

			this.ws.onmessage = (e) => {
				this.handleRawMessage(e.data);
			};
		} catch (error) {
			logger.error('Failed to create WebSocket', { error: String(error) });
			this.actor.send({ type: 'SOCKET_ERROR', error: String(error) });
		}
	}

	private closeSocket(): void {
		this.stopAuthTimer();
		this.stopHeartbeat();
		this.stopReconnectTimer();

		if (this.ws) {
			this.ws.onopen = null;
			this.ws.onclose = null;
			this.ws.onerror = null;
			this.ws.onmessage = null;
			if (this.ws.readyState === WebSocket.OPEN || this.ws.readyState === WebSocket.CONNECTING) {
				this.ws.close(1000, 'Client disconnect');
			}
			this.ws = null;
		}
	}

	private sendAuthMessage(): void {
		const ctx = this.actor.getSnapshot().context;
		if (!ctx.sessionToken || !this.ws || this.ws.readyState !== WebSocket.OPEN) {
			this.actor.send({ type: 'AUTH_ERROR', message: 'Cannot send auth message' });
			return;
		}

		const authMsg = { type: 'auth', token: ctx.sessionToken };
		this.ws.send(JSON.stringify(authMsg));
		logger.debug('Sent WebSocket auth message');
	}

	private startAuthTimer(): void {
		this.stopAuthTimer();
		this.authTimer = setTimeout(() => {
			logger.warn('WebSocket auth timeout');
			this.actor.send({ type: 'AUTH_TIMEOUT' });
		}, AUTH_TIMEOUT_MS);
	}

	private stopAuthTimer(): void {
		if (this.authTimer) {
			clearTimeout(this.authTimer);
			this.authTimer = null;
		}
	}

	private startHeartbeat(): void {
		this.stopHeartbeat();
		this.missedPongs = 0;

		this.heartbeatInterval = setInterval(() => {
			if (this.ws?.readyState === WebSocket.OPEN) {
				this.missedPongs++;
				if (this.missedPongs > MAX_MISSED_PONGS) {
					logger.warn('WebSocket heartbeat timeout');
					this.actor.send({ type: 'HEARTBEAT_TIMEOUT' });
					return;
				}
				this.send({
					type: 'control',
					id: crypto.randomUUID(),
					data: { command: 'ping' },
					timestamp: new Date().toISOString(),
				});
			}
		}, PING_INTERVAL_MS);
	}

	private stopHeartbeat(): void {
		if (this.heartbeatInterval) {
			clearInterval(this.heartbeatInterval);
			this.heartbeatInterval = null;
		}
	}

	private scheduleReconnect(delayMs: number): void {
		this.stopReconnectTimer();
		const ctx = this.actor.getSnapshot().context;
		logger.info('Scheduling reconnect', { delayMs, attempt: ctx.retries });

		this.reconnectTimer = setTimeout(() => {
			this.actor.send({ type: 'RETRY_DELAY_ELAPSED' });
		}, delayMs);
	}

	private stopReconnectTimer(): void {
		if (this.reconnectTimer) {
			clearTimeout(this.reconnectTimer);
			this.reconnectTimer = null;
		}
	}

	private handleRawMessage(data: string): void {
		try {
			const msg = JSON.parse(data);
			const state = this.actor.getSnapshot().value;

			// During authenticating, only handle auth responses
			if (state === 'authenticating') {
				if (msg.type === 'auth_ok') {
					this.actor.send({ type: 'AUTH_OK', userId: msg.user_id });
				} else if (msg.type === 'auth_error') {
					this.actor.send({ type: 'AUTH_ERROR', message: msg.message });
				}
				return;
			}

			// In connected state, handle all messages
			if (state === 'connected') {
				// Handle server pings
				if (msg.type === 'ping') {
					this.ws?.send(JSON.stringify({ type: 'pong' }));
					return;
				}

				// Handle pong responses
				if (msg.type === 'control' && msg.data?.command === 'pong') {
					this.missedPongs = 0;
					return;
				}

				// Forward to machine for notification
				this.actor.send({ type: 'INCOMING_MESSAGE', message: msg });
			}
		} catch (error) {
			logger.error('Failed to parse WebSocket message', { error: String(error) });
		}
	}

	private handleMessage(msg: RealtimeMessage): void {
		// Notify all message handlers
		this.messageHandlers.forEach((h) => h(msg));

		// Convert LLM events for convenience handlers
		if (msg.type === 'llm_event') {
			const llmEvent = this.convertLlmEvent(msg.data);
			if (llmEvent) {
				this.llmEventHandlers.forEach((h) => h(llmEvent));
			}
		}

		// Convert tool events for convenience handlers
		if (msg.type === 'tool_event') {
			const toolEvent = this.convertToolEvent(msg.data);
			if (toolEvent) {
				this.toolEventHandlers.forEach((h) => h(toolEvent));
			}
		}
	}

	private convertLlmEvent(wire: LlmEventWire): LlmEvent | null {
		switch (wire.event_type) {
			case 'text_delta':
				return { type: 'text_delta', content: wire.content || '' };
			case 'tool_call_delta':
				return {
					type: 'tool_call_delta',
					callId: wire.call_id || '',
					toolName: wire.tool_name || '',
					argsFragment: wire.arguments_fragment || '',
				};
			case 'completed':
				if (wire.response) {
					return { type: 'completed', response: wire.response };
				}
				return null;
			case 'error':
				return { type: 'error', error: wire.error || 'Unknown error' };
			default:
				return null;
		}
	}

	private convertToolEvent(wire: ToolEventWire): ToolEvent {
		return {
			type: wire.event_type,
			callId: wire.call_id,
			toolName: wire.tool_name,
			progress: wire.progress,
			message: wire.message,
			output: wire.output,
			error: wire.error,
		};
	}
}
