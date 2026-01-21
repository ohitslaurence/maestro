<!--
  Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
  SPDX-License-Identifier: Proprietary
-->
<script lang="ts">
	import type { Snippet } from 'svelte';
	import type { Repository, Branch } from '$lib/api/repos';
	import { RepoHeader, RepoNav } from '$lib/components/repos';

	interface Props {
		children: Snippet;
		data: {
			repo: Repository;
			branches: Branch[];
		};
	}

	let { children, data }: Props = $props();
</script>

<div class="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8 py-6">
	<RepoHeader repo={data.repo} />
	<RepoNav
		owner={data.repo.owner_id}
		repo={data.repo.name}
		defaultBranch={data.repo.default_branch}
	/>
	{@render children()}
</div>
