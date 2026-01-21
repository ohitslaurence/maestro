/**
 * Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
 * SPDX-License-Identifier: Proprietary
 */

import { getReposClient } from '$lib/api/repos';
import { error } from '@sveltejs/kit';
import type { PageLoad } from './$types';

export const load: PageLoad = async ({ params, parent, url }) => {
	const { repo, branches } = await parent();
	const client = getReposClient();

	const currentRef = params.ref || repo.default_branch;
	const limit = 30;
	const offset = parseInt(url.searchParams.get('offset') ?? '0', 10);

	try {
		const response = await client.getCommits(params.owner, params.repo, currentRef, {
			limit,
			offset,
		});

		return {
			repo,
			branches,
			commits: response.commits,
			total: response.total,
			currentRef,
			offset,
			limit,
		};
	} catch (e) {
		if (e instanceof Error && 'status' in e && (e as { status: number }).status === 404) {
			error(404, 'Branch not found');
		}
		throw e;
	}
};
