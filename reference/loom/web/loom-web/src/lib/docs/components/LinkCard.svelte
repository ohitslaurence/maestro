<!--
 Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
 SPDX-License-Identifier: Proprietary
-->
<script lang="ts">
	interface Props {
		href: string;
		title: string;
		description?: string;
		external?: boolean;
	}

	let { href, title, description, external = false }: Props = $props();
</script>

<a
	class="link-card"
	{href}
	target={external ? '_blank' : undefined}
	rel={external ? 'noopener noreferrer' : undefined}
>
	<span class="link-card-title">
		{title}
		{#if external}
			<span class="external-icon">↗</span>
		{/if}
	</span>
	{#if description}
		<span class="link-card-description">{description}</span>
	{/if}
</a>

<style>
	.link-card {
		display: block;
		font-family: var(--font-mono);
		padding: var(--space-4);
		background: var(--color-bg-muted);
		border: 1px solid var(--color-border);
		border-radius: var(--radius-md);
		text-decoration: none;
		transition: all 0.15s ease;
		margin: var(--space-3) 0;
	}

	.link-card:hover {
		border-color: var(--color-accent);
		background: var(--color-bg-subtle);
	}

	.link-card-title {
		display: flex;
		align-items: center;
		gap: var(--space-2);
		font-size: var(--text-base);
		font-weight: 500;
		color: var(--color-accent);
	}

	.external-icon {
		font-size: var(--text-sm);
		opacity: 0.7;
	}

	.link-card-description {
		display: block;
		font-size: var(--text-sm);
		color: var(--color-fg-muted);
		margin-top: var(--space-1);
	}
</style>
