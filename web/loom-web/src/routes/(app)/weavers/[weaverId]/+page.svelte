<!--
  Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
  SPDX-License-Identifier: Proprietary
-->
<script lang="ts">
	import { page } from '$app/stores';
	import { goto } from '$app/navigation';
	import { i18n } from '$lib/i18n';
	import { getApiClient } from '$lib/api/client';
	import type { Weaver, WeaverStatus } from '$lib/api/types';
	import WeaverTerminal from '$lib/components/WeaverTerminal.svelte';
	import { Card, Badge, Button } from '$lib/ui';

	const weaverId = $derived($page.params.weaverId!);
	const client = getApiClient();

	let weaver = $state<Weaver | null>(null);
	let loading = $state(true);
	let error = $state<string | null>(null);
	let deleting = $state(false);

	async function loadWeaver() {
		loading = true;
		error = null;
		try {
			weaver = await client.getWeaver(weaverId);
		} catch (e) {
			error = e instanceof Error ? e.message : i18n._('general.error');
		} finally {
			loading = false;
		}
	}

	async function deleteWeaver() {
		if (!confirm(i18n._('weavers.deleteConfirm'))) return;
		deleting = true;
		try {
			await client.deleteWeaver(weaverId);
			goto('/weavers');
		} catch (e) {
			error = e instanceof Error ? e.message : i18n._('general.error');
			deleting = false;
		}
	}

	function getStatusVariant(status: WeaverStatus): 'success' | 'warning' | 'error' | 'muted' {
		switch (status) {
			case 'running':
				return 'success';
			case 'pending':
				return 'warning';
			case 'failed':
				return 'error';
			default:
				return 'muted';
		}
	}

	function formatDate(dateStr: string): string {
		return new Date(dateStr).toLocaleString();
	}

	function formatAge(hours: number | undefined): string {
		if (hours === undefined) return '-';
		if (hours < 1) return `${Math.round(hours * 60)}m`;
		return `${hours.toFixed(1)}h`;
	}

	$effect(() => {
		loadWeaver();
	});
</script>

<svelte:head>
	<title>{weaver?.id ?? i18n._('weavers.title')} - Loom</title>
</svelte:head>

<div class="h-[calc(100vh-4rem)] flex flex-col">
	<div class="px-4 sm:px-6 lg:px-8 py-4 border-b border-gray-200 dark:border-gray-700 bg-white dark:bg-gray-800">
		<div class="flex items-center justify-between max-w-7xl mx-auto">
			<div class="flex items-center gap-4">
				<a
					href="/weavers"
					class="text-sm text-gray-600 dark:text-gray-400 hover:text-gray-900 dark:hover:text-white"
				>
					← {i18n._('weavers.backToList')}
				</a>
				{#if weaver}
					<div class="flex items-center gap-2">
						<span class="font-mono text-sm text-fg">{weaver.id}</span>
						<Badge variant={getStatusVariant(weaver.status)} size="sm">
							{weaver.status}
						</Badge>
					</div>
				{/if}
			</div>
			<div class="flex items-center gap-4">
				{#if weaver}
					<div class="text-sm text-fg-muted">
						{#if weaver.image}
							<span class="mr-4">{weaver.image}</span>
						{/if}
						<span>{i18n._('weavers.age')}: {formatAge(weaver.age_hours)}</span>
					</div>
				{/if}
				<Button
					variant="danger"
					size="sm"
					disabled={deleting}
					loading={deleting}
					onclick={deleteWeaver}
				>
					{i18n._('weavers.delete')}
				</Button>
			</div>
		</div>
	</div>

	{#if error}
		<div class="px-4 py-3 bg-error/10 text-error text-sm">{error}</div>
	{/if}

	{#if loading}
		<div class="flex-1 flex items-center justify-center bg-gray-900">
			<div class="text-gray-400">{i18n._('general.loading')}</div>
		</div>
	{:else if weaver}
		{#if weaver.status === 'running'}
			<div class="flex-1 min-h-0">
				<WeaverTerminal {weaverId} />
			</div>
		{:else if weaver.status === 'pending'}
			<div class="flex-1 flex items-center justify-center bg-gray-900">
				<div class="text-center">
					<div class="text-yellow-400 mb-2">{i18n._('weavers.terminal.pending')}</div>
					<div class="text-gray-400 text-sm">{i18n._('weavers.terminal.pendingHint')}</div>
					<Button variant="secondary" onclick={loadWeaver} class="mt-4">
						{i18n._('general.refresh')}
					</Button>
				</div>
			</div>
		{:else}
			<div class="flex-1 flex items-center justify-center bg-gray-900">
				<div class="text-center">
					<div class="text-red-400 mb-2">
						{i18n._('weavers.terminal.notRunning', { status: weaver.status })}
					</div>
					<a href="/weavers" class="text-sm text-gray-400 hover:text-white">
						{i18n._('weavers.backToList')}
					</a>
				</div>
			</div>
		{/if}
	{:else}
		<div class="flex-1 flex items-center justify-center bg-gray-900">
			<div class="text-gray-400">{i18n._('weavers.notFound')}</div>
		</div>
	{/if}
</div>
