/**
 * Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
 * SPDX-License-Identifier: Proprietary
 */

export const prerender = false;

import { getReposClient } from '$lib/api/repos';
import { error } from '@sveltejs/kit';
import type { LayoutLoad } from './$types';

export const load: LayoutLoad = async ({ params }) => {
	const client = getReposClient();

	try {
		const [repo, branches] = await Promise.all([
			client.getRepo(params.owner, params.repo),
			client.getBranches(params.owner, params.repo),
		]);

		return {
			repo,
			branches,
		};
	} catch (e) {
		if (e instanceof Error && 'status' in e && (e as { status: number }).status === 404) {
			error(404, 'Repository not found');
		}
		throw e;
	}
};
