<!--
  Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
  SPDX-License-Identifier: Proprietary
-->
<script lang="ts">
	import { i18n } from '$lib/i18n';
	import { Badge, Button } from '$lib/ui';

	interface Props {
		jobId: string;
	}

	let { jobId }: Props = $props();

	interface JobRun {
		id: string;
		status: string;
		started_at: string;
		completed_at: string | null;
		duration_ms: number | null;
		error_message: string | null;
		retry_count: number;
		triggered_by: string;
		metadata: unknown;
	}

	let runs = $state<JobRun[]>([]);
	let loading = $state(true);
	let hasMore = $state(true);
	let offset = $state(0);
	const limit = 20;

	async function loadHistory(reset: boolean = false) {
		if (reset) {
			runs = [];
			offset = 0;
			hasMore = true;
		}
		loading = true;
		try {
			const res = await fetch(`/api/admin/jobs/${jobId}/history?limit=${limit}&offset=${offset}`, {
				credentials: 'include',
			});
			if (!res.ok) throw new Error('Failed to load history');
			const data = await res.json();
			runs = [...runs, ...data.runs];
			hasMore = data.runs.length >= limit;
		} finally {
			loading = false;
		}
	}

	function loadMore() {
		offset += limit;
		loadHistory();
	}

	function getStatusVariant(status: string): 'success' | 'error' | 'accent' | 'muted' {
		switch (status) {
			case 'succeeded':
				return 'success';
			case 'failed':
				return 'error';
			case 'running':
				return 'accent';
			default:
				return 'muted';
		}
	}

	function formatDuration(ms: number | null): string {
		if (ms === null) return '-';
		return `${(ms / 1000).toFixed(2)}s`;
	}

	$effect(() => {
		loadHistory(true);
	});
</script>

<div class="mt-4 border-t border-border pt-4">
	<h3 class="font-medium text-fg mb-3">{i18n._('jobs.history.title')}</h3>

	{#if loading && runs.length === 0}
		<div class="text-fg-muted">{i18n._('general.loading')}</div>
	{:else if runs.length === 0}
		<div class="text-fg-muted">{i18n._('jobs.history.noRuns')}</div>
	{:else}
		<div class="overflow-x-auto">
			<table class="w-full text-sm">
				<thead>
					<tr class="text-left border-b border-border text-fg-muted">
						<th class="p-2">{i18n._('jobs.history.status')}</th>
						<th class="p-2">{i18n._('jobs.history.started')}</th>
						<th class="p-2">{i18n._('jobs.history.duration')}</th>
						<th class="p-2">{i18n._('jobs.history.trigger')}</th>
						<th class="p-2">{i18n._('jobs.history.details')}</th>
					</tr>
				</thead>
				<tbody>
					{#each runs as run (run.id)}
						<tr class="border-b border-border hover:bg-bg-muted">
							<td class="p-2">
								<Badge variant={getStatusVariant(run.status)} size="sm">
									{i18n._(`jobs.status.${run.status}`)}
								</Badge>
							</td>
							<td class="p-2 text-fg">{new Date(run.started_at).toLocaleString()}</td>
							<td class="p-2 text-fg">{formatDuration(run.duration_ms)}</td>
							<td class="p-2 text-fg">{i18n._(`jobs.trigger.${run.triggered_by}`)}</td>
							<td class="p-2">
								{#if run.error_message}
									<span class="text-error">{run.error_message}</span>
								{:else if run.metadata}
									<span class="text-fg-muted">{JSON.stringify(run.metadata)}</span>
								{:else}
									<span class="text-fg-muted">-</span>
								{/if}
							</td>
						</tr>
					{/each}
				</tbody>
			</table>
		</div>

		{#if hasMore}
			<div class="mt-3">
				<Button variant="secondary" size="sm" disabled={loading} loading={loading} onclick={loadMore}>
					{i18n._('jobs.history.loadMore')}
				</Button>
			</div>
		{/if}
	{/if}
</div>
