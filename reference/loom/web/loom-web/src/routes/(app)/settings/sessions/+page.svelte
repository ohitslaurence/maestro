<!--
  Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
  SPDX-License-Identifier: Proprietary
-->
<script lang="ts">
	import { i18n } from '$lib/i18n';
	import { getApiClient } from '$lib/api/client';
	import type { Session } from '$lib/api/types';
	import { Card, Badge, Button, ThreadDivider } from '$lib/ui';

	let sessions: Session[] = $state([]);
	let loading = $state(true);
	let error = $state<string | null>(null);
	let revokingId = $state<string | null>(null);

	const client = getApiClient();

	async function loadSessions() {
		loading = true;
		error = null;
		try {
			const response = await client.listSessions();
			sessions = response.sessions;
		} catch (e) {
			error = e instanceof Error ? e.message : i18n._('settings.sessions.loadError');
		} finally {
			loading = false;
		}
	}

	async function revokeSession(sessionId: string) {
		if (!confirm(i18n._('settings.sessions.revokeConfirm'))) {
			return;
		}

		revokingId = sessionId;
		try {
			await client.revokeSession(sessionId);
			sessions = sessions.filter((s) => s.id !== sessionId);
		} catch (e) {
			error = e instanceof Error ? e.message : i18n._('settings.sessions.revokeError');
		} finally {
			revokingId = null;
		}
	}

	function getSessionTypeLabel(type: Session['session_type']): string {
		switch (type) {
			case 'web':
				return i18n._('settings.sessions.web');
			case 'cli':
				return i18n._('settings.sessions.cli');
			case 'vscode':
				return i18n._('settings.sessions.vscode');
		}
	}

	function getSessionTypeVariant(type: Session['session_type']): 'accent' | 'success' | 'warning' {
		switch (type) {
			case 'web':
				return 'accent';
			case 'cli':
				return 'success';
			case 'vscode':
				return 'warning';
		}
	}

	function formatDate(dateStr: string): string {
		return new Date(dateStr).toLocaleString();
	}

	$effect(() => {
		loadSessions();
	});
</script>

<svelte:head>
	<title>{i18n._('settings.sessions.title')} - Loom</title>
</svelte:head>

<div class="sessions-page">
	<h1 class="page-title">
		{i18n._('settings.sessions.title')}
	</h1>
	<p class="page-subtitle">
		{i18n._('settings.sessions.description')}
	</p>

	<ThreadDivider variant="gradient" />

	{#if loading}
		<div class="loading-state">{i18n._('general.loading')}</div>
	{:else if error}
		<Card>
			<div class="error-state">
				<p class="error-text">{error}</p>
				<Button variant="secondary" onclick={loadSessions}>
					{i18n._('general.retry')}
				</Button>
			</div>
		</Card>
	{:else if sessions.length === 0}
		<Card>
			<p class="empty-text">{i18n._('settings.sessions.noSessions')}</p>
		</Card>
	{:else}
		<div class="session-list">
			{#each sessions as session (session.id)}
				<Card>
					<div class="session-item">
						<div class="session-info">
							<div class="session-badges">
								<Badge variant={getSessionTypeVariant(session.session_type)} size="sm">
									{getSessionTypeLabel(session.session_type)}
								</Badge>
								{#if session.is_current}
									<Badge variant="success" size="sm">
										{i18n._('settings.sessions.current')}
									</Badge>
								{/if}
							</div>

							<div class="session-details">
								{#if session.ip_address}
									<div class="session-ip">
										{session.ip_address}
										{#if session.geo_location}
											<span class="session-geo"> · {session.geo_location}</span>
										{/if}
									</div>
								{/if}

								{#if session.user_agent}
									<div class="session-ua" title={session.user_agent}>
										{session.user_agent}
									</div>
								{/if}

								<div class="session-times">
									<span>{i18n._('settings.sessions.lastUsed')}: {formatDate(session.last_used_at)}</span>
									<span class="time-separator">·</span>
									<span>{i18n._('settings.sessions.createdAt')}: {formatDate(session.created_at)}</span>
								</div>
							</div>
						</div>

						{#if !session.is_current}
							<Button
								variant="danger"
								size="sm"
								disabled={revokingId === session.id}
								loading={revokingId === session.id}
								onclick={() => revokeSession(session.id)}
							>
								{i18n._('settings.sessions.revoke')}
							</Button>
						{/if}
					</div>
				</Card>
			{/each}
		</div>
	{/if}
</div>

<style>
	.sessions-page {
		font-family: var(--font-mono);
	}

	.page-title {
		font-size: var(--text-2xl);
		font-weight: 600;
		color: var(--color-fg);
		margin-bottom: var(--space-2);
	}

	.page-subtitle {
		color: var(--color-fg-muted);
		font-size: var(--text-sm);
	}

	.loading-state {
		color: var(--color-fg-muted);
	}

	.error-state {
		text-align: center;
		padding: var(--space-4);
	}

	.error-text {
		color: var(--color-error);
		margin-bottom: var(--space-4);
	}

	.empty-text {
		color: var(--color-fg-muted);
	}

	.session-list {
		display: flex;
		flex-direction: column;
		gap: var(--space-4);
	}

	.session-item {
		display: flex;
		flex-direction: column;
		gap: var(--space-4);
	}

	@media (min-width: 640px) {
		.session-item {
			flex-direction: row;
			align-items: center;
			justify-content: space-between;
		}
	}

	.session-info {
		flex: 1;
		min-width: 0;
	}

	.session-badges {
		display: flex;
		align-items: center;
		gap: var(--space-2);
		margin-bottom: var(--space-2);
	}

	.session-details {
		display: flex;
		flex-direction: column;
		gap: var(--space-1);
		font-size: var(--text-sm);
	}

	.session-ip {
		color: var(--color-fg);
	}

	.session-geo {
		color: var(--color-fg-muted);
	}

	.session-ua {
		color: var(--color-fg-muted);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.session-times {
		color: var(--color-fg-muted);
	}

	.time-separator {
		margin: 0 var(--space-2);
	}
</style>
