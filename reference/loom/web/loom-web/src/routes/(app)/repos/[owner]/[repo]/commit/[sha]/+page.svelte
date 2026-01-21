<!--
  Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
  SPDX-License-Identifier: Proprietary
-->
<script lang="ts">
	import type { Repository, CommitWithDiff } from '$lib/api/repos';
	import { CommitDiff } from '$lib/components/repos';

	interface Props {
		data: {
			repo: Repository;
			commit: CommitWithDiff;
		};
	}

	let { data }: Props = $props();

	const title = $derived(data.commit.message.split('\n')[0]);
</script>

<svelte:head>
	<title>{title} - {data.repo.owner_id}/{data.repo.name}</title>
</svelte:head>

<CommitDiff
	commit={data.commit}
	owner={data.repo.owner_id}
	repo={data.repo.name}
/>
