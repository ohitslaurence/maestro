<!--
  Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
  SPDX-License-Identifier: Proprietary
-->
<script lang="ts">
	import { goto } from '$app/navigation';
	import { onMount } from 'svelte';

	onMount(() => {
		goto('/threads');
	});
</script>

<div class="dashboard-loader">
	<div class="spinner"></div>
</div>

<style>
	.dashboard-loader {
		display: flex;
		align-items: center;
		justify-content: center;
		min-height: 100vh;
		font-family: var(--font-mono);
		background: var(--color-bg);
	}

	.spinner {
		width: 32px;
		height: 32px;
		border: 3px solid var(--color-border);
		border-top-color: var(--color-accent);
		border-radius: var(--radius-full);
		animation: spin 0.8s linear infinite;
	}

	@keyframes spin {
		to {
			transform: rotate(360deg);
		}
	}
</style>
