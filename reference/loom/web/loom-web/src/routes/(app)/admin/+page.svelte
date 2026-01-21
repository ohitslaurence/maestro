<!--
  Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
  SPDX-License-Identifier: Proprietary
-->
<script lang="ts">
	import { onDestroy } from 'svelte';
	import { i18n } from '$lib/i18n';
	import { Card, Badge, ThreadDivider, LoomFrame } from '$lib/ui';
	import type { HealthResponse, HealthStatus, LogEntry, LogLevel, ListLogsResponse } from '$lib/api/types';

	let health = $state<HealthResponse | null>(null);
	let loading = $state(true);
	let error = $state<string | null>(null);

	let logs = $state<LogEntry[]>([]);
	let logsLoading = $state(true);
	let streaming = $state(false);
	let eventSource: EventSource | null = null;

	interface AdminSection {
		href: string;
		titleKey: string;
		descriptionKey: string;
		icon: string;
	}

	const adminSections: AdminSection[] = [
		{
			href: '/admin/users',
			titleKey: 'admin.users.title',
			descriptionKey: 'admin.users.description',
			icon: '👥',
		},
		{
			href: '/admin/anthropic-accounts',
			titleKey: 'admin.anthropic.title',
			descriptionKey: 'admin.dashboard.anthropic_description',
			icon: '🤖',
		},
		{
			href: '/admin/jobs',
			titleKey: 'jobs.title',
			descriptionKey: 'jobs.description',
			icon: '⚙️',
		},
		{
			href: '/admin/logs',
			titleKey: 'admin.logs.title',
			descriptionKey: 'admin.logs.description',
			icon: '📋',
		},
		{
			href: '/admin/audit-logs',
			titleKey: 'admin.audit.title',
			descriptionKey: 'admin.audit.description',
			icon: '🔒',
		},
	];

	async function loadHealth() {
		loading = true;
		error = null;
		try {
			const res = await fetch('/health', { credentials: 'include' });
			const data = await res.json();
			health = data;
		} catch (e) {
			error = e instanceof Error ? e.message : i18n._('general.error');
		} finally {
			loading = false;
		}
	}

	async function loadLogs() {
		logsLoading = true;
		try {
			const res = await fetch('/api/admin/logs?limit=50', { credentials: 'include' });
			if (!res.ok) throw new Error(`Failed to load logs: ${res.status}`);
			const data: ListLogsResponse = await res.json();
			logs = data.entries;
			startStreaming();
		} catch {
			// Ignore log load errors on dashboard
		} finally {
			logsLoading = false;
		}
	}

	function startStreaming() {
		if (eventSource) return;

		eventSource = new EventSource('/api/admin/logs/stream', { withCredentials: true });

		eventSource.onmessage = (event) => {
			try {
				const entry: LogEntry = JSON.parse(event.data);
				logs = [...logs, entry].slice(-100);
				requestAnimationFrame(() => {
					const container = document.getElementById('dashboard-log-container');
					if (container) {
						container.scrollTop = container.scrollHeight;
					}
				});
			} catch {
				// Ignore parse errors
			}
		};

		eventSource.onerror = () => {
			stopStreaming();
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

	function getStatusVariant(status: HealthStatus): 'success' | 'warning' | 'error' | 'muted' {
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

	function getStatusIcon(status: HealthStatus): string {
		switch (status) {
			case 'healthy':
				return '✓';
			case 'degraded':
				return '!';
			case 'unhealthy':
				return '✕';
			default:
				return '?';
		}
	}

	function formatLatency(ms: number): string {
		if (ms < 1000) return `${ms}ms`;
		return `${(ms / 1000).toFixed(2)}s`;
	}

	function formatTimestamp(ts: string): string {
		const date = new Date(ts);
		return date.toLocaleTimeString('en-US', {
			hour12: false,
			hour: '2-digit',
			minute: '2-digit',
			second: '2-digit',
		});
	}

	interface ComponentInfo {
		name: string;
		status: HealthStatus;
		latency?: number;
		configured?: boolean;
		error?: string;
		extra?: string;
	}

	function getComponents(): ComponentInfo[] {
		if (!health) return [];

		const components: ComponentInfo[] = [];
		const c = health.components;

		components.push({
			name: i18n._('admin.health.database'),
			status: c.database.status,
			latency: c.database.latency_ms,
			error: c.database.error,
		});

		components.push({
			name: i18n._('admin.health.bin_dir'),
			status: c.bin_dir.status,
			latency: c.bin_dir.latency_ms,
			error: c.bin_dir.error,
			extra: c.bin_dir.file_count !== undefined ? i18n._('admin.health.fileCount', { count: c.bin_dir.file_count }) : undefined,
		});

		components.push({
			name: i18n._('admin.health.llm_providers'),
			status: c.llm_providers.status,
			extra:
				c.llm_providers.providers.length > 0
					? c.llm_providers.providers.map((p) => p.name).join(', ')
					: undefined,
		});

		components.push({
			name: i18n._('admin.health.google_cse'),
			status: c.google_cse.status,
			latency: c.google_cse.latency_ms,
			configured: c.google_cse.configured,
			error: c.google_cse.error,
		});

		components.push({
			name: i18n._('admin.health.serper'),
			status: c.serper.status,
			latency: c.serper.latency_ms,
			configured: c.serper.configured,
			error: c.serper.error,
		});

		components.push({
			name: i18n._('admin.health.github_app'),
			status: c.github_app.status,
			latency: c.github_app.latency_ms,
			configured: c.github_app.configured,
			error: c.github_app.error,
		});

		if (c.kubernetes) {
			components.push({
				name: i18n._('admin.health.kubernetes'),
				status: c.kubernetes.status,
				latency: c.kubernetes.latency_ms,
				error: c.kubernetes.error,
				extra: c.kubernetes.namespace,
			});
		}

		components.push({
			name: i18n._('admin.health.smtp'),
			status: c.smtp.status,
			latency: c.smtp.latency_ms,
			configured: c.smtp.configured,
			error: c.smtp.error,
		});

		components.push({
			name: i18n._('admin.health.geoip'),
			status: c.geoip.status,
			latency: c.geoip.latency_ms,
			configured: c.geoip.configured,
			error: c.geoip.error,
			extra: c.geoip.database_type,
		});

		if (c.jobs) {
			components.push({
				name: i18n._('admin.health.jobs'),
				status: c.jobs.status,
				extra: `${c.jobs.jobs_healthy}/${c.jobs.jobs_total} healthy`,
				error: c.jobs.failing_jobs?.join(', '),
			});
		}

		// Auth providers
		if (c.auth_providers) {
			const configuredProviders = c.auth_providers.providers
				.filter((p) => p.configured)
				.map((p) => p.name)
				.join(', ');
			components.push({
				name: i18n._('admin.health.auth_providers'),
				status: c.auth_providers.status,
				extra: configuredProviders || 'none configured',
			});
		}

		return components;
	}

	$effect(() => {
		loadHealth();
		loadLogs();
		const interval = setInterval(loadHealth, 30000);
		return () => {
			clearInterval(interval);
			stopStreaming();
		};
	});

	onDestroy(() => {
		stopStreaming();
	});
</script>

<svelte:head>
	<title>{i18n._('admin.dashboard.title')} - Loom</title>
</svelte:head>

<div class="admin-page">
	<div class="page-header">
		<h1 class="page-title">{i18n._('admin.dashboard.title')}</h1>
		<p class="page-subtitle">{i18n._('admin.dashboard.description')}</p>
	</div>

	<ThreadDivider variant="gradient" />

	<section class="section">
		<h2 class="section-title">{i18n._('admin.dashboard.sections')}</h2>
		<div class="sections-grid">
			{#each adminSections as section}
				<a href={section.href} class="section-link">
					<Card hover={true}>
						<div class="section-card">
							<span class="section-icon">{section.icon}</span>
							<div>
								<h3 class="section-name">
									{i18n._(section.titleKey)}
								</h3>
								<p class="section-desc">
									{i18n._(section.descriptionKey)}
								</p>
							</div>
						</div>
					</Card>
				</a>
			{/each}
		</div>
	</section>

	<ThreadDivider variant="simple" />

	<div class="two-column">
		<section class="column">
			<div class="column-header">
				<h2 class="section-title">{i18n._('admin.health.title')}</h2>
				<button
					onclick={() => loadHealth()}
					disabled={loading}
					class="refresh-button"
				>
					{i18n._('general.refresh')}
				</button>
			</div>

			{#if error}
				<div class="error-message">{error}</div>
			{/if}

			{#if loading && !health}
				<Card>
					<div class="loading-state">{i18n._('general.loading')}</div>
				</Card>
			{:else if health}
				<LoomFrame variant="corners">
					<div class="status-overview">
						<div
							class="status-icon"
							class:status-healthy={health.status === 'healthy'}
							class:status-degraded={health.status === 'degraded'}
							class:status-unhealthy={health.status === 'unhealthy'}
						>
							{getStatusIcon(health.status)}
						</div>
						<div class="status-info">
							<div class="status-badges">
								<Badge variant={getStatusVariant(health.status)}>
									{i18n._(`admin.health.status.${health.status}`)}
								</Badge>
								<span class="status-sha">
									{health.version.git_sha}
								</span>
							</div>
							<div class="status-meta">
								{formatLatency(health.duration_ms)} · {new Date(health.timestamp).toLocaleTimeString()}
							</div>
						</div>
					</div>
				</LoomFrame>

				<div class="components-grid">
					{#each getComponents() as component}
						<Card padding="sm">
							<div class="component-item">
								<Badge variant={getStatusVariant(component.status)} size="sm">
									{component.status.charAt(0).toUpperCase()}
								</Badge>
								<span class="component-name">{component.name}</span>
							</div>
							{#if component.error}
								<div class="component-error" title={component.error}>
									{component.error}
								</div>
							{/if}
						</Card>
					{/each}
				</div>
			{/if}
		</section>

		<section class="column">
			<div class="column-header">
				<h2 class="section-title">
					{i18n._('admin.logs.title')}
					{#if streaming}
						<span class="streaming-dot"></span>
					{/if}
				</h2>
				<a href="/admin/logs" class="view-all-link">
					{i18n._('admin.dashboard.view_all_logs')}
				</a>
			</div>

			<div id="dashboard-log-container" class="log-container">
				{#if logsLoading && logs.length === 0}
					<div class="log-placeholder">{i18n._('general.loading')}</div>
				{:else if logs.length === 0}
					<div class="log-placeholder">{i18n._('admin.logs.no_logs')}</div>
				{:else}
					<div class="log-entries">
						{#each logs as log (log.id)}
							<div class="log-entry">
								<span class="log-time">{formatTimestamp(log.timestamp)}</span>
								<span
									class="log-level"
									class:level-debug={log.level === 'trace' || log.level === 'debug'}
									class:level-info={log.level === 'info'}
									class:level-warn={log.level === 'warn'}
									class:level-error={log.level === 'error'}
								>
									{log.level.toUpperCase().padEnd(5)}
								</span>
								<span class="log-message">{log.message}</span>
							</div>
						{/each}
					</div>
				{/if}
			</div>
		</section>
	</div>
</div>

<style>
	.admin-page {
		max-width: 1280px;
		margin: 0 auto;
		padding: var(--space-8) var(--space-4);
		font-family: var(--font-mono);
	}

	.page-header {
		margin-bottom: var(--space-4);
	}

	.page-title {
		font-size: var(--text-2xl);
		font-weight: 600;
		color: var(--color-fg);
	}

	.page-subtitle {
		color: var(--color-fg-muted);
		font-size: var(--text-sm);
	}

	.section {
		margin-bottom: var(--space-8);
	}

	.section-title {
		font-size: var(--text-lg);
		font-weight: 500;
		color: var(--color-fg);
		margin-bottom: var(--space-4);
		display: flex;
		align-items: center;
		gap: var(--space-2);
	}

	.sections-grid {
		display: grid;
		grid-template-columns: repeat(1, 1fr);
		gap: var(--space-4);
	}

	@media (min-width: 768px) {
		.sections-grid {
			grid-template-columns: repeat(2, 1fr);
		}
	}

	@media (min-width: 1024px) {
		.sections-grid {
			grid-template-columns: repeat(4, 1fr);
		}
	}

	.section-link {
		display: block;
		text-decoration: none;
	}

	.section-card {
		display: flex;
		align-items: flex-start;
		gap: var(--space-3);
	}

	.section-icon {
		font-size: var(--text-2xl);
	}

	.section-name {
		font-weight: 500;
		color: var(--color-fg);
		font-size: var(--text-sm);
	}

	.section-link:hover .section-name {
		color: var(--color-accent);
	}

	.section-desc {
		font-size: var(--text-xs);
		color: var(--color-fg-muted);
	}

	.two-column {
		display: grid;
		grid-template-columns: 1fr;
		gap: var(--space-6);
	}

	@media (min-width: 1024px) {
		.two-column {
			grid-template-columns: 1fr 1fr;
		}
	}

	.column-header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		margin-bottom: var(--space-4);
	}

	.refresh-button {
		font-size: var(--text-sm);
		color: var(--color-accent);
		background: transparent;
		border: none;
		cursor: pointer;
		font-family: var(--font-mono);
	}

	.refresh-button:hover {
		color: var(--color-accent-hover);
	}

	.refresh-button:disabled {
		opacity: 0.5;
		cursor: not-allowed;
	}

	.error-message {
		padding: var(--space-3);
		border-radius: var(--radius-md);
		background: var(--color-error-soft);
		color: var(--color-error);
		font-size: var(--text-sm);
		margin-bottom: var(--space-4);
	}

	.loading-state {
		text-align: center;
		padding: var(--space-8);
		color: var(--color-fg-muted);
	}

	.status-overview {
		display: flex;
		align-items: center;
		gap: var(--space-4);
	}

	.status-icon {
		width: 40px;
		height: 40px;
		border-radius: var(--radius-full);
		display: flex;
		align-items: center;
		justify-content: center;
		font-size: var(--text-lg);
		font-weight: 600;
		color: var(--color-bg);
	}

	.status-healthy {
		background: var(--color-success);
	}

	.status-degraded {
		background: var(--color-warning);
	}

	.status-unhealthy {
		background: var(--color-error);
	}

	.status-info {
		flex: 1;
	}

	.status-badges {
		display: flex;
		align-items: center;
		gap: var(--space-2);
	}

	.status-sha {
		font-size: var(--text-xs);
		color: var(--color-fg-muted);
	}

	.status-meta {
		font-size: var(--text-xs);
		color: var(--color-fg-muted);
		margin-top: var(--space-1);
	}

	.components-grid {
		display: grid;
		grid-template-columns: repeat(2, 1fr);
		gap: var(--space-2);
		margin-top: var(--space-4);
	}

	.component-item {
		display: flex;
		align-items: center;
		gap: var(--space-2);
	}

	.component-name {
		font-size: var(--text-sm);
		color: var(--color-fg);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.component-error {
		font-size: var(--text-xs);
		color: var(--color-error);
		margin-top: var(--space-1);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.streaming-dot {
		display: inline-block;
		width: 8px;
		height: 8px;
		border-radius: var(--radius-full);
		background: var(--color-success);
		animation: pulse 1s ease-in-out infinite;
	}

	@keyframes pulse {
		0%, 100% {
			opacity: 1;
		}
		50% {
			opacity: 0.5;
		}
	}

	.view-all-link {
		font-size: var(--text-sm);
		color: var(--color-accent);
		text-decoration: none;
	}

	.view-all-link:hover {
		color: var(--color-accent-hover);
	}

	.log-container {
		background: var(--color-bg-muted);
		border-radius: var(--radius-md);
		border: 1px solid var(--color-border);
		overflow: auto;
		height: 400px;
	}

	.log-placeholder {
		color: var(--color-fg-muted);
		text-align: center;
		padding: var(--space-8);
		font-size: var(--text-sm);
	}

	.log-entries {
		padding: var(--space-2);
	}

	.log-entry {
		display: flex;
		gap: var(--space-2);
		padding: var(--space-1);
		border-radius: var(--radius-sm);
	}

	.log-entry:hover {
		background: var(--color-bg-subtle);
	}

	.log-time {
		color: var(--color-fg-subtle);
		flex-shrink: 0;
		font-size: var(--text-xs);
	}

	.log-level {
		flex-shrink: 0;
		width: 48px;
		font-size: var(--text-xs);
	}

	.level-debug {
		color: var(--color-fg-subtle);
	}

	.level-info {
		color: var(--color-info);
	}

	.level-warn {
		color: var(--color-warning);
	}

	.level-error {
		color: var(--color-error);
	}

	.log-message {
		color: var(--color-fg);
		font-size: var(--text-xs);
		word-break: break-all;
	}
</style>
