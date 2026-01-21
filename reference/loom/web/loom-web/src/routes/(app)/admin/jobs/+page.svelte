<!--
  Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
  SPDX-License-Identifier: Proprietary
-->
<script lang="ts">
	import { i18n } from '$lib/i18n';
	import { getApiClient } from '$lib/api/client';
	import { Card, Badge, Button } from '$lib/ui';
	import JobHistory from './JobHistory.svelte';

	const client = getApiClient();

	interface JobInfo {
		id: string;
		name: string;
		description: string;
		job_type: string;
		interval_secs: number | null;
		enabled: boolean;
		status: 'healthy' | 'degraded' | 'unhealthy';
		last_run: {
			run_id: string;
			status: string;
			started_at: string;
			duration_ms: number | null;
			error: string | null;
		} | null;
		consecutive_failures: number;
	}

	let jobs = $state<JobInfo[]>([]);
	let loading = $state(true);
	let error = $state<string | null>(null);
	let selectedJob = $state<string | null>(null);
	let triggerLoading = $state<string | null>(null);

	async function loadJobs() {
		try {
			const res = await fetch('/api/admin/jobs', { credentials: 'include' });
			if (!res.ok) throw new Error(i18n._('jobs.loadError'));
			const data = await res.json();
			jobs = data.jobs;
		} catch (e) {
			error = e instanceof Error ? e.message : i18n._('general.unknownError');
		} finally {
			loading = false;
		}
	}

	async function triggerJob(jobId: string) {
		triggerLoading = jobId;
		try {
			const res = await fetch(`/api/admin/jobs/${jobId}/run`, {
				method: 'POST',
				credentials: 'include',
			});
			if (!res.ok) throw new Error(i18n._('jobs.triggerError'));
			await loadJobs();
		} catch (e) {
			error = e instanceof Error ? e.message : i18n._('general.unknownError');
		} finally {
			triggerLoading = null;
		}
	}

	function formatDuration(ms: number | null): string {
		if (ms === null) return '-';
		if (ms < 1000) return `${ms}ms`;
		return `${(ms / 1000).toFixed(1)}s`;
	}

	function formatTimeAgo(dateStr: string): string {
		const date = new Date(dateStr);
		const now = new Date();
		const diffMs = now.getTime() - date.getTime();
		const diffMins = Math.floor(diffMs / 60000);
		if (diffMins < 1) return i18n._('jobs.time.justNow');
		if (diffMins < 60) return i18n._('jobs.time.minutesAgo', { count: diffMins });
		const diffHours = Math.floor(diffMins / 60);
		if (diffHours < 24) return i18n._('jobs.time.hoursAgo', { count: diffHours });
		return i18n._('jobs.time.daysAgo', { count: Math.floor(diffHours / 24) });
	}

	function getStatusVariant(status: string): 'success' | 'warning' | 'error' | 'muted' {
		switch (status) {
			case 'healthy':
				return 'success';
			case 'degraded':
				return 'warning';
			case 'unhealthy':
				return 'error';
			default:
				return 'muted';
		}
	}

	$effect(() => {
		loadJobs();
		const interval = setInterval(loadJobs, 30000);
		return () => clearInterval(interval);
	});
</script>

<svelte:head>
	<title>{i18n._('jobs.title')} - Loom</title>
</svelte:head>

<div class="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8 py-8">
	<div class="mb-6">
		<h1 class="text-2xl font-bold text-fg">{i18n._('jobs.title')}</h1>
		<p class="text-fg-muted">{i18n._('jobs.description')}</p>
	</div>

	{#if error}
		<div class="mb-4 p-3 rounded-md bg-error/10 text-error text-sm">{error}</div>
	{/if}

	{#if loading && jobs.length === 0}
		<Card>
			<div class="text-fg-muted text-center py-8">{i18n._('general.loading')}</div>
		</Card>
	{:else}
		<div class="space-y-4">
			{#each jobs as job (job.id)}
				<Card>
					<div class="flex items-start justify-between gap-4">
						<div class="flex items-start gap-3 min-w-0">
							<Badge variant={getStatusVariant(job.status)} size="sm">
								{job.status}
							</Badge>
							<div class="min-w-0">
								<h2 class="font-semibold text-fg">{job.name}</h2>
								<p class="text-sm text-fg-muted">{job.description}</p>
							</div>
						</div>
						<div class="flex items-center gap-2 flex-shrink-0">
							<Button
								variant="primary"
								size="sm"
								disabled={triggerLoading === job.id}
								loading={triggerLoading === job.id}
								onclick={() => triggerJob(job.id)}
							>
								{i18n._('jobs.runNow')}
							</Button>
							<Button
								variant="secondary"
								size="sm"
								onclick={() => (selectedJob = selectedJob === job.id ? null : job.id)}
							>
								{selectedJob === job.id ? i18n._('jobs.hideHistory') : i18n._('jobs.history')}
							</Button>
						</div>
					</div>

					<div class="mt-3 flex flex-wrap gap-x-4 gap-y-1 text-sm text-fg-muted">
						<span>{i18n._('jobs.type')}: <span class="text-fg">{job.job_type}</span></span>
						{#if job.interval_secs}
							<span>{i18n._('jobs.interval')}: <span class="text-fg">{job.interval_secs}s</span></span>
						{/if}
						{#if job.last_run}
							<span>{i18n._('jobs.lastRun')}: <span class="text-fg">{formatTimeAgo(job.last_run.started_at)}</span></span>
							<span>{i18n._('jobs.duration')}: <span class="text-fg">{formatDuration(job.last_run.duration_ms)}</span></span>
							<span>{i18n._('jobs.history.status')}: <span class="text-fg">{i18n._(`jobs.status.${job.last_run.status}`)}</span></span>
						{:else}
							<span>{i18n._('jobs.never')}</span>
						{/if}
						{#if job.consecutive_failures > 0}
							<span class="text-error">({i18n._('jobs.failures', { count: job.consecutive_failures })})</span>
						{/if}
					</div>

					{#if job.last_run?.error}
						<div class="mt-3 p-2 rounded-md bg-error/10 text-error text-sm">
							{job.last_run.error}
						</div>
					{/if}

					{#if selectedJob === job.id}
						<JobHistory jobId={job.id} />
					{/if}
				</Card>
			{/each}
		</div>

		{#if jobs.length === 0}
			<Card>
				<p class="text-fg-muted text-center py-4">{i18n._('jobs.noJobs')}</p>
			</Card>
		{/if}
	{/if}
</div>
