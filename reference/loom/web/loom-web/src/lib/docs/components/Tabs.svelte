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
	let activeTab = $state(0);
	let tabLabels = $state<string[]>([]);

	export function registerTab(label: string): number {
		const index = tabLabels.length;
		tabLabels = [...tabLabels, label];
		return index;
	}

	export function isActive(index: number): boolean {
		return activeTab === index;
	}

	export function setActive(index: number): void {
		activeTab = index;
	}
</script>

<div class="tabs">
	<div class="tabs-header" role="tablist">
		{#each tabLabels as label, i}
			<button
				class="tab-button"
				class:active={activeTab === i}
				role="tab"
				aria-selected={activeTab === i}
				onclick={() => (activeTab = i)}
			>
				{label}
			</button>
		{/each}
	</div>
	<div class="tabs-content">
		{@render children()}
	</div>
</div>

<style>
	.tabs {
		margin: var(--space-4) 0;
		border: 1px solid var(--color-border);
		border-radius: var(--radius-md);
		overflow: hidden;
	}

	.tabs-header {
		display: flex;
		background: var(--color-bg-muted);
		border-bottom: 1px solid var(--color-border);
	}

	.tab-button {
		font-family: var(--font-mono);
		font-size: var(--text-sm);
		padding: var(--space-2) var(--space-4);
		background: transparent;
		border: none;
		color: var(--color-fg-muted);
		cursor: pointer;
		transition: all 0.15s ease;
		border-bottom: 2px solid transparent;
		margin-bottom: -1px;
	}

	.tab-button:hover {
		color: var(--color-fg);
		background: var(--color-bg-subtle);
	}

	.tab-button.active {
		color: var(--color-accent);
		border-bottom-color: var(--color-accent);
		background: var(--color-bg);
	}

	.tabs-content {
		padding: var(--space-4);
		background: var(--color-bg);
	}
</style>
