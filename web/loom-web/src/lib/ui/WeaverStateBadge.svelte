<!--
  Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
  SPDX-License-Identifier: Proprietary
-->
<script lang="ts">
	import { i18n } from '$lib/i18n';

	export type WeaverState = 'idle' | 'weaving' | 'waiting' | 'error' | 'complete';

	interface Props {
		state: WeaverState;
		weaverColor?: string;
		size?: 'sm' | 'md';
	}

	let { state, weaverColor = 'var(--color-thread)', size = 'md' }: Props = $props();

	const stateLabels: Record<WeaverState, string> = {
		idle: i18n.t('state.idle'),
		weaving: i18n.t('state.weaving'),
		waiting: i18n.t('state.waiting'),
		error: i18n.t('state.brokenThread'),
		complete: i18n.t('state.complete'),
	};
</script>

<span
	class="badge badge-{state} badge-{size}"
	style="--weaver-color: {weaverColor}"
>
	<span class="badge-dot"></span>
	{stateLabels[state]}
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

	.badge-idle .badge-dot {
		animation: thread-idle 3s ease-in-out infinite;
	}

	.badge-weaving {
		overflow: hidden;
	}

	.badge-weaving .badge-dot {
		background: linear-gradient(
			90deg,
			var(--weaver-color) 0%,
			color-mix(in srgb, var(--weaver-color) 60%, white) 50%,
			var(--weaver-color) 100%
		);
		background-size: 200% 100%;
		animation: thread-weaving 1.5s ease-in-out infinite;
	}

	.badge-waiting .badge-dot {
		animation: thread-waiting 2s ease-in-out infinite;
	}

	.badge-error {
		background: var(--color-error-soft);
		color: var(--color-error);
	}

	.badge-error .badge-dot {
		background: var(--color-error);
		animation: thread-snap 0.5s ease-out forwards;
	}

	.badge-complete {
		background: var(--color-success-soft);
		color: var(--color-success);
	}

	.badge-complete .badge-dot {
		background: var(--color-success);
	}

	@keyframes thread-idle {
		0%,
		100% {
			opacity: 0.6;
		}
		50% {
			opacity: 1;
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

	@keyframes thread-waiting {
		0%,
		100% {
			transform: translateY(0);
		}
		50% {
			transform: translateY(-2px);
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
