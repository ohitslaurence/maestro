/**
 * Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
 * SPDX-License-Identifier: Proprietary
 */

import type { LayoutLoad } from './$types';
import { parseDocModules, buildNavSections } from '$lib/docs';

export const prerender = true;
export const ssr = true;

export const load: LayoutLoad = async () => {
	const modules = import.meta.glob('./**/+page.svx', { eager: true }) as Record<
		string,
		{ metadata?: Record<string, unknown> }
	>;

	const docs = parseDocModules(modules);
	const sections = buildNavSections(docs);

	return {
		sections,
		docs,
	};
};
