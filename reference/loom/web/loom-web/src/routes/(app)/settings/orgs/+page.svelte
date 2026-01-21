<!--
  Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
  SPDX-License-Identifier: Proprietary
-->
<script lang="ts">
	import { i18n } from '$lib/i18n';
	import { getApiClient } from '$lib/api/client';
	import type { Org } from '$lib/api/types';
	import { Card, Badge, Button, ThreadDivider } from '$lib/ui';

	let orgs: Org[] = $state([]);
	let loading = $state(true);
	let error = $state<string | null>(null);

	const client = getApiClient();

	async function loadOrgs() {
		loading = true;
		error = null;
		try {
			const response = await client.listOrgs();
			orgs = response.orgs;
		} catch (e) {
			error = e instanceof Error ? e.message : i18n._('settings.orgs.loadError');
		} finally {
			loading = false;
		}
	}

	function getVisibilityVariant(visibility: Org['visibility']): 'accent' | 'success' | 'muted' {
		switch (visibility) {
			case 'public':
				return 'success';
			case 'unlisted':
				return 'accent';
			case 'private':
				return 'muted';
		}
	}

	function getVisibilityLabel(visibility: Org['visibility']): string {
		switch (visibility) {
			case 'public':
				return i18n._('settings.orgs.visibility.public');
			case 'unlisted':
				return i18n._('settings.orgs.visibility.unlisted');
			case 'private':
				return i18n._('settings.orgs.visibility.private');
		}
	}

	$effect(() => {
		loadOrgs();
	});
</script>

<svelte:head>
	<title>{i18n._('settings.orgs.title')} - Loom</title>
</svelte:head>

<div class="orgs-page">
	<div class="page-header">
		<div>
			<h1 class="page-title">
				{i18n._('settings.orgs.title')}
			</h1>
			<p class="page-subtitle">
				{i18n._('settings.orgs.description')}
			</p>
		</div>
		<a href="/settings/orgs/new">
			<Button>
				{i18n._('settings.orgs.create')}
			</Button>
		</a>
	</div>

	<ThreadDivider variant="gradient" />

	{#if loading}
		<div class="loading-state">{i18n._('general.loading')}</div>
	{:else if error}
		<Card>
			<div class="error-state">
				<p class="error-text">{error}</p>
				<Button variant="secondary" onclick={loadOrgs}>
					{i18n._('general.retry')}
				</Button>
			</div>
		</Card>
	{:else if orgs.length === 0}
		<Card>
			<p class="empty-text">{i18n._('settings.orgs.noOrgs')}</p>
		</Card>
	{:else}
		<div class="org-list">
			{#each orgs as org (org.id)}
				<a href="/settings/orgs/{org.id}" class="org-link">
					<Card hover>
						<div class="org-item">
							<div class="org-info">
								<div class="org-header">
									<span class="org-name">{org.name}</span>
									{#if org.is_personal}
										<Badge variant="muted" size="sm">
											{i18n._('settings.orgs.personal')}
										</Badge>
									{/if}
									<Badge variant={getVisibilityVariant(org.visibility)} size="sm">
										{getVisibilityLabel(org.visibility)}
									</Badge>
								</div>

								<div class="org-details">
									<div class="org-slug">
										{i18n._('settings.orgs.slug')}: <span class="slug-value">{org.slug}</span>
									</div>
									{#if org.member_count !== null}
										<div class="org-members">
											{org.member_count} {org.member_count === 1 ? i18n._('settings.orgs.member') : i18n._('settings.orgs.members')}
										</div>
									{/if}
								</div>
							</div>
						</div>
					</Card>
				</a>
			{/each}
		</div>
	{/if}
</div>

<style>
	.orgs-page {
		font-family: var(--font-mono);
	}

	.page-header {
		display: flex;
		align-items: flex-start;
		justify-content: space-between;
		gap: var(--space-4);
	}

	.page-title {
		font-size: var(--text-2xl);
		font-weight: 600;
		color: var(--color-fg);
	}

	.page-subtitle {
		color: var(--color-fg-muted);
		margin-top: var(--space-1);
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

	.org-list {
		display: flex;
		flex-direction: column;
		gap: var(--space-4);
	}

	.org-link {
		display: block;
		text-decoration: none;
	}

	.org-item {
		display: flex;
		flex-direction: column;
		gap: var(--space-4);
	}

	@media (min-width: 640px) {
		.org-item {
			flex-direction: row;
			align-items: center;
			justify-content: space-between;
		}
	}

	.org-info {
		flex: 1;
		min-width: 0;
	}

	.org-header {
		display: flex;
		align-items: center;
		gap: var(--space-2);
		margin-bottom: var(--space-2);
	}

	.org-name {
		font-weight: 600;
		color: var(--color-fg);
	}

	.org-details {
		display: flex;
		flex-direction: column;
		gap: var(--space-1);
		font-size: var(--text-sm);
	}

	.org-slug {
		color: var(--color-fg-muted);
	}

	.slug-value {
		font-family: var(--font-mono);
	}

	.org-members {
		color: var(--color-fg-muted);
	}
</style>
