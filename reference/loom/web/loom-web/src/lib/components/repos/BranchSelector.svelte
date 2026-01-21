<!--
  Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
  SPDX-License-Identifier: Proprietary
-->
<script lang="ts">
	import type { Branch } from '$lib/api/repos';
	import { i18n } from '$lib/i18n';

	interface Props {
		branches: Branch[];
		currentRef: string;
		onSelect: (ref: string) => void;
	}

	let { branches, currentRef, onSelect }: Props = $props();

	let open = $state(false);
	let search = $state('');

	const filteredBranches = $derived(
		branches.filter((b) => b.name.toLowerCase().includes(search.toLowerCase()))
	);

	function selectBranch(name: string) {
		onSelect(name);
		open = false;
		search = '';
	}
</script>

<div class="branch-selector">
	<button
		type="button"
		onclick={() => (open = !open)}
		class="branch-trigger"
	>
		<svg class="branch-icon" fill="none" stroke="currentColor" viewBox="0 0 24 24">
			<path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M13 10V3L4 14h7v7l9-11h-7z" />
		</svg>
		<span class="branch-name">{currentRef}</span>
		<svg class="chevron-icon" fill="none" stroke="currentColor" viewBox="0 0 24 24">
			<path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 9l-7 7-7-7" />
		</svg>
	</button>

	{#if open}
		<div class="branch-dropdown">
			<div class="branch-search-container">
				<input
					type="text"
					placeholder={i18n.t('client.repos.branch.find')}
					bind:value={search}
					class="branch-search"
				/>
			</div>
			<div class="branch-list">
				{#if filteredBranches.length === 0}
					<div class="branch-empty">{i18n.t('client.repos.branch.not_found')}</div>
				{:else}
					{#each filteredBranches as branch}
						<button
							type="button"
							onclick={() => selectBranch(branch.name)}
							class="branch-item"
							class:branch-item-active={branch.name === currentRef}
						>
							<svg class="branch-item-icon" fill="none" stroke="currentColor" viewBox="0 0 24 24">
								<path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M13 10V3L4 14h7v7l9-11h-7z" />
							</svg>
							<span class="branch-item-name">{branch.name}</span>
							{#if branch.is_default}
								<span class="branch-default-badge">{i18n.t('client.repos.branch.default')}</span>
							{/if}
							{#if branch.name === currentRef}
								<svg class="branch-check-icon" fill="currentColor" viewBox="0 0 20 20">
									<path fill-rule="evenodd" d="M16.707 5.293a1 1 0 010 1.414l-8 8a1 1 0 01-1.414 0l-4-4a1 1 0 011.414-1.414L8 12.586l7.293-7.293a1 1 0 011.414 0z" clip-rule="evenodd" />
								</svg>
							{/if}
						</button>
					{/each}
				{/if}
			</div>
		</div>
	{/if}
</div>

<style>
	.branch-selector {
		position: relative;
	}

	.branch-trigger {
		display: flex;
		align-items: center;
		gap: var(--space-2);
		padding: var(--space-1) var(--space-3);
		font-family: var(--font-mono);
		font-size: var(--text-sm);
		font-weight: 500;
		background: var(--color-bg-muted);
		border: 1px solid var(--color-border);
		border-radius: var(--radius-md);
		color: var(--color-fg);
		cursor: pointer;
		transition: background 0.15s ease;
	}

	.branch-trigger:hover {
		background: var(--color-bg-subtle);
	}

	.branch-icon,
	.chevron-icon {
		width: 1rem;
		height: 1rem;
		color: var(--color-fg-muted);
	}

	.branch-name {
		font-family: var(--font-mono);
	}

	.branch-dropdown {
		position: absolute;
		left: 0;
		margin-top: var(--space-1);
		width: 16rem;
		background: var(--color-bg);
		border: 1px solid var(--color-border);
		border-radius: var(--radius-md);
		box-shadow: var(--shadow-lg);
		z-index: 20;
	}

	.branch-search-container {
		padding: var(--space-2);
		border-bottom: 1px solid var(--color-border);
	}

	.branch-search {
		width: 100%;
		padding: var(--space-1) var(--space-2);
		font-family: var(--font-mono);
		font-size: var(--text-sm);
		background: var(--color-bg-muted);
		border: 1px solid var(--color-border);
		border-radius: var(--radius-md);
		color: var(--color-fg);
		outline: none;
	}

	.branch-search:focus {
		border-color: var(--color-accent);
		box-shadow: 0 0 0 2px var(--color-accent-soft);
	}

	.branch-list {
		max-height: 16rem;
		overflow-y: auto;
		padding: var(--space-1) 0;
	}

	.branch-empty {
		padding: var(--space-2) var(--space-3);
		font-family: var(--font-mono);
		font-size: var(--text-sm);
		color: var(--color-fg-muted);
	}

	.branch-item {
		display: flex;
		align-items: center;
		gap: var(--space-2);
		width: 100%;
		padding: var(--space-2) var(--space-3);
		font-family: var(--font-mono);
		font-size: var(--text-sm);
		text-align: left;
		background: transparent;
		border: none;
		color: var(--color-fg);
		cursor: pointer;
		transition: background 0.15s ease;
	}

	.branch-item:hover {
		background: var(--color-bg-muted);
	}

	.branch-item-active {
		background: var(--color-accent-soft);
	}

	.branch-item-icon {
		width: 1rem;
		height: 1rem;
		flex-shrink: 0;
		color: var(--color-fg-muted);
	}

	.branch-item-name {
		flex: 1;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.branch-default-badge {
		margin-left: auto;
		padding: 2px var(--space-1);
		font-size: var(--text-xs);
		background: var(--color-bg-subtle);
		border-radius: var(--radius-md);
		color: var(--color-fg-muted);
	}

	.branch-check-icon {
		width: 1rem;
		height: 1rem;
		flex-shrink: 0;
		margin-left: auto;
		color: var(--color-accent);
	}
</style>
