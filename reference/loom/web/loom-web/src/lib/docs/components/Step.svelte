<!--
 Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
 SPDX-License-Identifier: Proprietary
-->
<script lang="ts">
	import type { Snippet } from 'svelte';

	interface Props {
		title?: string;
		children: Snippet;
	}

	let { title, children }: Props = $props();
</script>

<li class="step">
	{#if title}
		<h4 class="step-title">{title}</h4>
	{/if}
	<div class="step-content">
		{@render children()}
	</div>
</li>

<style>
	.step {
		font-family: var(--font-mono);
	}

	.step-title {
		font-size: var(--text-base);
		font-weight: 600;
		color: var(--color-fg);
		margin: 0 0 var(--space-2) 0;
	}

	.step-content {
		font-size: var(--text-sm);
		color: var(--color-fg-muted);
		line-height: 1.6;
	}

	.step-content :global(p:last-child) {
		margin-bottom: 0;
	}
</style>
