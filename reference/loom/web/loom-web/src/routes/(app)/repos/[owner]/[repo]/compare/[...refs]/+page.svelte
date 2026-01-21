<!--
  Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
  SPDX-License-Identifier: Proprietary
-->
<script lang="ts">
	import { goto } from '$app/navigation';
	import type { Repository, Branch, CompareResult } from '$lib/api/repos';
	import { CompareView, BranchSelector } from '$lib/components/repos';
	import { i18n } from '$lib/i18n';

	interface Props {
		data: {
			repo: Repository;
			branches: Branch[];
			result: CompareResult;
			baseRef: string;
			headRef: string;
		};
	}

	let { data }: Props = $props();

	function handleBaseChange(ref: string) {
		goto(`/repos/${data.repo.owner_id}/${data.repo.name}/compare/${ref}...${data.headRef}`);
	}

	function handleHeadChange(ref: string) {
		goto(`/repos/${data.repo.owner_id}/${data.repo.name}/compare/${data.baseRef}...${ref}`);
	}
</script>

<svelte:head>
	<title>{i18n('client.repos.compare.title')} {data.baseRef}...{data.headRef} - {data.repo.owner_id}/{data.repo.name}</title>
</svelte:head>

<div class="space-y-4">
	<div class="flex items-center gap-4 flex-wrap">
		<div class="flex items-center gap-2">
			<span class="text-sm text-fg-muted">{i18n('client.repos.compare.base')}</span>
			<BranchSelector
				branches={data.branches}
				currentRef={data.baseRef}
				onSelect={handleBaseChange}
			/>
		</div>

		<svg class="w-5 h-5 text-fg-muted" fill="none" stroke="currentColor" viewBox="0 0 24 24">
			<path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M14 5l7 7m0 0l-7 7m7-7H3" />
		</svg>

		<div class="flex items-center gap-2">
			<span class="text-sm text-fg-muted">{i18n('client.repos.compare.compare')}</span>
			<BranchSelector
				branches={data.branches}
				currentRef={data.headRef}
				onSelect={handleHeadChange}
			/>
		</div>
	</div>

	<CompareView
		result={data.result}
		owner={data.repo.owner_id}
		repo={data.repo.name}
	/>
</div>
