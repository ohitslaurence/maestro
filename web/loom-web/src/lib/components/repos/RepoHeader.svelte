<!--
  Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
  SPDX-License-Identifier: Proprietary
-->
<script lang="ts">
	import type { Repository } from '$lib/api/repos';
	import { i18n } from '$lib/i18n';
	import { Badge, Button } from '$lib/ui';

	interface Props {
		repo: Repository;
	}

	let { repo }: Props = $props();

	let showCloneUrl = $state(false);
	let copied = $state(false);

	async function copyCloneUrl() {
		await navigator.clipboard.writeText(repo.clone_url);
		copied = true;
		setTimeout(() => (copied = false), 2000);
	}
</script>

<div class="repo-header">
	<div class="repo-info">
		<svg class="repo-icon" fill="none" stroke="currentColor" viewBox="0 0 24 24">
			<path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" d="M3 7v10a2 2 0 002 2h14a2 2 0 002-2V9a2 2 0 00-2-2h-6l-2-2H5a2 2 0 00-2 2z" />
		</svg>
		<div class="repo-details">
			<h1 class="repo-title">
				<a href="/repos/{repo.owner_id}" class="owner-link">{repo.owner_id}</a>
				<span class="title-separator">/</span>
				<span>{repo.name}</span>
			</h1>
			<div class="repo-meta">
				<Badge variant={repo.visibility === 'public' ? 'success' : 'muted'} size="sm">
					{repo.visibility}
				</Badge>
				<span class="default-branch">
					{i18n.t('client.repos.header.default_branch')}: <code class="branch-name">{repo.default_branch}</code>
				</span>
			</div>
		</div>
	</div>

	<div class="clone-container">
		<Button variant="secondary" size="sm" onclick={() => (showCloneUrl = !showCloneUrl)}>
			<svg class="clone-icon" fill="none" stroke="currentColor" viewBox="0 0 24 24">
				<path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M8 16H6a2 2 0 01-2-2V6a2 2 0 012-2h8a2 2 0 012 2v2m-6 12h8a2 2 0 002-2v-8a2 2 0 00-2-2h-8a2 2 0 00-2 2v8a2 2 0 002 2z" />
			</svg>
			{i18n.t('client.repos.header.clone')}
		</Button>

		{#if showCloneUrl}
			<div class="clone-dropdown">
				<div class="clone-label">{i18n.t('client.repos.header.clone_https')}</div>
				<div class="clone-input-row">
					<input
						type="text"
						readonly
						value={repo.clone_url}
						class="clone-url-input"
					/>
					<Button variant="secondary" size="sm" onclick={copyCloneUrl}>
						{copied ? i18n.t('client.repos.header.copied') : i18n.t('client.repos.header.copy')}
					</Button>
				</div>
			</div>
		{/if}
	</div>
</div>

<style>
	.repo-header {
		display: flex;
		align-items: flex-start;
		justify-content: space-between;
		gap: var(--space-4);
		margin-bottom: var(--space-6);
	}

	.repo-info {
		display: flex;
		align-items: center;
		gap: var(--space-3);
	}

	.repo-icon {
		width: 2rem;
		height: 2rem;
		color: var(--color-fg-muted);
	}

	.repo-details {
		display: flex;
		flex-direction: column;
	}

	.repo-title {
		font-family: var(--font-mono);
		font-size: var(--text-xl);
		font-weight: 700;
		color: var(--color-fg);
		display: flex;
		align-items: center;
		gap: var(--space-2);
	}

	.owner-link {
		color: var(--color-fg);
	}

	.owner-link:hover {
		color: var(--color-accent);
	}

	.title-separator {
		color: var(--color-fg-muted);
	}

	.repo-meta {
		display: flex;
		align-items: center;
		gap: var(--space-2);
		margin-top: var(--space-1);
	}

	.default-branch {
		font-family: var(--font-mono);
		font-size: var(--text-sm);
		color: var(--color-fg-muted);
	}

	.branch-name {
		font-family: var(--font-mono);
		font-size: var(--text-xs);
		background: var(--color-bg-muted);
		padding: 2px var(--space-1);
		border-radius: var(--radius-md);
	}

	.clone-container {
		position: relative;
	}

	.clone-icon {
		width: 1rem;
		height: 1rem;
		margin-right: var(--space-2);
	}

	.clone-dropdown {
		position: absolute;
		right: 0;
		margin-top: var(--space-2);
		padding: var(--space-3);
		background: var(--color-bg);
		border: 1px solid var(--color-border);
		border-radius: var(--radius-md);
		box-shadow: var(--shadow-lg);
		z-index: 10;
		width: 20rem;
	}

	.clone-label {
		font-family: var(--font-mono);
		font-size: var(--text-sm);
		font-weight: 500;
		color: var(--color-fg);
		margin-bottom: var(--space-2);
	}

	.clone-input-row {
		display: flex;
		gap: var(--space-2);
	}

	.clone-url-input {
		flex: 1;
		font-family: var(--font-mono);
		font-size: var(--text-sm);
		background: var(--color-bg-muted);
		border: 1px solid var(--color-border);
		border-radius: var(--radius-md);
		padding: var(--space-1) var(--space-2);
		color: var(--color-fg);
	}
</style>
