/**
 * Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
 * SPDX-License-Identifier: Proprietary
 */

import type { ConnectionStatus, LlmEvent, LlmEventHandler, StatusHandler } from './types';
import { logger } from '../logging';

export class LoomSseClient {
	private eventSource: EventSource | null = null;
	private llmEventHandlers = new Set<LlmEventHandler>();
	private statusHandlers = new Set<StatusHandler>();
	private status: ConnectionStatus = 'disconnected';

	constructor(private serverUrl: string) {}

	connect(provider: 'anthropic' | 'openai' = 'anthropic'): void {
		this.setStatus('connecting');

		const url = `${this.serverUrl}/proxy/${provider}/stream`;

		try {
			this.eventSource = new EventSource(url);

			this.eventSource.onopen = () => {
				logger.info('SSE connected', { provider });
				this.setStatus('connected');
			};

			this.eventSource.onerror = () => {
				logger.error('SSE error', { provider });
				this.setStatus('error');
			};

			this.eventSource.addEventListener('llm', (event) => {
				this.handleLlmEvent(event);
			});
		} catch (error) {
			logger.error('SSE connection failed', { provider, error: String(error) });
			this.setStatus('error');
			throw error;
		}
	}

	disconnect(): void {
		if (this.eventSource) {
			this.eventSource.close();
			this.eventSource = null;
		}
		this.setStatus('disconnected');
	}

	async sendMessage(content: string): Promise<void> {
		try {
			const response = await fetch(`${this.serverUrl}/api/messages`, {
				method: 'POST',
				headers: { 'Content-Type': 'application/json' },
				body: JSON.stringify({ content, timestamp: new Date().toISOString() }),
			});
			if (!response.ok) {
				throw new Error(`Failed to send message: ${response.status}`);
			}
		} catch (error) {
			logger.error('Failed to send message via SSE client', { error: String(error) });
			throw error;
		}
	}

	onLlmEvent(handler: LlmEventHandler): () => void {
		this.llmEventHandlers.add(handler);
		return () => this.llmEventHandlers.delete(handler);
	}

	onStatus(handler: StatusHandler): () => void {
		this.statusHandlers.add(handler);
		return () => this.statusHandlers.delete(handler);
	}

	getStatus(): ConnectionStatus {
		return this.status;
	}

	private setStatus(status: ConnectionStatus): void {
		this.status = status;
		this.statusHandlers.forEach((h) => h(status));
	}

	private handleLlmEvent(event: MessageEvent): void {
		try {
			const data = JSON.parse(event.data);

			let llmEvent: LlmEvent | null = null;

			switch (data.type) {
				case 'text_delta':
					llmEvent = { type: 'text_delta', content: data.content || '' };
					break;
				case 'tool_call_delta':
					llmEvent = {
						type: 'tool_call_delta',
						callId: data.call_id || '',
						toolName: data.tool_name || '',
						argsFragment: data.arguments_fragment || '',
					};
					break;
				case 'completed':
					if (data.response) {
						llmEvent = { type: 'completed', response: data.response };
					}
					break;
				case 'error':
					llmEvent = { type: 'error', error: data.message || 'Unknown error' };
					break;
			}

			if (llmEvent) {
				this.llmEventHandlers.forEach((h) => h(llmEvent));
			}
		} catch (error) {
			logger.error('Failed to parse SSE message', { error: String(error) });
		}
	}
}
