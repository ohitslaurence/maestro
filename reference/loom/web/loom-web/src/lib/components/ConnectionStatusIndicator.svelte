<!--
  Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
  SPDX-License-Identifier: Proprietary
-->

<script lang="ts">
	import type { ConnectionStatus } from '../realtime/types';
	import { Badge } from '../ui';
	import { i18n } from '../i18n';

	interface Props {
		status: ConnectionStatus;
		attemptCount?: number;
		onreconnect?: () => void;
	}

	let { status, attemptCount = 0, onreconnect }: Props = $props();

	const statusConfig: Record<
		ConnectionStatus,
		{
			labelKey: string;
			variant: 'success' | 'warning' | 'error' | 'muted';
			dotColor: string;
			animation?: string;
		}
	> = {
		connected: {
			labelKey: 'connection.connected',
			variant: 'success',
			dotColor: 'var(--color-success)'
		},
		connecting: {
			labelKey: 'connection.connecting',
			variant: 'muted',
			dotColor: 'var(--color-thread)',
			animation: 'thread-weaving'
		},
		disconnected: {
			labelKey: 'connection.disconnected',
			variant: 'error',
			dotColor: 'var(--color-error)'
		},
		reconnecting: {
			labelKey: 'connection.reconnecting',
			variant: 'warning',
			dotColor: 'var(--color-warning)',
			animation: 'thread-weaving'
		},
		error: {
			labelKey: 'connection.error',
			variant: 'error',
			dotColor: 'var(--color-error)'
		}
	};

	const config = $derived(statusConfig[status]);
	const isClickable = $derived(
		(status === 'disconnected' || status === 'error') && onreconnect !== undefined
	);
	const label = $derived(i18n.t(config.labelKey));
	const attemptLabel = $derived(
		attemptCount > 0 && status === 'reconnecting'
			? i18n.t('connection.attempt').replace('{count}', String(attemptCount))
			: null
	);
	const title = $derived(isClickable ? i18n.t('connection.clickToReconnect') : undefined);

	function handleClick() {
		if (isClickable && onreconnect) {
			onreconnect();
		}
	}
</script>

<Badge variant={config.variant} size="sm">
	{#if isClickable}
		<button
			type="button"
			class="status-button"
			onclick={handleClick}
			{title}
		>
			<span class="status-dot" style:background={config.dotColor}></span>
			<span>{label}</span>
		</button>
	{:else}
		<span class="status-content">
			<span
				class="status-dot"
				class:animating={config.animation}
				style:background={config.dotColor}
			></span>
			<span>{label}</span>
			{#if attemptLabel}
				<span class="attempt-count">({attemptLabel})</span>
			{/if}
		</span>
	{/if}
</Badge>

<style>
	.status-button {
		display: inline-flex;
		align-items: center;
		gap: var(--space-2);
		cursor: pointer;
		background: transparent;
		border: none;
		padding: 0;
		margin: 0;
		color: inherit;
		font-family: var(--font-mono);
		font-size: inherit;
	}

	.status-content {
		display: inline-flex;
		align-items: center;
		gap: var(--space-2);
	}

	.status-dot {
		display: inline-block;
		width: 6px;
		height: 6px;
		border-radius: var(--radius-full);
	}

	.status-dot.animating {
		animation: thread-weaving 1.5s ease-in-out infinite;
	}

	@keyframes thread-weaving {
		0%,
		100% {
			opacity: 1;
			transform: scale(1);
		}
		50% {
			opacity: 0.5;
			transform: scale(0.8);
		}
	}

	.attempt-count {
		font-size: var(--text-xs);
		opacity: 0.75;
	}
</style>
