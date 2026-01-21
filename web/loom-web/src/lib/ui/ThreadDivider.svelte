<!--
  Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
  SPDX-License-Identifier: Proprietary
-->

<script lang="ts">
	interface Props {
		variant?: 'simple' | 'gradient' | 'knot';
		class?: string;
	}

	let { variant = 'gradient', class: className = '' }: Props = $props();
</script>

<div
	class="thread-divider thread-divider-{variant} {className}"
	role="separator"
></div>

<style>
	.thread-divider {
		height: 1px;
		margin: var(--space-6) 0;
	}

	.thread-divider-simple {
		background: var(--color-thread-muted);
	}

	.thread-divider-gradient {
		background: linear-gradient(
			90deg,
			transparent 0%,
			var(--color-thread-muted) 10%,
			var(--color-thread-muted) 90%,
			transparent 100%
		);
	}

	.thread-divider-knot {
		position: relative;
		background: var(--color-thread-muted);
	}

	.thread-divider-knot::after {
		content: '';
		position: absolute;
		left: 50%;
		top: 50%;
		transform: translate(-50%, -50%);
		width: 6px;
		height: 6px;
		background: var(--color-thread);
		border-radius: var(--radius-full);
	}
</style>
