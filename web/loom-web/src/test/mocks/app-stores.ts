/**
 * Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
 * SPDX-License-Identifier: Proprietary
 */

import { readable, writable } from 'svelte/store';

export const page = readable({
	url: new URL('http://localhost'),
	params: {},
	route: { id: null },
	status: 200,
	error: null,
	data: {},
	form: null,
	state: {},
});

export const navigating = readable(null);

export const updated = {
	subscribe: readable(false).subscribe,
	check: async () => false,
};

export function getStores() {
	return {
		page,
		navigating,
		updated,
	};
}
