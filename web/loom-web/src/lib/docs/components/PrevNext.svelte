<!--
 Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
 SPDX-License-Identifier: Proprietary
-->
<script lang="ts">
	import { i18n } from '$lib/i18n';
	import type { NavItem } from '../types';

	interface Props {
		prev: NavItem | null;
		next: NavItem | null;
	}

	let { prev, next }: Props = $props();
</script>

{#if prev || next}
	<nav class="prev-next" aria-label="Page navigation">
		<div class="prev-next-grid">
			{#if prev}
				<a href={prev.path} class="prev-next-link prev">
					<span class="prev-next-label">{i18n('general.previous')}</span>
					<span class="prev-next-title">← {prev.title}</span>
				</a>
			{:else}
				<div></div>
			{/if}

			{#if next}
				<a href={next.path} class="prev-next-link next">
					<span class="prev-next-label">{i18n('general.next')}</span>
					<span class="prev-next-title">{next.title} →</span>
				</a>
			{/if}
		</div>
	</nav>
{/if}

<style>
	.prev-next {
		margin-top: var(--space-12);
		padding-top: var(--space-6);
		border-top: 1px solid var(--color-border);
	}

	.prev-next-grid {
		display: grid;
		grid-template-columns: 1fr 1fr;
		gap: var(--space-4);
	}

	.prev-next-link {
		display: flex;
		flex-direction: column;
		padding: var(--space-4);
		background: var(--color-bg-muted);
		border: 1px solid var(--color-border);
		border-radius: var(--radius-md);
		text-decoration: none;
		transition: all 0.15s ease;
		font-family: var(--font-mono);
	}

	.prev-next-link:hover {
		border-color: var(--color-accent);
		background: var(--color-bg-subtle);
	}

	.prev-next-link.next {
		text-align: right;
	}

	.prev-next-label {
		font-size: var(--text-xs);
		color: var(--color-fg-muted);
		text-transform: uppercase;
		letter-spacing: 0.05em;
		margin-bottom: var(--space-1);
	}

	.prev-next-title {
		font-size: var(--text-sm);
		color: var(--color-accent);
		font-weight: 500;
	}

	@media (max-width: 640px) {
		.prev-next-grid {
			grid-template-columns: 1fr;
		}

		.prev-next-link.next {
			text-align: left;
		}
	}
</style>
