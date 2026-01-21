/**
 * Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
 * SPDX-License-Identifier: Proprietary
 */

import { getReposClient } from '$lib/api/repos';
import { error } from '@sveltejs/kit';
import type { PageLoad } from './$types';

export const load: PageLoad = async ({ params, parent }) => {
	const { repo } = await parent();
	const client = getReposClient();

	try {
		const commit = await client.getCommit(params.owner, params.repo, params.sha);

		return {
			repo,
			commit,
		};
	} catch (e) {
		if (e instanceof Error && 'status' in e && (e as { status: number }).status === 404) {
			error(404, 'Commit not found');
		}
		throw e;
	}
};
