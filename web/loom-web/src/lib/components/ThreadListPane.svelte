<!--
  Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
  SPDX-License-Identifier: Proprietary
-->

<script lang="ts">
	import { onMount } from 'svelte';
	import { createActor, type SnapshotFrom } from 'xstate';
	import { threadListMachine } from '../state';
	import { getApiClient } from '../api';
	import { Input, Skeleton, Button, ThreadDivider } from '../ui';
	import ThreadListItem from './ThreadListItem.svelte';
	import { logger } from '../logging';
	import { i18n } from '$lib/i18n';

	interface Props {
		selectedThreadId?: string | null;
		onSelectThread?: (threadId: string) => void;
	}

	let { selectedThreadId = null, onSelectThread }: Props = $props();

	const actor = createActor(threadListMachine);
	let snapshot: SnapshotFrom<typeof threadListMachine> = $state(actor.getSnapshot());
	let searchInput = $state('');
	let searchTimeout: ReturnType<typeof setTimeout> | null = null;

	actor.subscribe((s) => {
		snapshot = s;
	});

	async function fetchThreads() {
		const api = getApiClient();
		const ctx = snapshot.context;

		try {
			let response;
			if (ctx.searchQuery) {
				response = await api.searchThreads(ctx.searchQuery, {
					limit: ctx.limit,
					offset: ctx.offset
				});
				actor.send({
					type: 'FETCH_SUCCESS',
					threads: response.hits.map((hit) => ({
						id: hit.id,
						title: hit.title,
						created_at: hit.created_at,
						updated_at: hit.updated_at,
						message_count: 0
					})),
					total: response.hits.length
				});
			} else {
				response = await api.listThreads({
					limit: ctx.limit,
					offset: ctx.offset
				});
				actor.send({
					type: 'FETCH_SUCCESS',
					threads: response.threads,
					total: response.total
				});
			}
		} catch (error) {
			logger.error('Failed to fetch threads', { error: String(error) });
			actor.send({ type: 'FETCH_ERROR', error: String(error) });
		}
	}

	function handleSearch() {
		if (searchTimeout) clearTimeout(searchTimeout);
		searchTimeout = setTimeout(() => {
			if (searchInput.trim()) {
				actor.send({ type: 'SEARCH', query: searchInput.trim() });
			} else {
				actor.send({ type: 'CLEAR_SEARCH' });
			}
		}, 300);
	}

	function handleSelectThread(threadId: string) {
		onSelectThread?.(threadId);
	}

	onMount(() => {
		actor.start();
		actor.send({ type: 'FETCH' });

		actor.subscribe((s) => {
			if (s.value === 'loading') {
				fetchThreads();
			}
		});

		return () => actor.stop();
	});

	$effect(() => {
		handleSearch();
	});
</script>

<div class="thread-list-pane">
	<div class="search-section">
		<Input type="search" placeholder={i18n.t('thread.search')} bind:value={searchInput} />
	</div>

	<ThreadDivider variant="simple" class="search-divider" />

	<div class="thread-list">
		{#if snapshot.value === 'loading' && snapshot.context.threads.length === 0}
			<div class="skeleton-list">
				{#each Array(5) as _}
					<div class="skeleton-item">
						<Skeleton width="70%" height="1rem" />
						<Skeleton width="50%" height="0.75rem" rounded="sm" />
					</div>
				{/each}
			</div>
		{:else if snapshot.value === 'error'}
			<div class="error-state">
				<p class="error-message">{snapshot.context.error}</p>
				<Button variant="secondary" onclick={() => actor.send({ type: 'FETCH' })}>{i18n.t('general.retry')}</Button>
			</div>
		{:else if snapshot.context.threads.length === 0}
			<div class="empty-state">
				{snapshot.context.searchQuery ? i18n.t('thread.noThreadsFound') : i18n.t('thread.noThreads')}
			</div>
		{:else}
			<div class="thread-items">
				{#each snapshot.context.threads as thread (thread.id)}
					<ThreadListItem
						{thread}
						isActive={thread.id === selectedThreadId}
						onclick={() => handleSelectThread(thread.id)}
					/>
				{/each}
			</div>
		{/if}
	</div>

	{#if snapshot.context.total > snapshot.context.limit}
		<div class="pagination">
			<Button
				variant="ghost"
				size="sm"
				disabled={snapshot.context.offset === 0}
				onclick={() => actor.send({ type: 'PREV_PAGE' })}
			>
				{i18n.t('general.previous')}
			</Button>
			<span class="page-info">
				{snapshot.context.offset + 1}-{Math.min(
					snapshot.context.offset + snapshot.context.limit,
					snapshot.context.total
				)} of {snapshot.context.total}
			</span>
			<Button
				variant="ghost"
				size="sm"
				disabled={snapshot.context.offset + snapshot.context.limit >= snapshot.context.total}
				onclick={() => actor.send({ type: 'NEXT_PAGE' })}
			>
				{i18n.t('general.next')}
			</Button>
		</div>
	{/if}
</div>

<style>
	.thread-list-pane {
		display: flex;
		flex-direction: column;
		height: 100%;
		background: var(--color-bg-muted);
		font-family: var(--font-mono);
	}

	.search-section {
		padding: var(--space-3);
	}

	.thread-list-pane :global(.search-divider) {
		margin: 0;
	}

	.thread-list {
		flex: 1;
		overflow-y: auto;
		padding: var(--space-2);
	}

	.skeleton-list {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
	}

	.skeleton-item {
		padding: var(--space-3);
	}

	.error-state {
		padding: var(--space-4);
		text-align: center;
	}

	.error-message {
		color: var(--color-error);
		margin-bottom: var(--space-2);
	}

	.empty-state {
		padding: var(--space-4);
		text-align: center;
		color: var(--color-fg-muted);
	}

	.thread-items {
		display: flex;
		flex-direction: column;
		gap: var(--space-1);
	}

	.pagination {
		padding: var(--space-3);
		border-top: 1px solid var(--color-border);
		display: flex;
		justify-content: space-between;
		align-items: center;
	}

	.page-info {
		font-size: var(--text-sm);
		color: var(--color-fg-muted);
	}
</style>
