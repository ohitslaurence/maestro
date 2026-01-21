<!--
  Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
  SPDX-License-Identifier: Proprietary
-->
<script lang="ts">
	import type { CommitInfo } from '$lib/api/repos';
	import { i18n } from '$lib/i18n';

	interface Props {
		commits: CommitInfo[];
		owner: string;
		repo: string;
	}

	let { commits, owner, repo }: Props = $props();

	const basePath = $derived(`/repos/${owner}/${repo}`);

	function formatDate(dateStr: string): string {
		const date = new Date(dateStr);
		const now = new Date();
		const diff = now.getTime() - date.getTime();

		const minutes = Math.floor(diff / 60000);
		if (minutes < 60) return i18n._('client.repos.commits.minutes_ago', { count: minutes });

		const hours = Math.floor(diff / 3600000);
		if (hours < 24) return i18n._('client.repos.commits.hours_ago', { count: hours });

		const days = Math.floor(diff / 86400000);
		if (days < 30) return i18n._('client.repos.commits.days_ago', { count: days });

		return date.toLocaleDateString('en-US', {
			year: 'numeric',
			month: 'short',
			day: 'numeric',
		});
	}

	function getCommitTitle(message: string): string {
		return message.split('\n')[0];
	}

	function getCommitBody(message: string): string | null {
		const lines = message.split('\n');
		if (lines.length <= 1) return null;
		return lines.slice(1).join('\n').trim();
	}

	function getAvatarUrl(email: string): string {
		return `https://www.gravatar.com/avatar/${email}?d=identicon&s=40`;
	}
</script>

<div class="commit-list">
	{#each commits as commit}
		<div class="commit-item">
			<img
				src={getAvatarUrl(commit.author_email)}
				alt={commit.author_name}
				class="commit-avatar"
			/>

			<div class="commit-content">
				<a
					href="{basePath}/commit/{commit.sha}"
					class="commit-message"
				>
					{getCommitTitle(commit.message)}
				</a>

				{#if getCommitBody(commit.message)}
					<button
						type="button"
						class="commit-expand-btn"
					>
						...
					</button>
				{/if}

				<div class="commit-meta">
					<span class="commit-author">{commit.author_name}</span>
					<span>{i18n._('client.repos.commits.committed')}</span>
					<span title={commit.author_date}>{formatDate(commit.author_date)}</span>
				</div>
			</div>

			<div class="commit-actions">
				<a
					href="{basePath}/commit/{commit.sha}"
					class="commit-sha"
					title={commit.sha}
				>
					{commit.sha.slice(0, 7)}
				</a>
				<button
					type="button"
					onclick={() => navigator.clipboard.writeText(commit.sha)}
					class="copy-btn"
					title={i18n._('client.repos.commits.copy_sha')}
				>
					<svg class="copy-icon" fill="none" stroke="currentColor" viewBox="0 0 24 24">
						<path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M8 16H6a2 2 0 01-2-2V6a2 2 0 012-2h8a2 2 0 012 2v2m-6 12h8a2 2 0 002-2v-8a2 2 0 00-2-2h-8a2 2 0 00-2 2v8a2 2 0 002 2z" />
					</svg>
				</button>
			</div>
		</div>
	{/each}

	{#if commits.length === 0}
		<div class="commit-empty">
			{i18n._('client.repos.commits.empty')}
		</div>
	{/if}
</div>

<style>
	.commit-list {
		border: 1px solid var(--color-border);
		border-radius: var(--radius-md);
		overflow: hidden;
	}

	.commit-item {
		display: flex;
		align-items: flex-start;
		gap: var(--space-4);
		padding: var(--space-3) var(--space-4);
		border-bottom: 1px solid var(--color-border);
		transition: background 0.15s ease;
	}

	.commit-item:last-child {
		border-bottom: none;
	}

	.commit-item:hover {
		background: var(--color-bg-muted);
	}

	.commit-avatar {
		width: 2.5rem;
		height: 2.5rem;
		border-radius: var(--radius-full);
		flex-shrink: 0;
	}

	.commit-content {
		flex: 1;
		min-width: 0;
	}

	.commit-message {
		font-family: var(--font-mono);
		font-weight: 500;
		color: var(--color-fg);
		display: -webkit-box;
		-webkit-line-clamp: 1;
		line-clamp: 1;
		-webkit-box-orient: vertical;
		overflow: hidden;
	}

	.commit-message:hover {
		color: var(--color-accent);
	}

	.commit-expand-btn {
		font-family: var(--font-mono);
		font-size: var(--text-xs);
		color: var(--color-fg-muted);
		background: none;
		border: none;
		padding: 0;
		margin-top: 2px;
		cursor: pointer;
	}

	.commit-expand-btn:hover {
		color: var(--color-fg);
	}

	.commit-meta {
		display: flex;
		align-items: center;
		gap: var(--space-2);
		margin-top: var(--space-1);
		font-family: var(--font-mono);
		font-size: var(--text-sm);
		color: var(--color-fg-muted);
	}

	.commit-author {
		font-weight: 500;
		color: var(--color-fg);
	}

	.commit-actions {
		display: flex;
		align-items: center;
		gap: var(--space-2);
		flex-shrink: 0;
	}

	.commit-sha {
		font-family: var(--font-mono);
		font-size: var(--text-sm);
		color: var(--color-accent);
	}

	.commit-sha:hover {
		text-decoration: underline;
	}

	.copy-btn {
		padding: var(--space-1);
		background: none;
		border: none;
		border-radius: var(--radius-md);
		cursor: pointer;
		transition: background 0.15s ease;
	}

	.copy-btn:hover {
		background: var(--color-bg-subtle);
	}

	.copy-icon {
		width: 1rem;
		height: 1rem;
		color: var(--color-fg-muted);
	}

	.commit-empty {
		padding: var(--space-8) var(--space-4);
		text-align: center;
		font-family: var(--font-mono);
		color: var(--color-fg-muted);
	}
</style>
