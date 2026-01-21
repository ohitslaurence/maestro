<!--
  Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
  SPDX-License-Identifier: Proprietary
-->
<script lang="ts">
	import { goto } from '$app/navigation';
	import type { Repository, Branch } from '$lib/api/repos';
	import { BlobView, BranchSelector } from '$lib/components/repos';

	interface Props {
		data: {
			repo: Repository;
			branches: Branch[];
			content: string;
			currentRef: string;
			currentPath: string;
		};
	}

	let { data }: Props = $props();

	const fileName = $derived(data.currentPath.split('/').pop() ?? '');
	const dirPath = $derived(data.currentPath.split('/').slice(0, -1).join('/'));

	function handleBranchChange(ref: string) {
		goto(`/repos/${data.repo.owner_id}/${data.repo.name}/blob/${ref}/${data.currentPath}`);
	}

	const breadcrumbs = $derived.by(() => {
		const parts = data.currentPath.split('/');
		return parts.slice(0, -1).map((part, i) => ({
			name: part,
			path: parts.slice(0, i + 1).join('/'),
		}));
	});

	const basePath = $derived(`/repos/${data.repo.owner_id}/${data.repo.name}`);
</script>

<svelte:head>
	<title>{fileName} - {data.repo.owner_id}/{data.repo.name}</title>
</svelte:head>

<div class="space-y-4">
	<div class="flex items-center justify-between">
		<div class="flex items-center gap-4">
			<BranchSelector
				branches={data.branches}
				currentRef={data.currentRef}
				onSelect={handleBranchChange}
			/>

			<div class="flex items-center gap-1 text-sm overflow-x-auto">
				<a href="{basePath}/tree/{data.currentRef}" class="text-accent hover:underline font-medium">
					{data.repo.name}
				</a>
				{#each breadcrumbs as crumb}
					<span class="text-fg-muted">/</span>
					<a href="{basePath}/tree/{data.currentRef}/{crumb.path}" class="text-accent hover:underline">
						{crumb.name}
					</a>
				{/each}
				<span class="text-fg-muted">/</span>
				<span class="font-medium text-fg">{fileName}</span>
			</div>
		</div>
	</div>

	<BlobView
		content={data.content}
		path={data.currentPath}
		owner={data.repo.owner_id}
		repo={data.repo.name}
		currentRef={data.currentRef}
	/>
</div>
