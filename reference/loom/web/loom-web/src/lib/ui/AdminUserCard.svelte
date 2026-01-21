<!--
  Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
  SPDX-License-Identifier: Proprietary
-->
<script lang="ts">
	import { i18n } from '$lib/i18n';
	import type { AdminUser } from '$lib/api/types';
	import Badge from './Badge.svelte';
	import Button from './Button.svelte';
	import Card from './Card.svelte';
	import ThreadDivider from './ThreadDivider.svelte';

	interface Props {
		user: AdminUser;
		currentUserId: string;
		onToggleSystemAdmin?: (userId: string, currentValue: boolean) => void;
		onToggleSupport?: (userId: string, currentValue: boolean) => void;
		onToggleAuditor?: (userId: string, currentValue: boolean) => void;
		onImpersonate?: (userId: string) => void;
		onDelete?: (userId: string) => void;
		isUpdating?: boolean;
		isImpersonating?: boolean;
		isDeleting?: boolean;
	}

	let {
		user,
		currentUserId,
		onToggleSystemAdmin,
		onToggleSupport,
		onToggleAuditor,
		onImpersonate,
		onDelete,
		isUpdating = false,
		isImpersonating = false,
		isDeleting = false,
	}: Props = $props();

	const isCurrentUser = $derived(user.id === currentUserId);
	const isSystemAdmin = $derived(user.is_system_admin);
	const isSupport = $derived(user.is_support);
	const isAuditor = $derived(user.is_auditor);

	function formatDate(dateStr: string | null): string {
		if (!dateStr) return '-';
		return new Date(dateStr).toLocaleDateString();
	}

	function getRoleBadges(): Array<{ role: string; variant: 'accent' | 'warning' | 'success' | 'muted' }> {
		const badges: Array<{ role: string; variant: 'accent' | 'warning' | 'success' | 'muted' }> = [];
		if (isSystemAdmin) badges.push({ role: 'system_admin', variant: 'accent' });
		if (isSupport) badges.push({ role: 'support', variant: 'warning' });
		if (isAuditor) badges.push({ role: 'auditor', variant: 'success' });
		return badges;
	}
</script>

<Card>
	<div class="user-card">
		<div class="user-info">
			{#if user.avatar_url}
				<img src={user.avatar_url} alt="" class="user-avatar" />
			{:else}
				<div class="user-avatar-placeholder">
					<span>{user.display_name?.charAt(0).toUpperCase() ?? '?'}</span>
				</div>
			{/if}
			<div class="user-details">
				<div class="user-name">
					{user.display_name}
					{#if isCurrentUser}
						<span class="user-you">({i18n._('admin.users.you')})</span>
					{/if}
				</div>
				<div class="user-email">{user.primary_email ?? '-'}</div>
				<div class="user-badges">
					{#each getRoleBadges() as { role, variant }}
						<Badge {variant} size="sm">{role}</Badge>
					{/each}
				</div>
			</div>
		</div>

		<ThreadDivider variant="simple" class="user-divider" />

		<div class="user-actions">
			<div class="user-dates">
				<div>{i18n._('admin.users.created')}: {formatDate(user.created_at)}</div>
				<div>{i18n._('admin.users.updated')}: {formatDate(user.updated_at)}</div>
			</div>
			<div class="user-buttons">
				{#if onToggleSystemAdmin}
					<Button
						variant={isSystemAdmin ? 'danger' : 'secondary'}
						size="sm"
						disabled={isUpdating || isCurrentUser}
						loading={isUpdating}
						onclick={() => onToggleSystemAdmin?.(user.id, isSystemAdmin)}
					>
						{#if isSystemAdmin}
							{i18n._('admin.users.removeAdmin')}
						{:else}
							{i18n._('admin.users.makeAdmin')}
						{/if}
					</Button>
				{/if}
				{#if onToggleSupport}
					<Button
						variant={isSupport ? 'warning' : 'secondary'}
						size="sm"
						disabled={isUpdating || isCurrentUser}
						loading={isUpdating}
						onclick={() => onToggleSupport?.(user.id, isSupport)}
					>
						{#if isSupport}
							{i18n._('admin.users.removeSupport')}
						{:else}
							{i18n._('admin.users.makeSupport')}
						{/if}
					</Button>
				{/if}
				{#if onToggleAuditor}
					<Button
						variant={isAuditor ? 'success' : 'secondary'}
						size="sm"
						disabled={isUpdating || isCurrentUser}
						loading={isUpdating}
						onclick={() => onToggleAuditor?.(user.id, isAuditor)}
					>
						{#if isAuditor}
							{i18n._('admin.users.removeAuditor')}
						{:else}
							{i18n._('admin.users.makeAuditor')}
						{/if}
					</Button>
				{/if}
				{#if onImpersonate}
					<Button
						variant="secondary"
						size="sm"
						disabled={isImpersonating || isCurrentUser}
						loading={isImpersonating}
						onclick={() => onImpersonate?.(user.id)}
					>
						{i18n._('admin.users.impersonate')}
					</Button>
				{/if}
				{#if onDelete}
					<Button
						variant="danger"
						size="sm"
						disabled={isDeleting || isCurrentUser}
						loading={isDeleting}
						onclick={() => onDelete?.(user.id)}
					>
						{i18n._('admin.users.delete')}
					</Button>
				{/if}
			</div>
		</div>
	</div>
</Card>

<style>
	.user-card {
		font-family: var(--font-mono);
	}

	.user-info {
		display: flex;
		align-items: flex-start;
		gap: var(--space-4);
	}

	.user-avatar {
		width: 40px;
		height: 40px;
		border-radius: var(--radius-full);
		flex-shrink: 0;
	}

	.user-avatar-placeholder {
		width: 40px;
		height: 40px;
		border-radius: var(--radius-full);
		background: var(--color-bg-muted);
		display: flex;
		align-items: center;
		justify-content: center;
		flex-shrink: 0;
	}

	.user-avatar-placeholder span {
		font-size: var(--text-sm);
		font-weight: 500;
		color: var(--color-fg-muted);
	}

	.user-details {
		min-width: 0;
		flex: 1;
	}

	.user-name {
		font-weight: 500;
		color: var(--color-fg);
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}

	.user-you {
		font-weight: 400;
		font-size: var(--text-sm);
		color: var(--color-fg-muted);
	}

	.user-email {
		font-size: var(--text-sm);
		color: var(--color-fg-muted);
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}

	.user-badges {
		display: flex;
		flex-wrap: wrap;
		gap: var(--space-1);
		margin-top: var(--space-2);
	}

	.user-card :global(.user-divider) {
		margin: var(--space-4) 0;
	}

	.user-actions {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: var(--space-4);
		flex-wrap: wrap;
	}

	.user-dates {
		font-size: var(--text-sm);
		color: var(--color-fg-muted);
	}

	.user-buttons {
		display: flex;
		gap: var(--space-2);
		flex-wrap: wrap;
	}
</style>
