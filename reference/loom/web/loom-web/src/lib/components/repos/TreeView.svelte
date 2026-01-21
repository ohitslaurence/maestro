<!--
  Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
  SPDX-License-Identifier: Proprietary
-->
<script lang="ts">
	import type { TreeEntry } from '$lib/api/repos';
	import { i18n } from '$lib/i18n';

	interface Props {
		entries: TreeEntry[];
		owner: string;
		repo: string;
		currentRef: string;
		currentPath: string;
	}

	let { entries, owner, repo, currentRef, currentPath }: Props = $props();

	const basePath = $derived(`/repos/${owner}/${repo}`);

	const sortedEntries = $derived(
		[...entries].sort((a, b) => {
			if (a.kind === 'directory' && b.kind !== 'directory') return -1;
			if (a.kind !== 'directory' && b.kind === 'directory') return 1;
			return a.name.localeCompare(b.name);
		})
	);

	function getIcon(entry: TreeEntry): string {
		if (entry.kind === 'directory') return 'folder';
		if (entry.kind === 'submodule') return 'submodule';
		if (entry.kind === 'symlink') return 'symlink';

		const ext = entry.name.split('.').pop()?.toLowerCase();
		if (['ts', 'tsx', 'js', 'jsx'].includes(ext ?? '')) return 'code';
		if (['md', 'mdx', 'txt', 'rst'].includes(ext ?? '')) return 'doc';
		if (['json', 'yaml', 'yml', 'toml', 'xml'].includes(ext ?? '')) return 'config';
		if (['png', 'jpg', 'jpeg', 'gif', 'svg', 'webp'].includes(ext ?? '')) return 'image';
		return 'file';
	}

	function formatSize(size: number | undefined): string {
		if (size === undefined) return '';
		if (size < 1024) return `${size} B`;
		if (size < 1024 * 1024) return `${(size / 1024).toFixed(1)} KB`;
		return `${(size / 1024 / 1024).toFixed(1)} MB`;
	}

	function getEntryHref(entry: TreeEntry): string {
		const pathPrefix = currentPath ? `${currentPath}/` : '';
		if (entry.kind === 'directory') {
			return `${basePath}/tree/${currentRef}/${pathPrefix}${entry.name}`;
		}
		return `${basePath}/blob/${currentRef}/${pathPrefix}${entry.name}`;
	}

	const breadcrumbs = $derived.by(() => {
		if (!currentPath) return [];
		const parts = currentPath.split('/');
		return parts.map((part, i) => ({
			name: part,
			path: parts.slice(0, i + 1).join('/'),
		}));
	});
</script>

{#if currentPath}
	<div class="breadcrumbs">
		<a href="{basePath}/tree/{currentRef}" class="breadcrumb-link">
			{repo}
		</a>
		{#each breadcrumbs as crumb, i}
			<span class="breadcrumb-separator">/</span>
			{#if i === breadcrumbs.length - 1}
				<span class="breadcrumb-current">{crumb.name}</span>
			{:else}
				<a href="{basePath}/tree/{currentRef}/{crumb.path}" class="breadcrumb-link">
					{crumb.name}
				</a>
			{/if}
		{/each}
	</div>
{/if}

<div class="tree-container">
	{#if currentPath}
		<a
			href="{basePath}/tree/{currentRef}/{currentPath.split('/').slice(0, -1).join('/')}"
			class="tree-entry tree-entry-parent"
		>
			<svg class="entry-icon" fill="none" stroke="currentColor" viewBox="0 0 24 24">
				<path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M11 17l-5-5m0 0l5-5m-5 5h12" />
			</svg>
			<span class="entry-name-muted">..</span>
		</a>
	{/if}

	{#each sortedEntries as entry, i}
		<a
			href={getEntryHref(entry)}
			class="tree-entry"
			class:tree-entry-last={i === sortedEntries.length - 1}
		>
			{#if getIcon(entry) === 'folder'}
				<svg class="entry-icon entry-icon-accent" fill="currentColor" viewBox="0 0 20 20">
					<path d="M2 6a2 2 0 012-2h5l2 2h5a2 2 0 012 2v6a2 2 0 01-2 2H4a2 2 0 01-2-2V6z" />
				</svg>
			{:else if getIcon(entry) === 'submodule'}
				<svg class="entry-icon entry-icon-warning" fill="none" stroke="currentColor" viewBox="0 0 24 24">
					<path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M10 6H6a2 2 0 00-2 2v10a2 2 0 002 2h10a2 2 0 002-2v-4M14 4h6m0 0v6m0-6L10 14" />
				</svg>
			{:else if getIcon(entry) === 'symlink'}
				<svg class="entry-icon entry-icon-muted" fill="none" stroke="currentColor" viewBox="0 0 24 24">
					<path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M13.828 10.172a4 4 0 00-5.656 0l-4 4a4 4 0 105.656 5.656l1.102-1.101m-.758-4.899a4 4 0 005.656 0l4-4a4 4 0 00-5.656-5.656l-1.1 1.1" />
				</svg>
			{:else if getIcon(entry) === 'code'}
				<svg class="entry-icon entry-icon-success" fill="none" stroke="currentColor" viewBox="0 0 24 24">
					<path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M10 20l4-16m4 4l4 4-4 4M6 16l-4-4 4-4" />
				</svg>
			{:else if getIcon(entry) === 'doc'}
				<svg class="entry-icon entry-icon-muted" fill="none" stroke="currentColor" viewBox="0 0 24 24">
					<path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 12h6m-6 4h6m2 5H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z" />
				</svg>
			{:else if getIcon(entry) === 'config'}
				<svg class="entry-icon entry-icon-warning" fill="none" stroke="currentColor" viewBox="0 0 24 24">
					<path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.065 2.572c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.572 1.065c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.065-2.572c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.572-1.065z" />
				</svg>
			{:else if getIcon(entry) === 'image'}
				<svg class="entry-icon entry-icon-accent" fill="none" stroke="currentColor" viewBox="0 0 24 24">
					<path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 16l4.586-4.586a2 2 0 012.828 0L16 16m-2-2l1.586-1.586a2 2 0 012.828 0L20 14m-6-6h.01M6 20h12a2 2 0 002-2V6a2 2 0 00-2-2H6a2 2 0 00-2 2v12a2 2 0 002 2z" />
				</svg>
			{:else}
				<svg class="entry-icon entry-icon-muted" fill="none" stroke="currentColor" viewBox="0 0 24 24">
					<path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M7 21h10a2 2 0 002-2V9.414a1 1 0 00-.293-.707l-5.414-5.414A1 1 0 0012.586 3H7a2 2 0 00-2 2v14a2 2 0 002 2z" />
				</svg>
			{/if}

			<span class="entry-name">{entry.name}</span>

			{#if entry.kind === 'file' && entry.size !== undefined}
				<span class="entry-size">{formatSize(entry.size)}</span>
			{/if}
		</a>
	{/each}

	{#if sortedEntries.length === 0}
		<div class="tree-empty">
			{i18n.t('client.repos.tree.empty')}
		</div>
	{/if}
</div>

<style>
	.breadcrumbs {
		display: flex;
		align-items: center;
		gap: var(--space-1);
		font-family: var(--font-mono);
		font-size: var(--text-sm);
		margin-bottom: var(--space-4);
		overflow-x: auto;
	}

	.breadcrumb-link {
		color: var(--color-accent);
		font-weight: 500;
	}

	.breadcrumb-link:hover {
		text-decoration: underline;
	}

	.breadcrumb-separator {
		color: var(--color-fg-muted);
	}

	.breadcrumb-current {
		font-weight: 500;
		color: var(--color-fg);
	}

	.tree-container {
		border: 1px solid var(--color-border);
		border-radius: var(--radius-md);
		overflow: hidden;
	}

	.tree-entry {
		display: flex;
		align-items: center;
		gap: var(--space-3);
		padding: var(--space-2) var(--space-4);
		border-bottom: 1px solid var(--color-border);
		transition: background 0.15s ease;
	}

	.tree-entry:hover {
		background: var(--color-bg-muted);
	}

	.tree-entry-parent {
		background: var(--color-bg-muted);
	}

	.tree-entry-parent:hover {
		background: var(--color-bg-subtle);
	}

	.tree-entry-last {
		border-bottom: none;
	}

	.entry-icon {
		width: 1.25rem;
		height: 1.25rem;
		flex-shrink: 0;
	}

	.entry-icon-accent {
		color: var(--color-accent);
	}

	.entry-icon-success {
		color: var(--color-success);
	}

	.entry-icon-warning {
		color: var(--color-warning);
	}

	.entry-icon-muted {
		color: var(--color-fg-muted);
	}

	.entry-name {
		font-family: var(--font-mono);
		font-size: var(--text-sm);
		color: var(--color-fg);
		flex: 1;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.entry-name-muted {
		font-family: var(--font-mono);
		font-size: var(--text-sm);
		color: var(--color-fg-muted);
	}

	.entry-size {
		font-family: var(--font-mono);
		font-size: var(--text-xs);
		color: var(--color-fg-muted);
		flex-shrink: 0;
	}

	.tree-empty {
		padding: var(--space-8) var(--space-4);
		text-align: center;
		font-family: var(--font-mono);
		color: var(--color-fg-muted);
	}
</style>
