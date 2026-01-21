/**
 * Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
 * SPDX-License-Identifier: Proprietary
 */

import { DocSchema } from './schema';
import type { NavSection, NavItem, DocEntry, TocItem } from './types';
import { CATEGORY_TITLES, CATEGORY_ORDER, type DiataxisCategoryType } from './types';

function slugToTitle(slug: string): string {
	return slug
		.split('-')
		.map((word) => word.charAt(0).toUpperCase() + word.slice(1))
		.join(' ');
}

function pathToCategory(path: string): DiataxisCategoryType | null {
	if (path.includes('/tutorials/')) return 'tutorial';
	if (path.includes('/how-to/')) return 'how-to';
	if (path.includes('/reference/')) return 'reference';
	if (path.includes('/explanation/')) return 'explanation';
	return null;
}

function pathToSlug(path: string): string {
	const match = path.match(/\/([^/]+)\/\+page\.svx$/);
	return match ? match[1] : '';
}

export interface ParsedDoc {
	path: string;
	urlPath: string;
	category: DiataxisCategoryType;
	slug: string;
	meta: {
		title: string;
		summary?: string;
		order: number;
		tags?: string[];
		draft: boolean;
	};
}

export function parseDocModules(
	modules: Record<string, { metadata?: Record<string, unknown> }>
): ParsedDoc[] {
	const docs: ParsedDoc[] = [];

	for (const [path, mod] of Object.entries(modules)) {
		const category = pathToCategory(path);
		if (!category) continue;

		const slug = pathToSlug(path);
		if (!slug) continue;

		const rawMeta = mod.metadata ?? {};
		const parsed = DocSchema.safeParse({
			...rawMeta,
			diataxis: rawMeta.diataxis ?? category,
		});

		const meta = parsed.success
			? parsed.data
			: {
					title: slugToTitle(slug),
					diataxis: category,
					order: 100,
					draft: false,
				};

		if (meta.draft) continue;

		docs.push({
			path,
			urlPath: `/docs/${category === 'tutorial' ? 'tutorials' : category}/${slug}`,
			category,
			slug,
			meta: {
				title: meta.title,
				summary: meta.summary,
				order: meta.order,
				tags: meta.tags,
				draft: meta.draft,
			},
		});
	}

	return docs;
}

export function buildNavSections(docs: ParsedDoc[]): NavSection[] {
	const grouped = new Map<DiataxisCategoryType, NavItem[]>();

	for (const doc of docs) {
		if (!grouped.has(doc.category)) {
			grouped.set(doc.category, []);
		}
		grouped.get(doc.category)!.push({
			slug: doc.slug,
			path: doc.urlPath,
			title: doc.meta.title,
			order: doc.meta.order,
		});
	}

	const sections: NavSection[] = [];

	for (const category of CATEGORY_ORDER) {
		const items = grouped.get(category) ?? [];
		if (items.length === 0) continue;

		items.sort((a, b) => (a.order ?? 100) - (b.order ?? 100));

		sections.push({
			category,
			title: CATEGORY_TITLES[category],
			items,
		});
	}

	return sections;
}

export function findPrevNext(
	docs: ParsedDoc[],
	currentPath: string
): { prev: NavItem | null; next: NavItem | null } {
	const sorted = [...docs].sort((a, b) => {
		const catA = CATEGORY_ORDER.indexOf(a.category);
		const catB = CATEGORY_ORDER.indexOf(b.category);
		if (catA !== catB) return catA - catB;
		return a.meta.order - b.meta.order;
	});

	const currentIndex = sorted.findIndex((d) => d.urlPath === currentPath);
	if (currentIndex === -1) return { prev: null, next: null };

	const prev =
		currentIndex > 0
			? {
					slug: sorted[currentIndex - 1].slug,
					path: sorted[currentIndex - 1].urlPath,
					title: sorted[currentIndex - 1].meta.title,
					order: sorted[currentIndex - 1].meta.order,
				}
			: null;

	const next =
		currentIndex < sorted.length - 1
			? {
					slug: sorted[currentIndex + 1].slug,
					path: sorted[currentIndex + 1].urlPath,
					title: sorted[currentIndex + 1].meta.title,
					order: sorted[currentIndex + 1].meta.order,
				}
			: null;

	return { prev, next };
}
