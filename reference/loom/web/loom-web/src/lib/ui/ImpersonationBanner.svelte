<!--
  Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
  SPDX-License-Identifier: Proprietary
-->
<script lang="ts">
	import { getApiClient } from '$lib/api/client';
	import { i18n } from '$lib/i18n';
	import type { ImpersonationState } from '$lib/api/types';
	import Button from './Button.svelte';

	interface Props {
		impersonation: ImpersonationState;
		onStop?: () => void;
	}

	let { impersonation, onStop }: Props = $props();

	let isStopping = $state(false);

	async function handleStop() {
		isStopping = true;
		try {
			const client = getApiClient();
			await client.stopImpersonation();
			onStop?.();
			window.location.reload();
		} catch (e) {
			console.error('Failed to stop impersonation:', e);
		} finally {
			isStopping = false;
		}
	}
</script>

{#if impersonation.is_impersonating && impersonation.original_user && impersonation.impersonated_user}
	<div class="impersonation-banner">
		<div class="impersonation-content">
			<svg class="impersonation-icon" fill="none" stroke="currentColor" viewBox="0 0 24 24">
				<path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-3L13.732 4c-.77-1.333-2.694-1.333-3.464 0L3.34 16c-.77 1.333.192 3 1.732 3z" />
			</svg>
			<span class="impersonation-text">
				{i18n._('admin.impersonation.banner', {
					admin: impersonation.original_user.display_name,
					user: impersonation.impersonated_user.display_name,
				})}
			</span>
		</div>
		<Button
			variant="secondary"
			size="sm"
			disabled={isStopping}
			loading={isStopping}
			onclick={handleStop}
		>
			{i18n._('admin.impersonation.stop')}
		</Button>
	</div>
{/if}

<style>
	.impersonation-banner {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: var(--space-4);
		padding: var(--space-2) var(--space-4);
		background: var(--color-warning);
		color: var(--color-bg);
		font-family: var(--font-mono);
	}

	.impersonation-content {
		display: flex;
		align-items: center;
		gap: var(--space-2);
	}

	.impersonation-icon {
		width: 20px;
		height: 20px;
		flex-shrink: 0;
	}

	.impersonation-text {
		font-size: var(--text-sm);
		font-weight: 500;
	}
</style>
