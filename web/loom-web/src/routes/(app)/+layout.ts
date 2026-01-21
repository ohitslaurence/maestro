/**
 * Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
 * SPDX-License-Identifier: Proprietary
 */

import type { LayoutLoad } from './$types';
import { goto } from '$app/navigation';
import { browser } from '$app/environment';

export const ssr = false;
export const prerender = false;

export const load: LayoutLoad = async ({ url, fetch }) => {
	if (!browser) {
		return { user: null };
	}

	try {
		const response = await fetch('/auth/me');

		if (!response.ok) {
			const redirectTo = url.pathname + url.search;
			goto(`/login?redirectTo=${encodeURIComponent(redirectTo)}`);
			return { user: null };
		}

		const user = await response.json();
		return { user };
	} catch {
		const redirectTo = url.pathname + url.search;
		goto(`/login?redirectTo=${encodeURIComponent(redirectTo)}`);
		return { user: null };
	}
};
