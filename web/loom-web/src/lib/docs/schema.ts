/**
 * Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
 * SPDX-License-Identifier: Proprietary
 */

import { z } from 'zod';

export const DiataxisCategory = z.enum(['tutorial', 'how-to', 'reference', 'explanation']);

export const DocSchema = z.object({
	title: z.string(),
	summary: z.string().optional(),
	diataxis: DiataxisCategory,
	order: z.number().default(100),
	tags: z.array(z.string()).optional(),
	draft: z.boolean().default(false),
	updatedAt: z.string().optional(),
});

export type DocMeta = z.infer<typeof DocSchema>;
export type DiataxisCategoryType = z.infer<typeof DiataxisCategory>;
