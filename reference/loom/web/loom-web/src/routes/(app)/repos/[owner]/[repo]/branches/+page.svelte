<!--
  Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
  SPDX-License-Identifier: Proprietary
-->
<script lang="ts">
	import type { Repository, Branch } from '$lib/api/repos';
	import { Badge, Button } from '$lib/ui';
	import { i18n } from '$lib/i18n';

	interface Props {
		data: {
			repo: Repository;
			branches: Branch[];
		};
	}

	let { data }: Props = $props();

	const basePath = $derived(`/repos/${data.repo.owner_id}/${data.repo.name}`);

	const sortedBranches = $derived(
		[...data.branches].sort((a, b) => {
			if (a.is_default) return -1;
			if (b.is_default) return 1;
			return a.name.localeCompare(b.name);
		})
	);
</script>

<svelte:head>
	<title>Branches - {data.repo.owner_id}/{data.repo.name}</title>
</svelte:head>

<div class="space-y-4">
	<div class="flex items-center justify-between">
		<h2 class="text-lg font-medium text-fg">
			<strong class="text-fg">{data.branches.length}</strong> {i18n._('client.repos.branches.title')}
		</h2>
	</div>

	<div class="border border-border rounded-lg divide-y divide-border">
		{#each sortedBranches as branch}
			<div class="flex items-center justify-between px-4 py-3 hover:bg-bg-muted">
				<div class="flex items-center gap-3 min-w-0">
					<svg class="w-5 h-5 text-fg-muted flex-shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24">
						<path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M13 10V3L4 14h7v7l9-11h-7z" />
					</svg>
					<a href="{basePath}/tree/{branch.name}" class="font-mono text-sm text-fg hover:text-accent truncate">
						{branch.name}
					</a>
					{#if branch.is_default}
						<Badge variant="accent" size="sm">{i18n._('client.repos.branches.default')}</Badge>
					{/if}
				</div>

				<div class="flex items-center gap-2 flex-shrink-0">
					<code class="font-mono text-xs text-fg-muted bg-bg-muted px-2 py-0.5 rounded">
						{branch.sha.slice(0, 7)}
					</code>

					{#if !branch.is_default}
						<a href="{basePath}/compare/{data.repo.default_branch}...{branch.name}">
							<Button variant="ghost" size="sm">{i18n._('client.repos.branches.compare')}</Button>
						</a>
					{/if}
				</div>
			</div>
		{/each}

		{#if data.branches.length === 0}
			<div class="px-4 py-8 text-center text-fg-muted">
				{i18n._('client.repos.branches.empty')}
			</div>
		{/if}
	</div>
</div>
