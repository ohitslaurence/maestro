<!--
 Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
 SPDX-License-Identifier: Proprietary
-->
<script lang="ts">
	import type { PageData } from './$types';
	import { i18n } from '$lib/i18n';
	import { LinkCard } from '$lib/docs/components';

	interface Props {
		data: PageData;
	}

	let { data }: Props = $props();

	const guides = $derived(
		data.docs?.filter((d) => d.category === 'how-to').sort((a, b) => a.meta.order - b.meta.order) ?? []
	);
</script>

<div class="category-page">
	<h1>{i18n.t('docs.howTo.title')}</h1>
	<p class="category-description">
		{i18n.t('docs.howTo.description')}
	</p>

	<div class="doc-list">
		{#each guides as doc}
			<LinkCard href={doc.urlPath} title={doc.meta.title} description={doc.meta.summary} />
		{/each}

		{#if guides.length === 0}
			<p class="empty-message">{i18n.t('docs.howTo.empty')}</p>
		{/if}
	</div>
</div>

<style>
	.category-page {
		font-family: var(--font-mono);
	}

	h1 {
		font-size: var(--text-2xl);
		font-weight: 600;
		color: var(--color-fg);
		margin: 0 0 var(--space-2) 0;
	}

	.category-description {
		font-size: var(--text-base);
		color: var(--color-fg-muted);
		margin: 0 0 var(--space-6) 0;
	}

	.doc-list {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
	}

	.empty-message {
		color: var(--color-fg-subtle);
		font-style: italic;
	}
</style>
