<!--
 Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
 SPDX-License-Identifier: Proprietary
-->
<script lang="ts">
	import { onMount } from 'svelte';
	import { browser } from '$app/environment';
	import Breadcrumbs from './Breadcrumbs.svelte';
	import TableOfContents from './TableOfContents.svelte';
	import PrevNext from './PrevNext.svelte';
	import type { TocItem, NavItem, DiataxisCategoryType } from '../types';

	interface Props {
		title: string;
		category: DiataxisCategoryType;
		prev: NavItem | null;
		next: NavItem | null;
		children: import('svelte').Snippet;
	}

	let { title, category, prev, next, children }: Props = $props();

	let tocItems = $state<TocItem[]>([]);
	let activeId = $state('');
	let contentRef = $state<HTMLElement | null>(null);

	onMount(() => {
		if (!browser || !contentRef) return;

		const headings = contentRef.querySelectorAll('h2, h3');
		const items: TocItem[] = [];

		headings.forEach((heading) => {
			const id = heading.id;
			const text = heading.textContent ?? '';
			const depth = heading.tagName === 'H2' ? 2 : 3;
			if (id && text) {
				items.push({ id, text, depth });
			}
		});

		tocItems = items;

		const observer = new IntersectionObserver(
			(entries) => {
				for (const entry of entries) {
					if (entry.isIntersecting) {
						activeId = entry.target.id;
						break;
					}
				}
			},
			{ rootMargin: '-80px 0px -80% 0px', threshold: 0 }
		);

		headings.forEach((heading) => observer.observe(heading));

		return () => observer.disconnect();
	});
</script>

<div class="doc-page-wrapper">
	<article class="doc-page" bind:this={contentRef}>
		<Breadcrumbs {category} {title} />

		<div class="doc-content prose">
			{@render children()}
		</div>

		<PrevNext {prev} {next} />
	</article>

	{#if tocItems.length > 0}
		<aside class="doc-toc">
			<TableOfContents items={tocItems} {activeId} />
		</aside>
	{/if}
</div>

<style>
	.doc-page-wrapper {
		display: grid;
		grid-template-columns: 1fr 200px;
		gap: var(--space-8);
		align-items: start;
	}

	.doc-page {
		min-width: 0;
	}

	.doc-content {
		margin-top: var(--space-4);
	}

	.doc-toc {
		position: sticky;
		top: var(--space-8);
	}

	@media (max-width: 1024px) {
		.doc-page-wrapper {
			grid-template-columns: 1fr;
		}

		.doc-toc {
			display: none;
		}
	}

	.prose :global(h1) {
		font-family: var(--font-mono);
		font-size: var(--text-2xl);
		font-weight: 600;
		color: var(--color-fg);
		margin: 0 0 var(--space-4) 0;
	}

	.prose :global(h2) {
		font-family: var(--font-mono);
		font-size: var(--text-xl);
		font-weight: 600;
		color: var(--color-fg);
		margin: var(--space-8) 0 var(--space-3) 0;
		padding-top: var(--space-4);
		border-top: 1px solid var(--color-border);
	}

	.prose :global(h3) {
		font-family: var(--font-mono);
		font-size: var(--text-lg);
		font-weight: 600;
		color: var(--color-fg);
		margin: var(--space-6) 0 var(--space-2) 0;
	}

	.prose :global(p) {
		font-family: var(--font-mono);
		font-size: var(--text-base);
		color: var(--color-fg);
		line-height: 1.7;
		margin: 0 0 var(--space-4) 0;
	}

	.prose :global(ul),
	.prose :global(ol) {
		font-family: var(--font-mono);
		font-size: var(--text-base);
		color: var(--color-fg);
		line-height: 1.7;
		margin: 0 0 var(--space-4) 0;
		padding-left: var(--space-6);
	}

	.prose :global(li) {
		margin-bottom: var(--space-1);
	}

	.prose :global(pre) {
		background: var(--color-bg-muted);
		border: 1px solid var(--color-border);
		border-radius: var(--radius-md);
		padding: var(--space-4);
		margin: 0 0 var(--space-4) 0;
		overflow-x: auto;
	}

	.prose :global(code) {
		font-family: var(--font-mono);
		font-size: var(--text-sm);
	}

	.prose :global(:not(pre) > code) {
		background: var(--color-bg-muted);
		padding: 2px 6px;
		border-radius: var(--radius-sm);
	}

	.prose :global(a) {
		color: var(--color-accent);
		text-decoration: none;
	}

	.prose :global(a:hover) {
		text-decoration: underline;
	}

	.prose :global(blockquote) {
		border-left: 3px solid var(--color-accent);
		padding-left: var(--space-4);
		margin: 0 0 var(--space-4) 0;
		color: var(--color-fg-muted);
		font-style: italic;
	}

	.prose :global(table) {
		width: 100%;
		border-collapse: collapse;
		margin: 0 0 var(--space-4) 0;
		font-family: var(--font-mono);
		font-size: var(--text-sm);
	}

	.prose :global(th),
	.prose :global(td) {
		border: 1px solid var(--color-border);
		padding: var(--space-2) var(--space-3);
		text-align: left;
	}

	.prose :global(th) {
		background: var(--color-bg-muted);
		font-weight: 600;
	}
</style>
