<!--
  Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
  SPDX-License-Identifier: Proprietary
-->
<script lang="ts">
	import { goto } from '$app/navigation';
	import type { Repository, Branch, TreeEntry } from '$lib/api/repos';
	import { TreeView, BranchSelector, OpenInWeaverButton, MarkdownContent } from '$lib/components/repos';
	import { Skeleton } from '$lib/ui';

	interface Props {
		data: {
			repo: Repository;
			branches: Branch[];
			entries: TreeEntry[];
			currentRef: string;
			currentPath: string;
			readme?: string;
			readmeFilename?: string;
		};
	}

	let { data }: Props = $props();

	function handleBranchChange(ref: string) {
		const path = data.currentPath ? `/${data.currentPath}` : '';
		goto(`/repos/${data.repo.owner_id}/${data.repo.name}/tree/${ref}${path}`);
	}

	const isMarkdown = $derived(
		data.readmeFilename?.toLowerCase().endsWith('.md') ?? false
	);
</script>

<svelte:head>
	<title>{data.currentPath || data.repo.name} - {data.repo.owner_id}/{data.repo.name}</title>
</svelte:head>

<div class="space-y-4">
	<div class="flex items-center justify-between">
		<div class="flex items-center gap-4">
			<BranchSelector
				branches={data.branches}
				currentRef={data.currentRef}
				onSelect={handleBranchChange}
			/>
		</div>
		<OpenInWeaverButton repo={data.repo} />
	</div>

	<TreeView
		entries={data.entries}
		owner={data.repo.owner_id}
		repo={data.repo.name}
		currentRef={data.currentRef}
		currentPath={data.currentPath}
	/>

	{#if data.readme}
		<div class="readme-section">
			<div class="readme-header">
				<svg class="readme-icon" viewBox="0 0 16 16" fill="currentColor">
					<path d="M0 1.75A.75.75 0 0 1 .75 1h4.253c1.227 0 2.317.59 3 1.501A3.743 3.743 0 0 1 11.006 1h4.245a.75.75 0 0 1 .75.75v10.5a.75.75 0 0 1-.75.75h-4.507a2.25 2.25 0 0 0-1.591.659l-.622.621a.75.75 0 0 1-1.06 0l-.622-.621A2.25 2.25 0 0 0 5.258 13H.75a.75.75 0 0 1-.75-.75Zm7.251 10.324.004-5.073-.002-2.253A2.25 2.25 0 0 0 5.003 2.5H1.5v9h3.757a3.75 3.75 0 0 1 1.994.574ZM8.755 4.75l-.004 7.322a3.752 3.752 0 0 1 1.992-.572H14.5v-9h-3.495a2.25 2.25 0 0 0-2.25 2.25Z"/>
				</svg>
				<span class="readme-filename">{data.readmeFilename}</span>
			</div>
			{#if isMarkdown}
				<MarkdownContent content={data.readme} />
			{:else}
				<pre class="readme-plain">{data.readme}</pre>
			{/if}
		</div>
	{/if}
</div>

<style>
	.readme-section {
		margin-top: var(--space-6);
	}

	.readme-header {
		display: flex;
		align-items: center;
		gap: var(--space-2);
		padding: var(--space-3) var(--space-4);
		background: var(--color-bg-muted);
		border: 1px solid var(--color-border);
		border-bottom: none;
		border-radius: var(--radius-md) var(--radius-md) 0 0;
	}

	.readme-icon {
		width: 16px;
		height: 16px;
		color: var(--color-fg-muted);
	}

	.readme-filename {
		font-family: var(--font-mono);
		font-size: var(--text-sm);
		font-weight: 500;
		color: var(--color-fg);
	}

	.readme-section :global(.markdown-content) {
		border-top-left-radius: 0;
		border-top-right-radius: 0;
	}

	.readme-plain {
		padding: var(--space-6);
		background: var(--color-bg);
		border: 1px solid var(--color-border);
		border-radius: 0 0 var(--radius-md) var(--radius-md);
		font-family: var(--font-mono);
		font-size: var(--text-sm);
		color: var(--color-fg);
		white-space: pre-wrap;
		word-break: break-word;
		margin: 0;
	}
</style>
