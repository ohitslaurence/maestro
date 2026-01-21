/**
 * Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
 * SPDX-License-Identifier: Proprietary
 */

import { getReposClient } from '$lib/api/repos';
import { error } from '@sveltejs/kit';
import type { PageLoad } from './$types';

const README_PATTERNS = ['README.md', 'readme.md', 'README.MD', 'Readme.md', 'README', 'readme'];

export const load: PageLoad = async ({ params, parent }) => {
	const { repo, branches } = await parent();
	const client = getReposClient();

	const pathParts = params.path?.split('/') ?? [];
	const currentRef = pathParts[0] || repo.default_branch;
	const currentPath = pathParts.slice(1).join('/');

	try {
		const entries = await client.getTree(params.owner, params.repo, currentRef, currentPath);

		// Look for README file in current directory
		let readme: string | undefined;
		let readmeFilename: string | undefined;

		for (const pattern of README_PATTERNS) {
			const readmeEntry = entries.find(
				(e) => e.name.toLowerCase() === pattern.toLowerCase() && e.kind === 'file'
			);
			if (readmeEntry) {
				try {
					const readmePath = currentPath ? `${currentPath}/${readmeEntry.name}` : readmeEntry.name;
					readme = await client.getBlob(params.owner, params.repo, currentRef, readmePath);
					readmeFilename = readmeEntry.name;
					break;
				} catch {
					// Ignore errors fetching README
				}
			}
		}

		return {
			repo,
			branches,
			entries,
			currentRef,
			currentPath,
			readme,
			readmeFilename,
		};
	} catch (e) {
		if (e instanceof Error && 'status' in e && (e as { status: number }).status === 404) {
			error(404, 'Path not found');
		}
		throw e;
	}
};
