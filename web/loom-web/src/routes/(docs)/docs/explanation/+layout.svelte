<!--
 Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
 SPDX-License-Identifier: Proprietary
-->
<script lang="ts">
	import { page } from '$app/stores';
	import { DocPage } from '$lib/docs/components';
	import { findPrevNext } from '$lib/docs';
	import type { LayoutData } from '../$types';

	interface Props {
		data: LayoutData;
		children: import('svelte').Snippet;
	}

	let { data, children }: Props = $props();

	const currentPath = $derived($page.url.pathname);
	const isIndexPage = $derived(currentPath === '/docs/explanation');
	const currentDoc = $derived(data.docs?.find((d) => d.urlPath === currentPath));
	const prevNext = $derived(findPrevNext(data.docs ?? [], currentPath));
</script>

{#if isIndexPage}
	{@render children()}
{:else if currentDoc}
	<DocPage
		title={currentDoc.meta.title}
		category={currentDoc.category}
		prev={prevNext.prev}
		next={prevNext.next}
	>
		{@render children()}
	</DocPage>
{:else}
	{@render children()}
{/if}
