<!--
  Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
  SPDX-License-Identifier: Proprietary
-->

<script lang="ts">
	import type { Snippet } from 'svelte';

	interface Props {
		variant?: 'corners' | 'full';
		class?: string;
		children: Snippet;
	}

	let { variant = 'corners', class: className = '', children }: Props = $props();
</script>

{#if variant === 'corners'}
	<div class="loom-frame {className}">
		{@render children()}
	</div>
{:else}
	<div class="loom-frame-full {className}">
		<span class="corner corner-tl"></span>
		<span class="corner corner-tr"></span>
		<span class="corner corner-bl"></span>
		<span class="corner corner-br"></span>
		{@render children()}
	</div>
{/if}

<style>
	.loom-frame {
		position: relative;
		padding: var(--space-6);
	}

	.loom-frame::before,
	.loom-frame::after {
		content: '';
		position: absolute;
		width: 16px;
		height: 16px;
		border: 2px solid var(--color-thread-muted);
	}

	.loom-frame::before {
		top: 0;
		left: 0;
		border-right: none;
		border-bottom: none;
	}

	.loom-frame::after {
		bottom: 0;
		right: 0;
		border-left: none;
		border-top: none;
	}

	.loom-frame-full {
		position: relative;
		padding: var(--space-6);
	}

	.loom-frame-full .corner {
		position: absolute;
		width: 12px;
		height: 12px;
		border: 2px solid var(--color-thread-muted);
	}

	.loom-frame-full .corner-tl {
		top: 0;
		left: 0;
		border-right: none;
		border-bottom: none;
	}

	.loom-frame-full .corner-tr {
		top: 0;
		right: 0;
		border-left: none;
		border-bottom: none;
	}

	.loom-frame-full .corner-bl {
		bottom: 0;
		left: 0;
		border-right: none;
		border-top: none;
	}

	.loom-frame-full .corner-br {
		bottom: 0;
		right: 0;
		border-left: none;
		border-top: none;
	}
</style>
