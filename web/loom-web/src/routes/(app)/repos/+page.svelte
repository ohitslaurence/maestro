<!--
  Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
  SPDX-License-Identifier: Proprietary
-->
<script lang="ts">
	import { getReposClient, type Repository } from '$lib/api/repos';
	import { getApiClient } from '$lib/api/client';
	import { Card, Badge, Button, Input, ThreadDivider, LoomFrame } from '$lib/ui';
	import { CreateRepoModal } from '$lib/components/repos';
	import { i18n } from '$lib/i18n';

	let repos = $state<Repository[]>([]);
	let loading = $state(true);
	let error = $state<string | null>(null);
	let searchQuery = $state('');
	let showCreateModal = $state(false);
	let currentUserId = $state<string | null>(null);

	const client = getReposClient();
	const apiClient = getApiClient();

	const filteredRepos = $derived(
		repos.filter(
			(r) =>
				r.name.toLowerCase().includes(searchQuery.toLowerCase()) ||
				r.owner_id.toLowerCase().includes(searchQuery.toLowerCase())
		)
	);

	async function loadRepos() {
		loading = true;
		error = null;

		try {
			const user = await apiClient.getCurrentUser();
			currentUserId = user.id;
			const response = await client.listRepos(user.id);
			repos = response.repos;
		} catch (e) {
			error = e instanceof Error ? e.message : i18n._('client.repos.list.loadError');
		} finally {
			loading = false;
		}
	}

	function handleRepoCreated(repo: Repository) {
		repos = [repo, ...repos];
		showCreateModal = false;
	}

	function formatDate(dateStr: string): string {
		return new Date(dateStr).toLocaleDateString('en-US', {
			year: 'numeric',
			month: 'short',
			day: 'numeric',
		});
	}

	$effect(() => {
		loadRepos();
	});
</script>

<svelte:head>
	<title>{i18n._('client.repos.list.title')} - Loom</title>
</svelte:head>

<div class="repos-page">
	<div class="header">
		<h1 class="title">{i18n._('client.repos.list.title')}</h1>
		<Button variant="primary" onclick={() => (showCreateModal = true)}>
			<svg class="icon" fill="none" stroke="currentColor" viewBox="0 0 24 24">
				<path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 4v16m8-8H4" />
			</svg>
			{i18n._('client.repos.list.new')}
		</Button>
	</div>

	<ThreadDivider variant="gradient" />

	<div class="search-bar">
		<Input
			type="search"
			placeholder={i18n._('client.repos.list.search_placeholder')}
			bind:value={searchQuery}
		/>
	</div>

	{#if loading}
		<div class="repo-list">
			{#each Array(3) as _}
				<Card>
					<div class="skeleton-item">
						<div class="skeleton-title"></div>
						<div class="skeleton-meta"></div>
					</div>
				</Card>
			{/each}
		</div>
	{:else if error}
		<Card>
			<div class="error-state">
				<p class="error-text">{error}</p>
				<Button variant="secondary" onclick={loadRepos}>
					{i18n._('client.repos.list.try_again')}
				</Button>
			</div>
		</Card>
	{:else if filteredRepos.length === 0}
		<LoomFrame variant="full">
			<div class="empty-state">
				{#if searchQuery}
					<p class="empty-text">{i18n._('client.repos.list.no_match')}</p>
				{:else}
					<svg class="empty-icon" fill="none" stroke="currentColor" viewBox="0 0 24 24">
						<path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" d="M3 7v10a2 2 0 002 2h14a2 2 0 002-2V9a2 2 0 00-2-2h-6l-2-2H5a2 2 0 00-2 2z" />
					</svg>
					<p class="empty-text">{i18n._('client.repos.list.empty')}</p>
					<Button variant="primary" onclick={() => (showCreateModal = true)}>
						{i18n._('client.repos.list.create_first')}
					</Button>
				{/if}
			</div>
		</LoomFrame>
	{:else}
		<div class="repo-list">
			{#each filteredRepos as repo (repo.id)}
				<Card>
					<div class="repo-item">
						<div class="repo-info">
							<div class="repo-header">
								<a
									href="/repos/{repo.owner_id}/{repo.name}"
									class="repo-link"
								>
									{repo.owner_id}/{repo.name}
								</a>
								<Badge variant={repo.visibility === 'public' ? 'success' : 'muted'} size="sm">
									{repo.visibility}
								</Badge>
							</div>
							<div class="repo-meta">
								<span>
									{i18n._('client.repos.list.default_branch')}
									<code class="branch-code">{repo.default_branch}</code>
								</span>
								<span>{i18n._('client.repos.list.updated')} {formatDate(repo.updated_at)}</span>
							</div>
						</div>
					</div>
				</Card>
			{/each}
		</div>
	{/if}
</div>

{#if currentUserId}
	<CreateRepoModal
		open={showCreateModal}
		onclose={() => (showCreateModal = false)}
		oncreate={handleRepoCreated}
		userId={currentUserId}
	/>
{/if}

<style>
	.repos-page {
		max-width: 1280px;
		margin: 0 auto;
		padding: var(--space-6) var(--space-4);
		font-family: var(--font-mono);
	}

	.header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: var(--space-4);
	}

	.title {
		font-size: var(--text-2xl);
		font-weight: 600;
		color: var(--color-fg);
	}

	.icon {
		width: 16px;
		height: 16px;
		margin-right: var(--space-2);
	}

	.search-bar {
		margin-bottom: var(--space-6);
	}

	.repo-list {
		display: flex;
		flex-direction: column;
		gap: var(--space-3);
	}

	.skeleton-item {
		animation: pulse 1.5s ease-in-out infinite;
	}

	.skeleton-title {
		height: 20px;
		background: var(--color-bg-muted);
		border-radius: var(--radius-md);
		width: 33%;
		margin-bottom: var(--space-2);
	}

	.skeleton-meta {
		height: 16px;
		background: var(--color-bg-muted);
		border-radius: var(--radius-md);
		width: 25%;
	}

	@keyframes pulse {
		0%, 100% {
			opacity: 1;
		}
		50% {
			opacity: 0.5;
		}
	}

	.error-state {
		text-align: center;
		padding: var(--space-8);
	}

	.error-text {
		color: var(--color-error);
		margin-bottom: var(--space-4);
	}

	.empty-state {
		text-align: center;
		padding: var(--space-8);
	}

	.empty-icon {
		width: 64px;
		height: 64px;
		margin: 0 auto var(--space-4);
		color: var(--color-fg-muted);
	}

	.empty-text {
		color: var(--color-fg-muted);
		margin-bottom: var(--space-4);
	}

	.repo-item {
		display: flex;
		align-items: flex-start;
		justify-content: space-between;
	}

	.repo-info {
		min-width: 0;
		flex: 1;
	}

	.repo-header {
		display: flex;
		align-items: center;
		gap: var(--space-2);
		margin-bottom: var(--space-1);
	}

	.repo-link {
		font-weight: 600;
		color: var(--color-accent);
		text-decoration: none;
	}

	.repo-link:hover {
		text-decoration: underline;
	}

	.repo-meta {
		display: flex;
		align-items: center;
		gap: var(--space-4);
		font-size: var(--text-sm);
		color: var(--color-fg-muted);
	}

	.branch-code {
		font-size: var(--text-xs);
		background: var(--color-bg-muted);
		padding: var(--space-1) var(--space-1);
		border-radius: var(--radius-sm);
	}
</style>
