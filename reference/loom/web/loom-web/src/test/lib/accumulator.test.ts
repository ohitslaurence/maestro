/**
 * Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
 * SPDX-License-Identifier: Proprietary
 */

import { describe, it, expect } from 'vitest';
import { test } from '@fast-check/vitest';
import * as fc from 'fast-check';
import {
	createAccumulator,
	accumulateEvent,
	accumulateTextDeltas,
} from '../../lib/realtime/accumulator';
import type { LlmEvent } from '../../lib/realtime/types';

describe('accumulator', () => {
	/**
	 * **Property: Text delta concatenation preserves all content**
	 *
	 * **Why this is important**: LLM responses stream in as chunks. If any
	 * content is lost during accumulation, users see incomplete responses.
	 *
	 * **Invariant**: concat(chunks) === original string
	 */
	test.prop([
		fc.array(fc.string({ minLength: 0, maxLength: 100 }), { minLength: 0, maxLength: 50 }),
	])('text_delta_concatenation_preserves_content', (chunks) => {
		const expected = chunks.join('');
		const deltas: Array<{ type: 'text_delta'; content: string }> = chunks.map((content) => ({
			type: 'text_delta',
			content,
		}));

		const result = accumulateTextDeltas(deltas);
		expect(result).toBe(expected);
	});

	/**
	 * **Property: Accumulator text grows monotonically**
	 *
	 * **Why this is important**: Text should never decrease during streaming.
	 * If it does, users would see content disappear, causing confusion.
	 *
	 * **Invariant**: |acc.text after event| >= |acc.text before event|
	 */
	test.prop([
		fc.array(
			fc.record({
				type: fc.constant('text_delta' as const),
				content: fc.string({ minLength: 0, maxLength: 50 }),
			}),
			{ minLength: 0, maxLength: 20 }
		),
	])('accumulator_text_grows_monotonically', (events) => {
		let acc = createAccumulator();
		let prevLength = 0;

		for (const event of events) {
			acc = accumulateEvent(acc, event);
			expect(acc.text.length).toBeGreaterThanOrEqual(prevLength);
			prevLength = acc.text.length;
		}
	});

	/**
	 * **Property: Tool call accumulation preserves all fragments**
	 *
	 * **Why this is important**: Tool call arguments stream as JSON fragments.
	 * Lost fragments result in invalid JSON and failed tool execution display.
	 *
	 * **Invariant**: For each tool call, argsJson === concat(all fragments for that call)
	 */
	test.prop([
		fc.string({ minLength: 1, maxLength: 10 }), // callId
		fc.string({ minLength: 1, maxLength: 20 }), // toolName
		fc.array(fc.string({ minLength: 1, maxLength: 20 }), { minLength: 1, maxLength: 10 }), // fragments
	])('tool_call_fragments_are_preserved', (callId, toolName, fragments) => {
		let acc = createAccumulator();
		const expectedArgsJson = fragments.join('');

		for (const fragment of fragments) {
			acc = accumulateEvent(acc, {
				type: 'tool_call_delta',
				callId,
				toolName,
				argsFragment: fragment,
			});
		}

		const toolCall = acc.toolCalls.get(callId);
		expect(toolCall).toBeDefined();
		expect(toolCall?.argsJson).toBe(expectedArgsJson);
		expect(toolCall?.name).toBe(toolName);
	});

	// Unit tests
	it('creates empty accumulator', () => {
		const acc = createAccumulator();
		expect(acc.text).toBe('');
		expect(acc.toolCalls.size).toBe(0);
	});

	it('accumulates text deltas', () => {
		let acc = createAccumulator();
		acc = accumulateEvent(acc, { type: 'text_delta', content: 'Hello ' });
		acc = accumulateEvent(acc, { type: 'text_delta', content: 'world!' });
		expect(acc.text).toBe('Hello world!');
	});

	it('ignores completed and error events', () => {
		let acc = createAccumulator();
		acc = accumulateEvent(acc, { type: 'text_delta', content: 'test' });
		acc = accumulateEvent(acc, { type: 'completed', response: {} as any });
		acc = accumulateEvent(acc, { type: 'error', error: 'fail' });
		expect(acc.text).toBe('test');
	});
});
