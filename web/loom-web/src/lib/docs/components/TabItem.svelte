<!--
 Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
 SPDX-License-Identifier: Proprietary
-->
<script lang="ts">
	import type { Snippet } from 'svelte';
	import { getContext, onMount } from 'svelte';

	interface Props {
		label: string;
		children: Snippet;
	}

	let { label, children }: Props = $props();
	let index = $state(-1);
	let visible = $state(false);

	const tabs = getContext<{
		registerTab: (label: string) => number;
		isActive: (index: number) => boolean;
	}>('tabs');

	onMount(() => {
		if (tabs) {
			index = tabs.registerTab(label);
		}
	});

	$effect(() => {
		if (tabs && index >= 0) {
			visible = tabs.isActive(index);
		}
	});
</script>

{#if visible}
	<div class="tab-panel" role="tabpanel">
		{@render children()}
	</div>
{/if}

<style>
	.tab-panel :global(pre) {
		margin: 0;
		border-radius: 0;
	}
</style>
