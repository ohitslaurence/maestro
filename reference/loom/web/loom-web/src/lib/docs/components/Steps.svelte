<!--
 Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
 SPDX-License-Identifier: Proprietary
-->
<script lang="ts">
	import type { Snippet } from 'svelte';

	interface Props {
		children: Snippet;
	}

	let { children }: Props = $props();
</script>

<ol class="steps">
	{@render children()}
</ol>

<style>
	.steps {
		list-style: none;
		padding: 0;
		margin: var(--space-4) 0;
		counter-reset: step-counter;
	}

	.steps :global(li) {
		counter-increment: step-counter;
		position: relative;
		padding-left: var(--space-10);
		padding-bottom: var(--space-6);
		border-left: 1px solid var(--color-border);
		margin-left: var(--space-3);
	}

	.steps :global(li:last-child) {
		border-left: none;
		padding-bottom: 0;
	}

	.steps :global(li::before) {
		content: counter(step-counter);
		position: absolute;
		left: calc(-1 * var(--space-3) - 1px);
		top: 0;
		width: var(--space-6);
		height: var(--space-6);
		background: var(--color-accent);
		color: var(--color-bg);
		border-radius: var(--radius-full);
		display: flex;
		align-items: center;
		justify-content: center;
		font-family: var(--font-mono);
		font-size: var(--text-xs);
		font-weight: 600;
	}
</style>
