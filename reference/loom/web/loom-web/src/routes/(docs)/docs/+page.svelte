<!--
 Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
 SPDX-License-Identifier: Proprietary
-->
<script lang="ts">
	import { goto } from '$app/navigation';
	import { i18n } from '$lib/i18n';
	import { onMount } from 'svelte';

	onMount(() => {
		goto('/docs/tutorials', { replaceState: true });
	});
</script>

<div class="redirect-message">
	<p>{i18n.t('docs.redirecting')}</p>
</div>

<style>
	.redirect-message {
		font-family: var(--font-mono);
		color: var(--color-fg-muted);
		padding: var(--space-8);
	}
</style>
