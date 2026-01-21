<!--
  Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
  SPDX-License-Identifier: Proprietary
-->
<script lang="ts">
	import type { ToolExecutionStatus } from '../api/types';
	import { i18n } from '$lib/i18n';

	export type ShuttleState = 'pending' | 'passing' | 'complete' | 'failed';

	interface Props {
		status: ToolExecutionStatus;
		weaverColor?: string;
		size?: 'sm' | 'md';
	}

	let { status, weaverColor = 'var(--color-thread)', size = 'md' }: Props = $props();

	const statusMapping: Record<string, { displayState: ShuttleState; label: string }> = {
		pending: { displayState: 'pending', label: i18n.t('tool.shuttleReady') },
		running: { displayState: 'passing', label: i18n.t('state.shuttlePass') },
		completed: { displayState: 'complete', label: i18n.t('state.complete') },
		failed: { displayState: 'failed', label: i18n.t('state.brokenThread') },
	};

	const config = $derived.by(() => {
		if (status.status === 'completed' && status.error) {
			return statusMapping.failed;
		}
		return statusMapping[status.status] || statusMapping.pending;
	});
</script>

<span
	class="badge badge-{config.displayState} badge-{size}"
	style="--weaver-color: {weaverColor}"
>
	<span class="badge-dot"></span>
	{config.label}
</span>

<style>
	.badge {
		display: inline-flex;
		align-items: center;
		gap: var(--space-2);
		font-family: var(--font-mono);
		border-radius: var(--radius-md);
		background: var(--color-bg-subtle);
		color: var(--color-fg-muted);
	}

	.badge-sm {
		padding: var(--space-1) var(--space-2);
		font-size: 10px;
	}

	.badge-md {
		padding: var(--space-1) var(--space-3);
		font-size: var(--text-xs);
	}

	.badge-dot {
		width: 6px;
		height: 6px;
		border-radius: var(--radius-full);
		background: var(--weaver-color, var(--color-thread));
	}

	.badge-pending .badge-dot {
		animation: thread-waiting 2s ease-in-out infinite;
	}

	.badge-passing {
		overflow: hidden;
	}

	.badge-passing .badge-dot {
		background: linear-gradient(
			90deg,
			var(--weaver-color) 0%,
			color-mix(in srgb, var(--weaver-color) 60%, white) 50%,
			var(--weaver-color) 100%
		);
		background-size: 200% 100%;
		animation: thread-weaving 1.5s ease-in-out infinite;
	}

	.badge-complete {
		background: var(--color-success-soft);
		color: var(--color-success);
	}

	.badge-complete .badge-dot {
		background: var(--color-success);
	}

	.badge-failed {
		background: var(--color-error-soft);
		color: var(--color-error);
	}

	.badge-failed .badge-dot {
		background: var(--color-error);
		animation: thread-snap 0.5s ease-out forwards;
	}

	@keyframes thread-waiting {
		0%,
		100% {
			transform: translateY(0);
		}
		50% {
			transform: translateY(-2px);
		}
	}

	@keyframes thread-weaving {
		0% {
			background-position: -200% 0;
		}
		100% {
			background-position: 200% 0;
		}
	}

	@keyframes thread-snap {
		0% {
			transform: scale(1);
			opacity: 1;
		}
		20% {
			transform: scale(1.2);
		}
		40% {
			transform: scale(0.8);
			opacity: 0.8;
		}
		100% {
			transform: scale(1);
			opacity: 1;
		}
	}
</style>
