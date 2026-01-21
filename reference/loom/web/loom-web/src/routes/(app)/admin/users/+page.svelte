<!--
  Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
  SPDX-License-Identifier: Proprietary
-->
<script lang="ts">
	import { i18n } from '$lib/i18n';
	import { getApiClient } from '$lib/api/client';
	import type { AdminUser } from '$lib/api/types';
	import { Card, Button, Input, AdminUserCard } from '$lib/ui';
	import { page } from '$app/stores';

	const client = getApiClient();

	let users = $state<AdminUser[]>([]);
	let total = $state(0);
	let loading = $state(true);
	let error = $state<string | null>(null);
	let search = $state('');
	let offset = $state(0);
	const limit = 20;

	let updatingUserId = $state<string | null>(null);
	let impersonatingId = $state<string | null>(null);
	let deletingId = $state<string | null>(null);
	let showDeleteConfirm = $state(false);
	let userToDelete = $state<AdminUser | null>(null);
	let showImpersonateConfirm = $state(false);
	let userToImpersonate = $state<AdminUser | null>(null);
	let impersonateReason = $state('');

	const currentUserId = $derived($page.data.user?.id ?? '');

	async function loadUsers() {
		loading = true;
		error = null;
		try {
			const response = await client.listAdminUsers({ limit, offset, search: search || undefined });
			users = response.users;
			total = response.total;
		} catch (e) {
			error = e instanceof Error ? e.message : i18n._('general.error');
		} finally {
			loading = false;
		}
	}

	async function handleSearch() {
		offset = 0;
		await loadUsers();
	}

	async function handleToggleSystemAdmin(userId: string, isCurrentlyAdmin: boolean) {
		updatingUserId = userId;
		error = null;
		try {
			await client.updateUserRoles(userId, { is_system_admin: !isCurrentlyAdmin });
			// Refresh the user list to get updated data
			await loadUsers();
		} catch (e) {
			if (e instanceof Error) {
				// Try to parse error message from API
				try {
					const parsed = JSON.parse(e.message.replace(/^API Error \d+: /, ''));
					error = parsed.message || e.message;
				} catch {
					error = e.message;
				}
			} else {
				error = i18n._('general.error');
			}
		} finally {
			updatingUserId = null;
		}
	}

	async function handleToggleSupport(userId: string, isCurrentlySupport: boolean) {
		updatingUserId = userId;
		error = null;
		try {
			await client.updateUserRoles(userId, { is_support: !isCurrentlySupport });
			await loadUsers();
		} catch (e) {
			if (e instanceof Error) {
				try {
					const parsed = JSON.parse(e.message.replace(/^API Error \d+: /, ''));
					error = parsed.message || e.message;
				} catch {
					error = e.message;
				}
			} else {
				error = i18n._('general.error');
			}
		} finally {
			updatingUserId = null;
		}
	}

	async function handleToggleAuditor(userId: string, isCurrentlyAuditor: boolean) {
		updatingUserId = userId;
		error = null;
		try {
			await client.updateUserRoles(userId, { is_auditor: !isCurrentlyAuditor });
			await loadUsers();
		} catch (e) {
			if (e instanceof Error) {
				try {
					const parsed = JSON.parse(e.message.replace(/^API Error \d+: /, ''));
					error = parsed.message || e.message;
				} catch {
					error = e.message;
				}
			} else {
				error = i18n._('general.error');
			}
		} finally {
			updatingUserId = null;
		}
	}

	function openImpersonateConfirm(userId: string) {
		const user = users.find(u => u.id === userId);
		if (user) {
			userToImpersonate = user;
			impersonateReason = '';
			showImpersonateConfirm = true;
		}
	}

	function closeImpersonateConfirm() {
		showImpersonateConfirm = false;
		userToImpersonate = null;
		impersonateReason = '';
	}

	async function handleImpersonate() {
		if (!userToImpersonate || !impersonateReason.trim()) return;

		impersonatingId = userToImpersonate.id;
		error = null;
		try {
			await client.startImpersonation(userToImpersonate.id, impersonateReason.trim());
			window.location.href = '/threads';
		} catch (e) {
			error = e instanceof Error ? e.message : i18n._('general.error');
			impersonatingId = null;
		}
	}

	function openDeleteConfirm(userId: string) {
		const user = users.find(u => u.id === userId);
		if (user) {
			userToDelete = user;
			showDeleteConfirm = true;
		}
	}

	function closeDeleteConfirm() {
		showDeleteConfirm = false;
		userToDelete = null;
	}

	async function handleDelete() {
		if (!userToDelete) return;

		deletingId = userToDelete.id;
		error = null;
		try {
			await client.deleteUser(userToDelete.id);
			showDeleteConfirm = false;
			userToDelete = null;
			await loadUsers();
		} catch (e) {
			if (e instanceof Error) {
				try {
					const parsed = JSON.parse(e.message.replace(/^API Error \d+: /, ''));
					error = parsed.message || e.message;
				} catch {
					error = e.message;
				}
			} else {
				error = i18n._('general.error');
			}
		} finally {
			deletingId = null;
		}
	}

	$effect(() => {
		loadUsers();
	});
</script>

<svelte:head>
	<title>{i18n._('admin.users.title')} - Loom</title>
</svelte:head>

<div class="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8 py-8">
	<div class="mb-6">
		<h1 class="text-2xl font-bold text-fg">{i18n._('admin.users.title')}</h1>
		<p class="text-fg-muted">{i18n._('admin.users.description')}</p>
	</div>

	{#if error}
		<div class="mb-4 p-3 rounded-md bg-error/10 text-error text-sm">{error}</div>
	{/if}

	<div class="mb-6">
		<form onsubmit={(e) => { e.preventDefault(); handleSearch(); }} class="flex gap-2">
			<Input
				bind:value={search}
				placeholder={i18n._('admin.users.searchPlaceholder')}
				class="flex-1"
			/>
			<Button type="submit" disabled={loading}>
				{i18n._('general.search')}
			</Button>
		</form>
	</div>

	{#if loading && users.length === 0}
		<Card>
			<div class="text-fg-muted text-center py-8">{i18n._('general.loading')}</div>
		</Card>
	{:else}
		<div class="space-y-3">
			{#each users as user (user.id)}
				<AdminUserCard
					{user}
					{currentUserId}
					onToggleSystemAdmin={handleToggleSystemAdmin}
					onToggleSupport={handleToggleSupport}
					onToggleAuditor={handleToggleAuditor}
					onImpersonate={openImpersonateConfirm}
					onDelete={openDeleteConfirm}
					isUpdating={updatingUserId === user.id}
					isImpersonating={impersonatingId === user.id}
					isDeleting={deletingId === user.id}
				/>
			{/each}
		</div>

		{#if users.length === 0}
			<Card>
				<p class="text-fg-muted text-center py-4">{i18n._('admin.users.empty')}</p>
			</Card>
		{/if}

		{#if total > limit}
			<div class="flex justify-between items-center mt-6">
				<Button
					variant="secondary"
					disabled={offset === 0 || loading}
					onclick={() => { offset = Math.max(0, offset - limit); loadUsers(); }}
				>
					{i18n._('general.previous')}
				</Button>
				<span class="text-sm text-fg-muted">
					{offset + 1} - {Math.min(offset + limit, total)} / {total}
				</span>
				<Button
					variant="secondary"
					disabled={offset + limit >= total || loading}
					onclick={() => { offset = offset + limit; loadUsers(); }}
				>
					{i18n._('general.next')}
				</Button>
			</div>
		{/if}
	{/if}

	{#if showDeleteConfirm && userToDelete}
		<div class="fixed inset-0 bg-black/50 flex items-center justify-center z-50">
			<div class="max-w-md w-full mx-4">
				<Card>
				<h2 class="text-lg font-bold text-fg mb-2">{i18n._('admin.users.deleteConfirmTitle')}</h2>
				<p class="text-fg-muted mb-4">
					{i18n._('admin.users.deleteConfirmMessage')}
				</p>
				<p class="text-sm text-fg mb-4">
					<strong>{userToDelete.display_name}</strong>
					{#if userToDelete.primary_email}
						({userToDelete.primary_email})
					{/if}
				</p>
				<div class="flex gap-2 justify-end">
					<Button variant="secondary" onclick={closeDeleteConfirm} disabled={!!deletingId}>
						{i18n._('admin.users.cancel')}
					</Button>
					<Button variant="danger" onclick={handleDelete} loading={!!deletingId}>
						{i18n._('admin.users.confirm')}
					</Button>
				</div>
				</Card>
			</div>
		</div>
	{/if}

	{#if showImpersonateConfirm && userToImpersonate}
		<div class="fixed inset-0 bg-black/50 flex items-center justify-center z-50">
			<div class="max-w-md w-full mx-4">
				<Card>
				<h2 class="text-lg font-bold text-fg mb-2">{i18n._('admin.users.impersonateConfirmTitle')}</h2>
				<p class="text-fg-muted mb-4">
					{i18n._('admin.users.impersonateConfirmMessage')}
				</p>
				<p class="text-sm text-fg mb-4">
					<strong>{userToImpersonate.display_name}</strong>
					{#if userToImpersonate.primary_email}
						({userToImpersonate.primary_email})
					{/if}
				</p>
				<div class="mb-4">
					<label for="impersonate-reason" class="block text-sm font-medium text-fg mb-1">
						{i18n._('admin.users.impersonateReasonLabel')}
					</label>
					<Input
						id="impersonate-reason"
						bind:value={impersonateReason}
						placeholder={i18n._('admin.users.impersonateReasonPlaceholder')}
					/>
				</div>
				<div class="flex gap-2 justify-end">
					<Button variant="secondary" onclick={closeImpersonateConfirm} disabled={!!impersonatingId}>
						{i18n._('admin.users.cancel')}
					</Button>
					<Button onclick={handleImpersonate} loading={!!impersonatingId} disabled={!impersonateReason.trim()}>
						{i18n._('admin.users.impersonate')}
					</Button>
				</div>
				</Card>
			</div>
		</div>
	{/if}
</div>
