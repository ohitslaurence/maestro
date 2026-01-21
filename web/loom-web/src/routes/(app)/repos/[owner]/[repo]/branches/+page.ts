/**
 * Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
 * SPDX-License-Identifier: Proprietary
 */

import type { PageLoad } from './$types';

export const load: PageLoad = async ({ parent }) => {
	const { repo, branches } = await parent();
	return { repo, branches };
};
