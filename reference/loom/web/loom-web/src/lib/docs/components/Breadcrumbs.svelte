<!--
 Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
 SPDX-License-Identifier: Proprietary
-->
<script lang="ts">
	import { i18n } from '$lib/i18n';
	import { CATEGORY_TITLES, type DiataxisCategoryType } from '../types';

	interface Props {
		category: DiataxisCategoryType;
		title: string;
	}

	let { category, title }: Props = $props();

	const categoryPath = $derived(category === 'tutorial' ? 'tutorials' : category);
</script>

<nav class="breadcrumbs" aria-label="Breadcrumb">
	<ol class="breadcrumb-list">
		<li class="breadcrumb-item">
			<a href="/docs" class="breadcrumb-link">{i18n('docs.breadcrumbs.docs')}</a>
		</li>
		<li class="breadcrumb-separator" aria-hidden="true">/</li>
		<li class="breadcrumb-item">
			<a href="/docs/{categoryPath}" class="breadcrumb-link">
				{CATEGORY_TITLES[category]}
			</a>
		</li>
		<li class="breadcrumb-separator" aria-hidden="true">/</li>
		<li class="breadcrumb-item current" aria-current="page">
			{title}
		</li>
	</ol>
</nav>

<style>
	.breadcrumbs {
		font-family: var(--font-mono);
		font-size: var(--text-sm);
		margin-bottom: var(--space-4);
	}

	.breadcrumb-list {
		display: flex;
		align-items: center;
		gap: var(--space-2);
		list-style: none;
		padding: 0;
		margin: 0;
		flex-wrap: wrap;
	}

	.breadcrumb-item {
		color: var(--color-fg-muted);
	}

	.breadcrumb-link {
		color: var(--color-fg-muted);
		text-decoration: none;
		transition: color 0.15s ease;
	}

	.breadcrumb-link:hover {
		color: var(--color-accent);
	}

	.breadcrumb-separator {
		color: var(--color-fg-subtle);
	}

	.breadcrumb-item.current {
		color: var(--color-fg);
	}
</style>
