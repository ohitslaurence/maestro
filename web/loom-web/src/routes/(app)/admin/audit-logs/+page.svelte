<!--
  Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
  SPDX-License-Identifier: Proprietary
-->
<script lang="ts">
	import { onMount } from 'svelte';
	import { Card, Badge, LoomFrame } from '$lib/ui';
	import { i18n } from '$lib/i18n';

	interface AuditLogEntry {
		id: string;
		timestamp: string;
		event_type: string;
		actor_user_id: string | null;
		impersonating_user_id: string | null;
		resource_type: string | null;
		resource_id: string | null;
		action: string | null;
		ip_address: string | null;
		user_agent: string | null;
		details: any | null;
	}

	interface AuditLogsResponse {
		logs: AuditLogEntry[];
		total: number;
		limit: number;
		offset: number;
	}

	let logs = $state<AuditLogEntry[]>([]);
	let total = $state(0);
	let loading = $state(true);
	let error = $state<string | null>(null);

	let eventTypeFilter = $state('');
	let limit = $state(50);
	let offset = $state(0);

	async function loadLogs() {
		loading = true;
		error = null;
		try {
			const params = new URLSearchParams();
			params.set('limit', limit.toString());
			params.set('offset', offset.toString());
			if (eventTypeFilter) params.set('event_type', eventTypeFilter);

			const res = await fetch(`/api/admin/audit-logs?${params}`, { credentials: 'include' });
			if (!res.ok) {
				const data = await res.json();
				throw new Error(data.message || `Failed to load audit logs: ${res.status}`);
			}
			const data: AuditLogsResponse = await res.json();
			logs = data.logs;
			total = data.total;
		} catch (e) {
			error = e instanceof Error ? e.message : 'Unknown error';
		} finally {
			loading = false;
		}
	}

	function formatTimestamp(ts: string): string {
		return new Date(ts).toLocaleString();
	}

	function getEventBadgeVariant(eventType: string): 'success' | 'warning' | 'error' | 'muted' {
		if (eventType.includes('failed') || eventType.includes('denied')) return 'error';
		if (eventType.includes('deleted') || eventType.includes('revoked')) return 'warning';
		if (eventType.includes('created') || eventType.includes('login')) return 'success';
		return 'muted';
	}

	function prevPage() {
		if (offset >= limit) {
			offset -= limit;
			loadLogs();
		}
	}

	function nextPage() {
		if (offset + limit < total) {
			offset += limit;
			loadLogs();
		}
	}

	onMount(() => {
		loadLogs();
	});
</script>

<LoomFrame>
	<div class="audit-logs-page">
		<div class="header">
			<h1>{i18n.t('admin.auditLogs.title')}</h1>
			<div class="filters">
				<select bind:value={eventTypeFilter} onchange={() => { offset = 0; loadLogs(); }}>
					<option value="">{i18n.t('admin.auditLogs.event.all')}</option>
					<option value="login">{i18n.t('admin.auditLogs.event.login')}</option>
					<option value="logout">{i18n.t('admin.auditLogs.event.logout')}</option>
					<option value="login_failed">{i18n.t('admin.auditLogs.event.loginFailed')}</option>
					<option value="access_denied">{i18n.t('admin.auditLogs.event.accessDenied')}</option>
					<option value="session_revoked">{i18n.t('admin.auditLogs.event.sessionRevoked')}</option>
					<option value="api_key_created">{i18n.t('admin.auditLogs.event.apiKeyCreated')}</option>
					<option value="api_key_revoked">{i18n.t('admin.auditLogs.event.apiKeyRevoked')}</option>
					<option value="org_created">{i18n.t('admin.auditLogs.event.orgCreated')}</option>
					<option value="member_added">{i18n.t('admin.auditLogs.event.memberAdded')}</option>
					<option value="member_removed">{i18n.t('admin.auditLogs.event.memberRemoved')}</option>
					<option value="impersonation_started">{i18n.t('admin.auditLogs.event.impersonationStarted')}</option>
					<option value="impersonation_ended">{i18n.t('admin.auditLogs.event.impersonationEnded')}</option>
				</select>
				<button onclick={loadLogs} disabled={loading}>
					{loading ? i18n.t('general.loading') : i18n.t('general.refresh')}
				</button>
			</div>
		</div>

		{#if error}
			<div class="error">{error}</div>
		{/if}

		{#if loading && logs.length === 0}
			<div class="loading">{i18n.t('admin.auditLogs.loading')}</div>
		{:else if logs.length === 0}
			<div class="empty">{i18n.t('admin.auditLogs.empty')}</div>
		{:else}
			<div class="logs-table-container">
				<table class="logs-table">
					<thead>
						<tr>
							<th>{i18n.t('admin.auditLogs.column.timestamp')}</th>
							<th>{i18n.t('admin.auditLogs.column.event')}</th>
							<th>{i18n.t('admin.auditLogs.column.actor')}</th>
							<th>{i18n.t('admin.auditLogs.column.resource')}</th>
							<th>{i18n.t('admin.auditLogs.column.details')}</th>
						</tr>
					</thead>
					<tbody>
						{#each logs as log}
							<tr>
								<td class="timestamp">{formatTimestamp(log.timestamp)}</td>
								<td>
									<Badge variant={getEventBadgeVariant(log.event_type)}>
										{log.event_type}
									</Badge>
								</td>
								<td class="actor">
									{log.actor_user_id || '-'}
									{#if log.impersonating_user_id}
										<span class="impersonating">({i18n.t('admin.auditLogs.via', { userId: log.impersonating_user_id })})</span>
									{/if}
								</td>
								<td class="resource">
									{#if log.resource_type}
										{log.resource_type}: {log.resource_id || '-'}
									{:else}
										-
									{/if}
								</td>
								<td class="details">
									{#if log.details}
										<code>{JSON.stringify(log.details)}</code>
									{:else}
										-
									{/if}
								</td>
							</tr>
						{/each}
					</tbody>
				</table>
			</div>

			<div class="pagination">
				<button onclick={prevPage} disabled={offset === 0}>{i18n.t('general.previous')}</button>
				<span>{i18n.t('admin.auditLogs.showing', { start: offset + 1, end: Math.min(offset + limit, total), total })}</span>
				<button onclick={nextPage} disabled={offset + limit >= total}>{i18n.t('general.next')}</button>
			</div>
		{/if}
	</div>
</LoomFrame>

<style>
	.audit-logs-page {
		padding: var(--space-4);
	}

	.header {
		display: flex;
		justify-content: space-between;
		align-items: center;
		margin-bottom: var(--space-4);
	}

	h1 {
		font-size: var(--text-xl);
		font-weight: 600;
	}

	.filters {
		display: flex;
		gap: var(--space-2);
	}

	select, button {
		padding: var(--space-2) var(--space-3);
		border: 1px solid var(--color-border);
		border-radius: var(--radius-md);
		background: var(--color-bg);
		color: var(--color-fg);
		font-size: var(--text-sm);
	}

	button:hover:not(:disabled) {
		background: var(--color-bg-muted);
	}

	button:disabled {
		opacity: 0.5;
		cursor: not-allowed;
	}

	.error {
		padding: var(--space-3);
		background: var(--color-error-soft);
		color: var(--color-error);
		border-radius: var(--radius-md);
		margin-bottom: var(--space-4);
	}

	.loading, .empty {
		text-align: center;
		padding: var(--space-8);
		color: var(--color-fg-muted);
	}

	.logs-table-container {
		overflow-x: auto;
	}

	.logs-table {
		width: 100%;
		border-collapse: collapse;
		font-size: var(--text-sm);
	}

	.logs-table th,
	.logs-table td {
		padding: var(--space-2) var(--space-3);
		text-align: left;
		border-bottom: 1px solid var(--color-border);
	}

	.logs-table th {
		background: var(--color-bg-muted);
		font-weight: 600;
	}

	.logs-table tr:hover {
		background: var(--color-bg-subtle);
	}

	.timestamp {
		white-space: nowrap;
		color: var(--color-fg-muted);
		font-family: var(--font-mono);
		font-size: var(--text-xs);
	}

	.actor {
		font-family: var(--font-mono);
		font-size: var(--text-xs);
	}

	.impersonating {
		color: var(--color-warning);
		font-size: var(--text-xs);
	}

	.resource {
		font-family: var(--font-mono);
		font-size: var(--text-xs);
	}

	.details code {
		font-size: var(--text-xs);
		background: var(--color-bg-muted);
		padding: var(--space-1);
		border-radius: var(--radius-sm);
		max-width: 300px;
		overflow: hidden;
		text-overflow: ellipsis;
		display: block;
	}

	.pagination {
		display: flex;
		justify-content: center;
		align-items: center;
		gap: var(--space-4);
		margin-top: var(--space-4);
	}

	.pagination span {
		color: var(--color-fg-muted);
		font-size: var(--text-sm);
	}
</style>
