<!--
  Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
  SPDX-License-Identifier: Proprietary
-->

<script lang="ts">
	import type { ThreadSummary } from '../api/types';
	import { i18n } from '$lib/i18n';

	interface Props {
		thread: ThreadSummary;
		isActive?: boolean;
		onclick?: () => void;
	}

	let { thread, isActive = false, onclick }: Props = $props();

	function formatRelativeTime(dateStr: string): string {
		const date = new Date(dateStr);
		const now = new Date();
		const diffMs = now.getTime() - date.getTime();
		const diffMins = Math.floor(diffMs / 60000);
		const diffHours = Math.floor(diffMs / 3600000);
		const diffDays = Math.floor(diffMs / 86400000);

		if (diffMins < 1) return i18n.t('thread.time.justNow');
		if (diffMins < 60) return i18n.t('thread.time.minutesAgo', { count: diffMins });
		if (diffHours < 24) return i18n.t('thread.time.hoursAgo', { count: diffHours });
		if (diffDays < 7) return i18n.t('thread.time.daysAgo', { count: diffDays });
		return date.toLocaleDateString();
	}
</script>

<button type="button" class="thread-item" class:active={isActive} {onclick}>
	<div class="thread-indicator"></div>
	<div class="thread-content">
		<div class="thread-header">
			<div class="thread-info">
				<h3 class="thread-title">
					{thread.title || `Thread ${thread.id.slice(0, 12)}...`}
				</h3>
				{#if thread.last_message_preview}
					<p class="thread-preview">
						{thread.last_message_preview}
					</p>
				{/if}
			</div>
			<span class="thread-time">
				{formatRelativeTime(thread.updated_at)}
			</span>
		</div>

		<div class="thread-meta">
			<span class="message-count">{i18n.t('thread.messageCount', { count: thread.message_count })}</span>
		</div>
	</div>
</button>

<style>
	.thread-item {
		display: flex;
		width: 100%;
		text-align: left;
		padding: var(--space-3);
		border-radius: var(--radius-md);
		border: 1px solid transparent;
		background: transparent;
		cursor: pointer;
		font-family: var(--font-mono);
		transition: all 0.15s ease;
	}

	.thread-item:hover {
		background: var(--color-bg-subtle);
	}

	.thread-item.active {
		background: var(--color-accent-soft);
		border-color: var(--color-accent);
	}

	.thread-indicator {
		width: 2px;
		flex-shrink: 0;
		margin-right: var(--space-3);
		border-radius: var(--radius-full);
		background: transparent;
		transition: background 0.15s ease;
	}

	.thread-item.active .thread-indicator {
		background: var(--color-thread);
	}

	.thread-item:hover .thread-indicator {
		background: var(--color-thread-muted);
	}

	.thread-item.active:hover .thread-indicator {
		background: var(--color-thread);
	}

	.thread-content {
		flex: 1;
		min-width: 0;
	}

	.thread-header {
		display: flex;
		align-items: flex-start;
		justify-content: space-between;
		gap: var(--space-2);
	}

	.thread-info {
		flex: 1;
		min-width: 0;
	}

	.thread-title {
		font-weight: 500;
		font-size: var(--text-base);
		color: var(--color-fg);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		margin: 0;
	}

	.thread-preview {
		font-size: var(--text-sm);
		color: var(--color-fg-muted);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		margin-top: 2px;
	}

	.thread-time {
		font-size: var(--text-xs);
		color: var(--color-fg-subtle);
		white-space: nowrap;
	}

	.thread-meta {
		display: flex;
		align-items: center;
		gap: var(--space-2);
		margin-top: var(--space-2);
	}

	.message-count {
		font-size: var(--text-xs);
		color: var(--color-fg-subtle);
	}
</style>
