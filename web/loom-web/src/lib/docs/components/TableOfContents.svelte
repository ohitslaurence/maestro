<!--
 Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
 SPDX-License-Identifier: Proprietary
-->
<script lang="ts">
	import { i18n } from '$lib/i18n';
	import type { TocItem } from '../types';

	interface Props {
		items: TocItem[];
		activeId?: string;
	}

	let { items, activeId = '' }: Props = $props();
</script>

{#if items.length > 0}
	<nav class="toc" aria-label={i18n('docs.toc.title')}>
		<h4 class="toc-title">{i18n('docs.toc.title')}</h4>
		<ul class="toc-list">
			{#each items as item}
				<li class="toc-item" class:depth-3={item.depth === 3}>
					<a
						href="#{item.id}"
						class="toc-link"
						class:active={activeId === item.id}
						aria-current={activeId === item.id ? 'location' : undefined}
					>
						{item.text}
					</a>
				</li>
			{/each}
		</ul>
	</nav>
{/if}

<style>
	.toc {
		font-family: var(--font-mono);
		font-size: var(--text-sm);
		position: sticky;
		top: var(--space-6);
	}

	.toc-title {
		font-size: var(--text-xs);
		font-weight: 600;
		color: var(--color-fg-muted);
		text-transform: uppercase;
		letter-spacing: 0.05em;
		margin: 0 0 var(--space-3) 0;
	}

	.toc-list {
		list-style: none;
		padding: 0;
		margin: 0;
		border-left: 1px solid var(--color-border);
	}

	.toc-item {
		margin: 0;
	}

	.toc-item.depth-3 {
		padding-left: var(--space-3);
	}

	.toc-link {
		display: block;
		padding: var(--space-1) var(--space-3);
		color: var(--color-fg-muted);
		text-decoration: none;
		border-left: 2px solid transparent;
		margin-left: -1px;
		transition: all 0.15s ease;
	}

	.toc-link:hover {
		color: var(--color-fg);
	}

	.toc-link.active {
		color: var(--color-accent);
		border-left-color: var(--color-accent);
	}
</style>
