<!--
  Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
  SPDX-License-Identifier: Proprietary
-->
<script lang="ts">
	import { onDestroy } from 'svelte';
	import { i18n } from '$lib/i18n';
	import { Card, Badge, Button, Input } from '$lib/ui';
	import type { LogEntry, LogLevel, ListLogsResponse } from '$lib/api/types';

	let allLogs = $state<LogEntry[]>([]);
	let bufferSize = $state(0);
	let bufferCapacity = $state(0);
	let loading = $state(true);
	let error = $state<string | null>(null);
	let streaming = $state(false);
	let eventSource: EventSource | null = null;

	// Client-side filters
	let searchQuery = $state('');
	let showTrace = $state(false);
	let showDebug = $state(true);
	let showInfo = $state(true);
	let showWarn = $state(true);
	let showError = $state(true);
	let autoScroll = $state(true);

	// Filtered logs based on client-side filters
	const filteredLogs = $derived.by(() => {
		return allLogs.filter((log) => {
			// Level filter
			if (log.level === 'trace' && !showTrace) return false;
			if (log.level === 'debug' && !showDebug) return false;
			if (log.level === 'info' && !showInfo) return false;
			if (log.level === 'warn' && !showWarn) return false;
			if (log.level === 'error' && !showError) return false;

			// Search filter
			if (searchQuery) {
				const query = searchQuery.toLowerCase();
				const matchesMessage = log.message.toLowerCase().includes(query);
				const matchesTarget = log.target.toLowerCase().includes(query);
				const matchesFields = log.fields?.some(
					([k, v]) => k.toLowerCase().includes(query) || v.toLowerCase().includes(query)
				);
				if (!matchesMessage && !matchesTarget && !matchesFields) return false;
			}

			return true;
		});
	});

	async function loadLogs() {
		loading = true;
		error = null;
		try {
			const res = await fetch('/api/admin/logs?limit=500', { credentials: 'include' });
			if (!res.ok) throw new Error(`Failed to load logs: ${res.status}`);
			const data: ListLogsResponse = await res.json();
			allLogs = data.entries;
			bufferSize = data.buffer_size;
			bufferCapacity = data.buffer_capacity;
		} catch (e) {
			error = e instanceof Error ? e.message : i18n._('general.error');
		} finally {
			loading = false;
		}
	}

	function startStreaming() {
		if (eventSource) return;

		eventSource = new EventSource('/api/admin/logs/stream', { withCredentials: true });

		eventSource.onmessage = (event) => {
			try {
				const entry: LogEntry = JSON.parse(event.data);
				allLogs = [...allLogs, entry].slice(-1000);
				if (autoScroll) {
					requestAnimationFrame(() => {
						const container = document.getElementById('log-container');
						if (container) {
							container.scrollTop = container.scrollHeight;
						}
					});
				}
			} catch {
				// Ignore parse errors
			}
		};

		eventSource.onerror = () => {
			stopStreaming();
			error = i18n._('admin.logs.stream_error');
		};

		streaming = true;
	}

	function stopStreaming() {
		if (eventSource) {
			eventSource.close();
			eventSource = null;
		}
		streaming = false;
	}

	function toggleStreaming() {
		if (streaming) {
			stopStreaming();
		} else {
			startStreaming();
		}
	}

	function getLevelColor(level: LogLevel): string {
		switch (level) {
			case 'trace':
				return 'text-gray-500';
			case 'debug':
				return 'text-gray-400';
			case 'info':
				return 'text-blue-400';
			case 'warn':
				return 'text-yellow-400';
			case 'error':
				return 'text-red-400';
			default:
				return 'text-gray-400';
		}
	}

	function getLevelBgColor(level: LogLevel): string {
		switch (level) {
			case 'trace':
				return 'bg-gray-700';
			case 'debug':
				return 'bg-gray-600';
			case 'info':
				return 'bg-blue-600';
			case 'warn':
				return 'bg-yellow-600';
			case 'error':
				return 'bg-red-600';
			default:
				return 'bg-gray-600';
		}
	}

	function formatTimestamp(ts: string): string {
		const date = new Date(ts);
		return date.toLocaleTimeString('en-US', {
			hour12: false,
			hour: '2-digit',
			minute: '2-digit',
			second: '2-digit',
			fractionalSecondDigits: 3,
		});
	}

	function clearLogs() {
		allLogs = [];
	}

	function selectAllLevels() {
		showTrace = true;
		showDebug = true;
		showInfo = true;
		showWarn = true;
		showError = true;
	}

	function selectErrorsOnly() {
		showTrace = false;
		showDebug = false;
		showInfo = false;
		showWarn = true;
		showError = true;
	}

	$effect(() => {
		loadLogs().then(() => {
			startStreaming();
		});
		return () => {
			stopStreaming();
		};
	});

	onDestroy(() => {
		stopStreaming();
	});
</script>

<svelte:head>
	<title>{i18n._('admin.logs.title')} - Loom</title>
</svelte:head>

<div class="max-w-full mx-auto px-4 sm:px-6 lg:px-8 py-8">
	<div class="mb-6 flex items-center justify-between">
		<div>
			<h1 class="text-2xl font-bold text-fg">{i18n._('admin.logs.title')}</h1>
			<p class="text-fg-muted">
				{i18n._('admin.logs.buffer_status', { size: bufferSize, capacity: bufferCapacity })}
				{#if streaming}
					<span class="inline-block w-2 h-2 rounded-full bg-success animate-pulse ml-2"></span>
					<span class="text-success">{i18n._('admin.logs.streaming')}</span>
				{/if}
			</p>
		</div>
		<div class="flex items-center gap-2">
			<Button variant={streaming ? 'primary' : 'secondary'} onclick={toggleStreaming}>
				{streaming ? i18n._('admin.logs.stop_stream') : i18n._('admin.logs.start_stream')}
			</Button>
			<Button variant="secondary" onclick={() => loadLogs()} disabled={loading}>
				{i18n._('general.refresh')}
			</Button>
			<Button variant="secondary" onclick={clearLogs}>
				{i18n._('admin.logs.clear')}
			</Button>
		</div>
	</div>

	{#if error}
		<div class="mb-4 p-3 rounded-md bg-error/10 text-error text-sm">{error}</div>
	{/if}

	<!-- Filters -->
	<Card>
		<div class="flex flex-wrap items-center gap-4">
			<!-- Search -->
			<div class="flex-1 min-w-64">
				<input
					type="text"
					bind:value={searchQuery}
					placeholder={i18n._('admin.logs.search_placeholder')}
					class="w-full px-3 py-1.5 rounded-md border border-border bg-bg text-fg text-sm focus:outline-none focus:ring-2 focus:ring-accent"
				/>
			</div>

			<!-- Level toggles -->
			<div class="flex items-center gap-1">
				<button
					onclick={() => (showTrace = !showTrace)}
					class="px-2 py-1 rounded text-xs font-medium transition-colors {showTrace
						? 'bg-gray-700 text-white'
						: 'bg-gray-200 dark:bg-gray-800 text-gray-500'}"
				>
					TRACE
				</button>
				<button
					onclick={() => (showDebug = !showDebug)}
					class="px-2 py-1 rounded text-xs font-medium transition-colors {showDebug
						? 'bg-gray-600 text-white'
						: 'bg-gray-200 dark:bg-gray-800 text-gray-500'}"
				>
					DEBUG
				</button>
				<button
					onclick={() => (showInfo = !showInfo)}
					class="px-2 py-1 rounded text-xs font-medium transition-colors {showInfo
						? 'bg-blue-600 text-white'
						: 'bg-gray-200 dark:bg-gray-800 text-gray-500'}"
				>
					INFO
				</button>
				<button
					onclick={() => (showWarn = !showWarn)}
					class="px-2 py-1 rounded text-xs font-medium transition-colors {showWarn
						? 'bg-yellow-600 text-white'
						: 'bg-gray-200 dark:bg-gray-800 text-gray-500'}"
				>
					WARN
				</button>
				<button
					onclick={() => (showError = !showError)}
					class="px-2 py-1 rounded text-xs font-medium transition-colors {showError
						? 'bg-red-600 text-white'
						: 'bg-gray-200 dark:bg-gray-800 text-gray-500'}"
				>
					ERROR
				</button>
			</div>

			<!-- Quick filters -->
			<div class="flex items-center gap-2">
				<button onclick={selectAllLevels} class="text-xs text-accent hover:text-accent-hover">
					{i18n._('admin.logs.show_all')}
				</button>
				<span class="text-fg-muted">|</span>
				<button onclick={selectErrorsOnly} class="text-xs text-accent hover:text-accent-hover">
					{i18n._('admin.logs.errors_only')}
				</button>
			</div>

			<!-- Auto-scroll -->
			<label class="flex items-center gap-2 text-sm text-fg-muted cursor-pointer ml-auto">
				<input type="checkbox" bind:checked={autoScroll} class="rounded border-border" />
				{i18n._('admin.logs.auto_scroll')}
			</label>
		</div>

		<!-- Result count -->
		<div class="mt-2 text-xs text-fg-muted">
			{i18n._('admin.logs.showing_count', { shown: filteredLogs.length, total: allLogs.length })}
		</div>
	</Card>

	<!-- Log entries -->
	<div class="mt-4">
		<div
			id="log-container"
			class="bg-gray-900 rounded-lg border border-border overflow-auto font-mono text-xs"
			style="height: calc(100vh - 320px); min-height: 400px;"
		>
			{#if loading && allLogs.length === 0}
				<div class="text-gray-400 text-center py-8">{i18n._('general.loading')}</div>
			{:else if filteredLogs.length === 0}
				<div class="text-gray-400 text-center py-8">
					{#if allLogs.length === 0}
						{i18n._('admin.logs.no_logs')}
					{:else}
						{i18n._('admin.logs.no_matches')}
					{/if}
				</div>
			{:else}
				<table class="w-full">
					<tbody>
						{#each filteredLogs as log (log.id)}
							<tr class="hover:bg-gray-800/50 border-b border-gray-800/50">
								<td class="px-2 py-1 text-gray-500 whitespace-nowrap align-top w-24">
									{formatTimestamp(log.timestamp)}
								</td>
								<td class="px-2 py-1 align-top w-16">
									<span class="px-1.5 py-0.5 rounded text-xs font-medium text-white {getLevelBgColor(log.level)}">
										{log.level.toUpperCase()}
									</span>
								</td>
								<td class="px-2 py-1 text-cyan-400 whitespace-nowrap align-top max-w-xs truncate" title={log.target}>
									{log.target}
								</td>
								<td class="px-2 py-1 text-gray-200 break-all align-top">
									{log.message}
									{#if log.fields && log.fields.length > 0}
										<span class="text-gray-500 ml-2">
											{#each log.fields as [key, value]}
												<span class="text-purple-400">{key}</span>=<span class="text-yellow-300">{value}</span>{' '}
											{/each}
										</span>
									{/if}
								</td>
							</tr>
						{/each}
					</tbody>
				</table>
			{/if}
		</div>
	</div>
</div>
