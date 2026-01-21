/**
 * Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
 * SPDX-License-Identifier: Proprietary
 */

import { getReposClient } from '$lib/api/repos';
import { error } from '@sveltejs/kit';
import type { PageLoad } from './$types';

export const load: PageLoad = async ({ params, parent }) => {
	const { repo, branches } = await parent();
	const client = getReposClient();

	const refsParam = params.refs ?? '';
	const match = refsParam.match(/^(.+)\.\.\.(.+)$/);

	if (!match) {
		error(400, 'Invalid compare format. Use base...head');
	}

	const [, baseRef, headRef] = match;

	try {
		const result = await client.compare(params.owner, params.repo, baseRef, headRef);

		return {
			repo,
			branches,
			result,
			baseRef,
			headRef,
		};
	} catch (e) {
		if (e instanceof Error && 'status' in e && (e as { status: number }).status === 404) {
			error(404, 'Refs not found');
		}
		throw e;
	}
};
