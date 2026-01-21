// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

export type DiataxisCategoryType = 'tutorial' | 'how-to' | 'explanation' | 'reference';

export const CATEGORY_TITLES: Record<DiataxisCategoryType, string> = {
	tutorial: 'Tutorials',
	'how-to': 'How-to Guides',
	explanation: 'Explanation',
	reference: 'Reference'
};

export const CATEGORY_ORDER: DiataxisCategoryType[] = ['tutorial', 'how-to', 'explanation', 'reference'];

export interface TocItem {
	id: string;
	text: string;
	depth: number;
}

export interface NavItem {
	slug?: string;
	title: string;
	path: string;
	order?: number;
}

export interface NavSection {
	category: DiataxisCategoryType;
	title: string;
	items: NavItem[];
}

export interface DocEntry {
	slug: string;
	path: string;
	title: string;
	summary?: string;
	order: number;
	category: DiataxisCategoryType;
}
