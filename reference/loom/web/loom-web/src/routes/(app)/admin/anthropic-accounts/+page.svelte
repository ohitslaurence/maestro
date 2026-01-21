<!--
  Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
  SPDX-License-Identifier: Proprietary
-->
<script lang="ts">
	import { i18n } from '$lib/i18n';
	import { Card, Badge, Button } from '$lib/ui';
	import {
		listAnthropicAccounts,
		initiateAnthropicOAuth,
		completeAnthropicOAuth,
		removeAnthropicAccount,
		type AnthropicAccount,
		type AccountsSummary,
	} from '$lib/api/anthropic';

	let accounts = $state<AnthropicAccount[]>([]);
	let summary = $state<AccountsSummary | null>(null);
	let loading = $state(true);
	let error = $state<string | null>(null);
	let addingAccount = $state(false);
	let removingId = $state<string | null>(null);
	let successMessage = $state<string | null>(null);
	let notConfigured = $state(false);

	let showCodeModal = $state(false);
	let oauthState = $state<string | null>(null);
	let authCode = $state('');
	let submittingCode = $state(false);

	async function loadAccounts() {
		loading = true;
		error = null;
		notConfigured = false;
		try {
			const response = await listAnthropicAccounts();
			accounts = response.accounts;
			summary = response.summary;
		} catch (e) {
			if (e instanceof Error && e.message.includes('501')) {
				notConfigured = true;
			} else {
				error = e instanceof Error ? e.message : i18n._('general.error');
			}
		} finally {
			loading = false;
		}
	}

	async function handleAddAccount() {
		addingAccount = true;
		error = null;
		try {
			const response = await initiateAnthropicOAuth('/admin/anthropic-accounts');
			oauthState = response.state;
			window.open(response.redirect_url, '_blank');
			showCodeModal = true;
		} catch (e) {
			error = e instanceof Error ? e.message : i18n._('general.error');
		} finally {
			addingAccount = false;
		}
	}

	async function handleSubmitCode() {
		if (!oauthState || !authCode.trim()) {
			return;
		}
		submittingCode = true;
		error = null;
		try {
			const response = await completeAnthropicOAuth(authCode.trim(), oauthState);
			successMessage = i18n._('admin.anthropic.account_added', { id: response.account_id });
			showCodeModal = false;
			authCode = '';
			oauthState = null;
			await loadAccounts();
		} catch (e) {
			error = e instanceof Error ? e.message : i18n._('general.error');
		} finally {
			submittingCode = false;
		}
	}

	function handleCancelOAuth() {
		showCodeModal = false;
		authCode = '';
		oauthState = null;
	}

	async function handleRemove(accountId: string) {
		if (!confirm(i18n._('admin.anthropic.remove_confirm'))) {
			return;
		}
		removingId = accountId;
		error = null;
		try {
			await removeAnthropicAccount(accountId);
			await loadAccounts();
		} catch (e) {
			error = e instanceof Error ? e.message : i18n._('general.error');
		} finally {
			removingId = null;
		}
	}

	function getStatusBadge(status: string): { variant: 'success' | 'warning' | 'error'; label: string } {
		switch (status) {
			case 'available':
				return { variant: 'success', label: i18n._('admin.anthropic.status.available') };
			case 'cooling_down':
				return { variant: 'warning', label: i18n._('admin.anthropic.status.cooling_down') };
			case 'disabled':
				return { variant: 'error', label: i18n._('admin.anthropic.status.disabled') };
			default:
				return { variant: 'error', label: status };
		}
	}

	function formatCooldown(secs: number): string {
		const hours = Math.floor(secs / 3600);
		const minutes = Math.floor((secs % 3600) / 60);
		if (hours > 0) {
			return `${hours}h ${minutes}m`;
		}
		return `${minutes}m`;
	}

	function formatExpiry(dateStr: string): string {
		const date = new Date(dateStr);
		const now = new Date();
		const diffMs = date.getTime() - now.getTime();
		if (diffMs <= 0) {
			return i18n._('admin.anthropic.expired');
		}
		const diffMins = Math.floor(diffMs / 60000);
		if (diffMins < 60) {
			return i18n._('admin.anthropic.expiresInMinutes', { count: diffMins });
		}
		const diffHours = Math.floor(diffMins / 60);
		if (diffHours < 24) {
			return i18n._('admin.anthropic.expiresInHours', { count: diffHours });
		}
		const diffDays = Math.floor(diffHours / 24);
		return i18n._('admin.anthropic.expiresInDays', { count: diffDays });
	}

	$effect(() => {
		loadAccounts();
	});
</script>

<svelte:head>
	<title>{i18n._('admin.anthropic.title')} - Loom</title>
</svelte:head>

<div class="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8 py-8">
	<div class="mb-6 flex items-center justify-between">
		<div>
			<h1 class="text-2xl font-bold text-fg">{i18n._('admin.anthropic.title')}</h1>
		</div>
		{#if !notConfigured}
			<Button onclick={handleAddAccount} disabled={addingAccount} loading={addingAccount}>
				{i18n._('admin.anthropic.add_account')}
			</Button>
		{/if}
	</div>

	{#if successMessage}
		<div class="mb-4 p-3 rounded-md bg-success/10 text-success text-sm">
			{successMessage}
		</div>
	{/if}

	{#if error}
		<div class="mb-4 p-3 rounded-md bg-error/10 text-error text-sm">{error}</div>
	{/if}

	{#if notConfigured}
		<Card>
			<p class="text-fg-muted text-center py-8">{i18n._('admin.anthropic.not_configured')}</p>
		</Card>
	{:else if loading && accounts.length === 0}
		<Card>
			<div class="text-fg-muted text-center py-8">{i18n._('general.loading')}</div>
		</Card>
	{:else if accounts.length === 0}
		<Card>
			<p class="text-fg-muted text-center py-8">{i18n._('admin.anthropic.no_accounts')}</p>
		</Card>
	{:else}
		<div class="space-y-3">
			{#each accounts as account (account.id)}
				{@const badge = getStatusBadge(account.status)}
				<Card>
					<div class="flex items-start justify-between gap-4">
						<div class="flex items-start gap-3 min-w-0">
							<div class="mt-1">
								{#if account.status === 'available'}
									<span class="inline-block w-3 h-3 rounded-full bg-success"></span>
								{:else if account.status === 'cooling_down'}
									<span class="inline-block w-3 h-3 rounded-full bg-warning"></span>
								{:else}
									<span class="inline-block w-3 h-3 rounded-full bg-error"></span>
								{/if}
							</div>
							<div class="min-w-0">
								<div class="font-medium text-fg">{account.id}</div>
								<div class="flex flex-wrap gap-2 mt-1">
									<Badge variant={badge.variant} size="sm">{badge.label}</Badge>
								</div>
								{#if account.status === 'cooling_down' && account.cooldown_remaining_secs}
									<div class="text-sm text-fg-muted mt-1">
										{formatCooldown(account.cooldown_remaining_secs)} {i18n._('admin.anthropic.remaining')}
									</div>
								{/if}
								{#if account.last_error}
									<div class="text-sm text-error mt-1">
										{account.last_error}
									</div>
								{/if}
								{#if account.expires_at}
									<div class="text-sm text-fg-muted mt-1">
										{i18n._('admin.anthropic.tokenExpires')} {formatExpiry(account.expires_at)}
									</div>
								{/if}
							</div>
						</div>
						<Button
							variant="secondary"
							size="sm"
							onclick={() => handleRemove(account.id)}
							disabled={removingId === account.id}
							loading={removingId === account.id}
						>
							{i18n._('admin.anthropic.remove')}
						</Button>
					</div>
				</Card>
			{/each}
		</div>

		{#if summary}
			<div class="mt-6 text-sm text-fg-muted text-center">
				{summary.available} {i18n._('admin.anthropic.summary.available')}, {summary.cooling_down} {i18n._('admin.anthropic.summary.cooling')}, {summary.disabled} {i18n._('admin.anthropic.summary.disabled')} ({summary.total} {i18n._('admin.anthropic.summary.total')})
			</div>
		{/if}
	{/if}
</div>

{#if showCodeModal}
	<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
	<div class="fixed inset-0 bg-black/50 flex items-center justify-center z-50" role="dialog" aria-modal="true" tabindex="-1" onkeydown={(e) => e.key === 'Escape' && handleCancelOAuth()}>
		<div class="bg-bg-secondary rounded-lg shadow-xl max-w-lg w-full mx-4 p-6" role="document">
			<h2 class="text-xl font-semibold text-fg mb-4">{i18n._('admin.anthropic.enter_code_title')}</h2>
			<p class="text-fg-muted text-sm mb-4">
				{i18n._('admin.anthropic.enter_code_description')}
			</p>
			{#if error}
				<div class="mb-4 p-3 rounded-md bg-error/10 text-error text-sm">{error}</div>
			{/if}
			<div class="mb-4">
				<label for="auth-code" class="block text-sm font-medium text-fg mb-1">
					{i18n._('admin.anthropic.authorization_code')}
				</label>
				<input
					id="auth-code"
					type="text"
					bind:value={authCode}
					class="w-full px-3 py-2 rounded-md border border-border bg-bg text-fg focus:outline-none focus:ring-2 focus:ring-accent"
					placeholder={i18n._('admin.anthropic.code_placeholder')}
					disabled={submittingCode}
				/>
			</div>
			<div class="flex justify-end gap-3">
				<Button variant="secondary" onclick={handleCancelOAuth} disabled={submittingCode}>
					{i18n._('general.cancel')}
				</Button>
				<Button onclick={handleSubmitCode} disabled={!authCode.trim() || submittingCode} loading={submittingCode}>
					{i18n._('admin.anthropic.submit_code')}
				</Button>
			</div>
		</div>
	</div>
{/if}
