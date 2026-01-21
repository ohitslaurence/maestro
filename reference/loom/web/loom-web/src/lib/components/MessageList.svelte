<!--
  Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
  SPDX-License-Identifier: Proprietary
-->

<script lang="ts">
	import type { MessageSnapshot } from '../api/types';
	import { ThreadDivider } from '../ui';
	import MessageBubble from './MessageBubble.svelte';
	import { i18n } from '$lib/i18n';

	interface Props {
		messages: MessageSnapshot[];
		streamingContent?: string;
		isStreaming?: boolean;
		weaverColor?: string;
	}

	let {
		messages,
		streamingContent = '',
		isStreaming = false,
		weaverColor = 'var(--weaver-indigo)'
	}: Props = $props();

	let containerRef: HTMLDivElement;

	function shouldShowDivider(currentMessage: MessageSnapshot, index: number): boolean {
		if (index === 0) return false;
		const prevMessage = messages[index - 1];
		return currentMessage.role !== prevMessage.role && currentMessage.role === 'user';
	}

	$effect(() => {
		if (containerRef) {
			containerRef.scrollTop = containerRef.scrollHeight;
		}
	});
</script>

<div bind:this={containerRef} class="message-list">
	{#if messages.length === 0 && !isStreaming}
		<div class="empty-state">{i18n.t('message.placeholder')}</div>
	{:else}
		{#each messages as message, index (message.id || message.created_at)}
			{#if shouldShowDivider(message, index)}
				<ThreadDivider variant="gradient" />
			{/if}
			<MessageBubble {message} {weaverColor} />
		{/each}

		{#if isStreaming && streamingContent}
			<MessageBubble
				message={{
					id: 'streaming',
					role: 'assistant',
					content: '',
					created_at: new Date().toISOString()
				}}
				isStreaming={true}
				{streamingContent}
				{weaverColor}
			/>
		{/if}
	{/if}
</div>

<style>
	.message-list {
		flex: 1;
		overflow-y: auto;
		padding: var(--space-4);
		display: flex;
		flex-direction: column;
		gap: var(--space-4);
		font-family: var(--font-mono);
	}

	.empty-state {
		display: flex;
		align-items: center;
		justify-content: center;
		height: 100%;
		color: var(--color-fg-muted);
	}
</style>
