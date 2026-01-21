<!--
 Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
 SPDX-License-Identifier: Proprietary
-->
<script lang="ts">
	import { page } from '$app/stores';
	import type { NavSection } from '../types';

	interface Props {
		sections: NavSection[];
	}

	let { sections }: Props = $props();

	const currentPath = $derived($page.url.pathname);

	function getCategoryPath(category: string): string {
		return category === 'tutorial' ? '/docs/tutorials' : `/docs/${category}`;
	}
</script>

<nav class="sidebar-nav" aria-label="Documentation">
	{#each sections as section}
		<div class="nav-section">
			<a href={getCategoryPath(section.category)} class="nav-section-title">
				{section.title}
			</a>
			<ul class="nav-items">
				{#each section.items as item}
					<li>
						<a
							href={item.path}
							class="nav-link"
							class:active={currentPath === item.path}
							aria-current={currentPath === item.path ? 'page' : undefined}
						>
							{item.title}
						</a>
					</li>
				{/each}
			</ul>
		</div>
	{/each}
</nav>

<style>
	.sidebar-nav {
		display: flex;
		flex-direction: column;
		gap: var(--space-6);
	}

	.nav-section {
		display: flex;
		flex-direction: column;
		gap: var(--space-1);
	}

	.nav-section-title {
		font-family: var(--font-mono);
		font-size: var(--text-xs);
		font-weight: 600;
		color: var(--color-fg-muted);
		text-transform: uppercase;
		letter-spacing: 0.05em;
		text-decoration: none;
		padding: var(--space-1) 0;
		transition: color 0.15s ease;
	}

	.nav-section-title:hover {
		color: var(--color-accent);
	}

	.nav-items {
		list-style: none;
		padding: 0;
		margin: 0;
		display: flex;
		flex-direction: column;
		gap: var(--space-1);
	}

	.nav-link {
		display: block;
		font-family: var(--font-mono);
		font-size: var(--text-sm);
		color: var(--color-fg-muted);
		text-decoration: none;
		padding: var(--space-1) var(--space-2);
		border-radius: var(--radius-sm);
		transition: all 0.15s ease;
		border-left: 2px solid transparent;
	}

	.nav-link:hover {
		color: var(--color-fg);
		background: var(--color-bg-muted);
	}

	.nav-link.active {
		color: var(--color-accent);
		background: var(--color-accent-soft);
		border-left-color: var(--color-accent);
	}
</style>
