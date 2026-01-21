/**
 * Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
 * SPDX-License-Identifier: Proprietary
 */

import { createMachine, assign } from 'xstate';
import type { ThreadListContext } from './types';
import type { ThreadSummary } from '../api/types';

type ThreadListEvent =
	| { type: 'FETCH' }
	| { type: 'FETCH_SUCCESS'; threads: ThreadSummary[]; total: number }
	| { type: 'FETCH_ERROR'; error: string }
	| { type: 'SEARCH'; query: string }
	| { type: 'CLEAR_SEARCH' }
	| { type: 'NEXT_PAGE' }
	| { type: 'PREV_PAGE' }
	| { type: 'SET_LIMIT'; limit: number };

const initialContext: ThreadListContext = {
	threads: [],
	searchQuery: '',
	limit: 50,
	offset: 0,
	total: 0,
	error: null,
	isSearching: false,
};

export const threadListMachine = createMachine({
	id: 'threadList',
	initial: 'idle',
	context: initialContext,
	types: {} as {
		context: ThreadListContext;
		events: ThreadListEvent;
	},
	states: {
		idle: {
			on: {
				FETCH: {
					target: 'loading',
				},
			},
		},
		loading: {
			on: {
				FETCH_SUCCESS: {
					target: 'loaded',
					actions: assign({
						threads: ({ event }) => event.threads,
						total: ({ event }) => event.total,
						error: null,
					}),
				},
				FETCH_ERROR: {
					target: 'error',
					actions: assign({
						error: ({ event }) => event.error,
					}),
				},
			},
		},
		loaded: {
			on: {
				FETCH: {
					target: 'loading',
				},
				SEARCH: {
					target: 'loading',
					actions: assign({
						searchQuery: ({ event }) => event.query,
						offset: 0,
						isSearching: true,
					}),
				},
				CLEAR_SEARCH: {
					target: 'loading',
					actions: assign({
						searchQuery: '',
						offset: 0,
						isSearching: false,
					}),
				},
				NEXT_PAGE: {
					target: 'loading',
					actions: assign({
						offset: ({ context }) => context.offset + context.limit,
					}),
				},
				PREV_PAGE: {
					target: 'loading',
					actions: assign({
						offset: ({ context }) => Math.max(0, context.offset - context.limit),
					}),
				},
				SET_LIMIT: {
					target: 'loading',
					actions: assign({
						limit: ({ event }) => event.limit,
						offset: 0,
					}),
				},
			},
		},
		error: {
			on: {
				FETCH: {
					target: 'loading',
					actions: assign({
						error: null,
					}),
				},
			},
		},
	},
});

export type ThreadListMachine = typeof threadListMachine;
