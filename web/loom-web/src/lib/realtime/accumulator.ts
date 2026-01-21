/**
 * Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
 * SPDX-License-Identifier: Proprietary
 */

import type { LlmEvent } from './types';

export interface AccumulatedContent {
	text: string;
	toolCalls: Map<string, { name: string; argsJson: string }>;
}

export function createAccumulator(): AccumulatedContent {
	return {
		text: '',
		toolCalls: new Map(),
	};
}

export function accumulateEvent(acc: AccumulatedContent, event: LlmEvent): AccumulatedContent {
	switch (event.type) {
		case 'text_delta':
			return {
				...acc,
				text: acc.text + event.content,
			};
		case 'tool_call_delta': {
			const existing = acc.toolCalls.get(event.callId) || { name: event.toolName, argsJson: '' };
			const updated = new Map(acc.toolCalls);
			updated.set(event.callId, {
				name: existing.name || event.toolName,
				argsJson: existing.argsJson + event.argsFragment,
			});
			return { ...acc, toolCalls: updated };
		}
		case 'completed':
		case 'error':
			return acc;
		default:
			return acc;
	}
}

export function accumulateTextDeltas(
	events: Array<{ type: 'text_delta'; content: string }>
): string {
	return events.reduce((acc, e) => acc + e.content, '');
}
