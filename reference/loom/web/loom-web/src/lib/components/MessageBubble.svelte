<!--
  Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
  SPDX-License-Identifier: Proprietary
-->

<script lang="ts">
	import type { MessageSnapshot } from '../api/types';
	import { i18n } from '$lib/i18n';

	interface Props {
		message: MessageSnapshot;
		isStreaming?: boolean;
		streamingContent?: string;
		weaverColor?: string;
	}

	let {
		message,
		isStreaming = false,
		streamingContent = '',
		weaverColor = 'var(--weaver-indigo)'
	}: Props = $props();

	const content = $derived(isStreaming ? streamingContent : message.content);
</script>

<div class="message-container" class:user={message.role === 'user'}>
	<div
		class="message-bubble message-{message.role}"
		style:--weaver-color={weaverColor}
	>
		{#if message.role === 'tool'}
			<div class="tool-label">
				{i18n._('message.toolResult')}{#if message.tool_call_id}
					<span class="tool-id">({message.tool_call_id})</span>
				{/if}
			</div>
		{/if}

		<div class="message-content">
			{content}
			{#if isStreaming}
				<span class="cursor"></span>
			{/if}
		</div>

		{#if message.created_at}
			<div class="timestamp" class:text-right={message.role === 'user'}>
				{new Date(message.created_at).toLocaleTimeString()}
			</div>
		{/if}
	</div>
</div>

<style>
	.message-container {
		display: flex;
		justify-content: flex-start;
	}

	.message-container.user {
		justify-content: flex-end;
	}

	.message-bubble {
		font-family: var(--font-mono);
		border-radius: var(--radius-md);
		padding: var(--space-3);
		max-width: 80%;
	}

	.message-user {
		background: var(--color-bg-subtle);
		color: var(--color-fg);
		margin-left: auto;
	}

	.message-assistant {
		background: var(--color-bg-muted);
		color: var(--color-fg);
		margin-right: auto;
		border-left: 2px solid var(--weaver-color);
	}

	.message-tool {
		background: var(--color-warning-soft);
		border: 1px solid color-mix(in srgb, var(--color-warning) 20%, transparent);
		color: var(--color-fg);
		margin-right: auto;
		max-width: 90%;
	}

	.message-system {
		background: var(--color-bg-subtle);
		color: var(--color-fg-muted);
		text-align: center;
		margin: 0 auto;
		max-width: 90%;
	}

	.tool-label {
		font-size: var(--text-xs);
		font-weight: 500;
		color: var(--color-warning);
		margin-bottom: var(--space-1);
	}

	.tool-id {
		color: var(--color-fg-muted);
	}

	.message-content {
		white-space: pre-wrap;
		word-break: break-word;
	}

	.cursor {
		display: inline-block;
		width: 0.5rem;
		height: 1rem;
		background: var(--color-accent);
		margin-left: 2px;
		animation: thread-idle 1s ease-in-out infinite;
	}

	@keyframes thread-idle {
		0%,
		100% {
			opacity: 1;
		}
		50% {
			opacity: 0.3;
		}
	}

	.timestamp {
		font-size: var(--text-xs);
		color: var(--color-fg-subtle);
		margin-top: var(--space-1);
	}

	.text-right {
		text-align: right;
	}
</style>
