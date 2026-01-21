<!--
  Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
  SPDX-License-Identifier: Proprietary
-->
<script lang="ts">
	import { goto } from '$app/navigation';
	import type { PageData } from './$types';

	interface Props {
		data: PageData;
	}

	let { data }: Props = $props();

	$effect(() => {
		goto(`/repos/${data.repo.owner_id}/${data.repo.name}/tree/${data.repo.default_branch}`, { replaceState: true });
	});
</script>

<div class="flex items-center justify-center py-12">
	<div class="animate-spin w-8 h-8 border-4 border-accent border-t-transparent rounded-full"></div>
</div>
