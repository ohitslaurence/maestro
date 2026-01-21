<!--
  Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
  SPDX-License-Identifier: Proprietary
-->
<script lang="ts">
	import { page } from '$app/stores';
	import { i18n } from '$lib/i18n';

	interface Props {
		owner: string;
		repo: string;
		defaultBranch: string;
	}

	let { owner, repo, defaultBranch }: Props = $props();

	const basePath = $derived(`/repos/${owner}/${repo}`);
	const currentPath = $derived($page.url.pathname);

	const tabs = $derived([
		{
			label: i18n.t('client.repos.nav.code'),
			href: `${basePath}/tree/${defaultBranch}`,
			icon: 'code',
			active: currentPath.includes('/tree/') || currentPath.includes('/blob/'),
		},
		{
			label: i18n.t('client.repos.nav.commits'),
			href: `${basePath}/commits/${defaultBranch}`,
			icon: 'history',
			active: currentPath.includes('/commits/') || currentPath.includes('/commit/'),
		},
		{
			label: i18n.t('client.repos.nav.branches'),
			href: `${basePath}/branches`,
			icon: 'branch',
			active: currentPath.includes('/branches'),
		},
		{
			label: i18n.t('client.repos.nav.settings'),
			href: `${basePath}/settings`,
			icon: 'settings',
			active: currentPath.includes('/settings'),
		},
	]);
</script>

<nav class="repo-nav">
	<div class="nav-tabs">
		{#each tabs as tab}
			<a
				href={tab.href}
				class="nav-tab"
				class:nav-tab-active={tab.active}
			>
				{#if tab.icon === 'code'}
					<svg class="tab-icon" fill="none" stroke="currentColor" viewBox="0 0 24 24">
						<path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M10 20l4-16m4 4l4 4-4 4M6 16l-4-4 4-4" />
					</svg>
				{:else if tab.icon === 'history'}
					<svg class="tab-icon" fill="none" stroke="currentColor" viewBox="0 0 24 24">
						<path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 8v4l3 3m6-3a9 9 0 11-18 0 9 9 0 0118 0z" />
					</svg>
				{:else if tab.icon === 'branch'}
					<svg class="tab-icon" fill="none" stroke="currentColor" viewBox="0 0 24 24">
						<path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M13 10V3L4 14h7v7l9-11h-7z" />
					</svg>
				{:else if tab.icon === 'settings'}
					<svg class="tab-icon" fill="none" stroke="currentColor" viewBox="0 0 24 24">
						<path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.065 2.572c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.572 1.065c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.065-2.572c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.572-1.065z" />
						<path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 12a3 3 0 11-6 0 3 3 0 016 0z" />
					</svg>
				{/if}
				{tab.label}
			</a>
		{/each}
	</div>
</nav>

<style>
	.repo-nav {
		border-bottom: 1px solid var(--color-border);
		margin-bottom: var(--space-6);
	}

	.nav-tabs {
		display: flex;
		gap: var(--space-1);
	}

	.nav-tab {
		display: flex;
		align-items: center;
		gap: var(--space-2);
		padding: var(--space-3) var(--space-4);
		font-family: var(--font-mono);
		font-size: var(--text-sm);
		font-weight: 500;
		color: var(--color-fg-muted);
		border-bottom: 2px solid transparent;
		transition: color 0.15s ease, border-color 0.15s ease;
	}

	.nav-tab:hover {
		color: var(--color-fg);
		border-color: var(--color-border);
	}

	.nav-tab-active {
		color: var(--color-accent);
		border-color: var(--color-accent);
	}

	.tab-icon {
		width: 1rem;
		height: 1rem;
	}
</style>
