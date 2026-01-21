<!--
 Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
 SPDX-License-Identifier: Proprietary
-->
<script lang="ts">
	import { i18n } from '$lib/i18n';
	import type { Snippet } from 'svelte';

	interface Props {
		variant?: 'info' | 'tip' | 'warning' | 'danger';
		title?: string;
		children: Snippet;
	}

	let { variant = 'info', title, children }: Props = $props();

	const icons: Record<string, string> = {
		info: 'ℹ',
		tip: '💡',
		warning: '⚠',
		danger: '🚨',
	};

	const defaultTitles: Record<string, string> = {
		info: i18n('docs.callout.info'),
		tip: i18n('docs.callout.tip'),
		warning: i18n('docs.callout.warning'),
		danger: i18n('docs.callout.danger'),
	};
</script>

<aside class="callout callout-{variant}">
	<div class="callout-header">
		<span class="callout-icon">{icons[variant]}</span>
		<span class="callout-title">{title ?? defaultTitles[variant]}</span>
	</div>
	<div class="callout-content">
		{@render children()}
	</div>
</aside>

<style>
	.callout {
		font-family: var(--font-mono);
		border-radius: var(--radius-md);
		padding: var(--space-4);
		margin: var(--space-4) 0;
		border-left: 3px solid;
	}

	.callout-header {
		display: flex;
		align-items: center;
		gap: var(--space-2);
		font-weight: 500;
		font-size: var(--text-sm);
		margin-bottom: var(--space-2);
	}

	.callout-icon {
		font-size: var(--text-base);
	}

	.callout-content {
		font-size: var(--text-sm);
		line-height: 1.6;
	}

	.callout-content :global(p:last-child) {
		margin-bottom: 0;
	}

	.callout-info {
		background: var(--color-info-soft);
		border-color: var(--color-info);
		color: var(--color-fg);
	}

	.callout-info .callout-title {
		color: var(--color-info);
	}

	.callout-tip {
		background: var(--color-success-soft);
		border-color: var(--color-success);
		color: var(--color-fg);
	}

	.callout-tip .callout-title {
		color: var(--color-success);
	}

	.callout-warning {
		background: var(--color-warning-soft);
		border-color: var(--color-warning);
		color: var(--color-fg);
	}

	.callout-warning .callout-title {
		color: var(--color-warning);
	}

	.callout-danger {
		background: var(--color-error-soft);
		border-color: var(--color-error);
		color: var(--color-fg);
	}

	.callout-danger .callout-title {
		color: var(--color-error);
	}
</style>
