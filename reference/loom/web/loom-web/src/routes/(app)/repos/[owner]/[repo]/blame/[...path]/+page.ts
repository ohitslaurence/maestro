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

	const pathParts = params.path?.split('/') ?? [];
	const currentRef = pathParts[0] || repo.default_branch;
	const currentPath = pathParts.slice(1).join('/');

	if (!currentPath) {
		error(404, 'File path required');
	}

	try {
		const blameLines = await client.getBlame(params.owner, params.repo, currentRef, currentPath);

		return {
			repo,
			branches,
			blameLines,
			currentRef,
			currentPath,
		};
	} catch (e) {
		if (e instanceof Error && 'status' in e && (e as { status: number }).status === 404) {
			error(404, 'File not found');
		}
		throw e;
	}
};
