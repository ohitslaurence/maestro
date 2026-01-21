<!--
  Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
  SPDX-License-Identifier: Proprietary
-->
<script lang="ts">
	import { page } from '$app/stores';
	import { goto } from '$app/navigation';
	import { i18n } from '$lib/i18n';
	import { getApiClient } from '$lib/api/client';
	import type {
		Org,
		OrgMember,
		Team,
		ApiKey,
		OrgRole,
		OrgVisibility,
		OrgJoinPolicy,
		CreateApiKeyResponse,
	} from '$lib/api/types';
	import { Card, Badge, Button, Input } from '$lib/ui';

	const orgId = $derived($page.params.orgId!);
	const client = getApiClient();

	type TabId = 'members' | 'teams' | 'api-keys' | 'settings';

	let activeTab = $state<TabId>('members');
	let org = $state<Org | null>(null);
	let members = $state<OrgMember[]>([]);
	let teams = $state<Team[]>([]);
	let apiKeys = $state<ApiKey[]>([]);

	let loading = $state(true);
	let error = $state<string | null>(null);

	let showAddMemberModal = $state(false);
	let showCreateTeamModal = $state(false);
	let showCreateApiKeyModal = $state(false);
	let createdApiKey = $state<CreateApiKeyResponse | null>(null);

	let newMemberEmail = $state('');
	let newTeamName = $state('');
	let newTeamSlug = $state('');
	let newApiKeyName = $state('');
	let newApiKeyScopes = $state<string[]>(['read']);

	let editOrgName = $state('');
	let editOrgVisibility = $state<OrgVisibility>('private');
	let editOrgJoinPolicy = $state<OrgJoinPolicy>('invite_only');
	let saving = $state(false);
	let saveSuccess = $state(false);

	let processingId = $state<string | null>(null);

	const tabs: { id: TabId; label: string }[] = [
		{ id: 'members', label: 'org.tabs.members' },
		{ id: 'teams', label: 'org.tabs.teams' },
		{ id: 'api-keys', label: 'org.tabs.apiKeys' },
		{ id: 'settings', label: 'org.tabs.settings' },
	];

	async function loadOrg() {
		loading = true;
		error = null;
		try {
			org = await client.getOrg(orgId);
			editOrgName = org.name;
			editOrgVisibility = org.visibility;
			editOrgJoinPolicy = org.join_policy;
		} catch (e) {
			error = e instanceof Error ? e.message : i18n._('general.error');
		} finally {
			loading = false;
		}
	}

	async function loadMembers() {
		try {
			const response = await client.listOrgMembers(orgId);
			members = response.members;
		} catch (e) {
			error = e instanceof Error ? e.message : i18n._('general.error');
		}
	}

	async function loadTeams() {
		try {
			const response = await client.listTeams(orgId);
			teams = response.teams;
		} catch (e) {
			error = e instanceof Error ? e.message : i18n._('general.error');
		}
	}

	async function loadApiKeys() {
		try {
			const response = await client.listApiKeys(orgId);
			apiKeys = response.api_keys;
		} catch (e) {
			error = e instanceof Error ? e.message : i18n._('general.error');
		}
	}

	async function handleTabChange(tabId: TabId) {
		activeTab = tabId;
		if (tabId === 'members' && members.length === 0) await loadMembers();
		if (tabId === 'teams' && teams.length === 0) await loadTeams();
		if (tabId === 'api-keys' && apiKeys.length === 0) await loadApiKeys();
	}

	async function updateMemberRole(userId: string, role: OrgRole) {
		processingId = userId;
		try {
			await client.updateOrgMemberRole(orgId, userId, role);
			members = members.map((m) => (m.user_id === userId ? { ...m, role } : m));
		} catch (e) {
			error = e instanceof Error ? e.message : i18n._('general.error');
		} finally {
			processingId = null;
		}
	}

	async function removeMember(userId: string) {
		if (!confirm(i18n._('org.members.removeConfirm'))) return;
		processingId = userId;
		try {
			await client.removeOrgMember(orgId, userId);
			members = members.filter((m) => m.user_id !== userId);
		} catch (e) {
			error = e instanceof Error ? e.message : i18n._('general.error');
		} finally {
			processingId = null;
		}
	}

	async function createTeam() {
		if (!newTeamName || !newTeamSlug) return;
		try {
			const team = await client.createTeam(orgId, { name: newTeamName, slug: newTeamSlug });
			teams = [...teams, team];
			showCreateTeamModal = false;
			newTeamName = '';
			newTeamSlug = '';
		} catch (e) {
			error = e instanceof Error ? e.message : i18n._('general.error');
		}
	}

	async function createApiKey() {
		if (!newApiKeyName) return;
		try {
			const key = await client.createApiKey(orgId, { name: newApiKeyName, scopes: newApiKeyScopes });
			createdApiKey = key;
			apiKeys = [...apiKeys, { id: key.id, name: key.name, prefix: key.prefix, scopes: key.scopes, created_at: key.created_at, last_used_at: null, created_by: '' }];
		} catch (e) {
			error = e instanceof Error ? e.message : i18n._('general.error');
		}
	}

	async function revokeApiKey(keyId: string) {
		if (!confirm(i18n._('org.apiKeys.revokeConfirm'))) return;
		processingId = keyId;
		try {
			await client.revokeApiKey(orgId, keyId);
			apiKeys = apiKeys.filter((k) => k.id !== keyId);
		} catch (e) {
			error = e instanceof Error ? e.message : i18n._('general.error');
		} finally {
			processingId = null;
		}
	}

	async function saveOrgSettings() {
		saving = true;
		saveSuccess = false;
		try {
			org = await client.updateOrg(orgId, {
				name: editOrgName,
				visibility: editOrgVisibility,
				join_policy: editOrgJoinPolicy,
			});
			saveSuccess = true;
		} catch (e) {
			error = e instanceof Error ? e.message : i18n._('general.error');
		} finally {
			saving = false;
		}
	}

	async function deleteOrg() {
		if (!confirm(i18n._('org.settings.deleteConfirm'))) return;
		try {
			await client.deleteOrg(orgId);
			goto('/settings/orgs');
		} catch (e) {
			error = e instanceof Error ? e.message : i18n._('general.error');
		}
	}

	function getRoleVariant(role: OrgRole): 'accent' | 'success' | 'muted' {
		switch (role) {
			case 'owner': return 'accent';
			case 'admin': return 'success';
			default: return 'muted';
		}
	}

	function formatDate(dateStr: string): string {
		return new Date(dateStr).toLocaleDateString();
	}

	function closeApiKeyModal() {
		showCreateApiKeyModal = false;
		createdApiKey = null;
		newApiKeyName = '';
		newApiKeyScopes = ['read'];
	}

	$effect(() => {
		loadOrg();
		loadMembers();
	});
</script>

<svelte:head>
	<title>{org?.name ?? i18n._('org.title')} - Loom</title>
</svelte:head>

{#if loading}
	<div class="text-fg-muted">{i18n._('general.loading')}</div>
{:else if error && !org}
	<Card>
		<div class="text-error">{error}</div>
		<Button variant="secondary" onclick={loadOrg} class="mt-2">
			{i18n._('general.retry')}
		</Button>
	</Card>
{:else if org}
	<div class="mb-6">
		<h1 class="text-2xl font-bold text-fg">{org.name}</h1>
		<p class="text-fg-muted">{org.slug}</p>
	</div>

	{#if error}
		<div class="mb-4 p-3 rounded-md bg-error/10 text-error text-sm">{error}</div>
	{/if}

	<div class="border-b border-border mb-6">
		<nav class="flex gap-4">
			{#each tabs as tab}
				<button
					class="px-3 py-2 text-sm font-medium border-b-2 transition-colors -mb-px
						{activeTab === tab.id
							? 'border-accent text-accent'
							: 'border-transparent text-fg-muted hover:text-fg hover:border-border'}"
					onclick={() => handleTabChange(tab.id)}
				>
					{i18n._(tab.label)}
				</button>
			{/each}
		</nav>
	</div>

	{#if activeTab === 'members'}
		<div class="space-y-4">
			<div class="flex justify-end">
				<Button onclick={() => (showAddMemberModal = true)}>
					{i18n._('org.members.add')}
				</Button>
			</div>

			{#each members as member (member.user_id)}
				<Card>
					<div class="flex items-center justify-between gap-4">
						<div class="flex items-center gap-3 min-w-0">
							{#if member.avatar_url}
								<img src={member.avatar_url} alt="" class="w-10 h-10 rounded-full" />
							{:else}
								<div class="w-10 h-10 rounded-full bg-bg-muted flex items-center justify-center text-fg-muted">
									{member.display_name?.charAt(0).toUpperCase() ?? '?'}
								</div>
							{/if}
							<div class="min-w-0">
								<div class="font-medium text-fg truncate">{member.display_name}</div>
								{#if member.email}
									<div class="text-sm text-fg-muted truncate">{member.email}</div>
								{/if}
							</div>
						</div>

						<div class="flex items-center gap-3">
							<Badge variant={getRoleVariant(member.role)} size="sm">
								{i18n._(`org.roles.${member.role}`)}
							</Badge>

							{#if member.role !== 'owner'}
								<select
									class="h-8 px-2 rounded-md border border-border bg-bg text-fg text-sm"
									value={member.role}
									onchange={(e) => updateMemberRole(member.user_id, (e.target as HTMLSelectElement).value as OrgRole)}
									disabled={processingId === member.user_id}
								>
									<option value="member">{i18n._('org.roles.member')}</option>
									<option value="admin">{i18n._('org.roles.admin')}</option>
								</select>

								<Button
									variant="danger"
									size="sm"
									disabled={processingId === member.user_id}
									loading={processingId === member.user_id}
									onclick={() => removeMember(member.user_id)}
								>
									{i18n._('org.members.remove')}
								</Button>
							{/if}
						</div>
					</div>
				</Card>
			{/each}

			{#if members.length === 0}
				<Card>
					<p class="text-fg-muted text-center">{i18n._('org.members.empty')}</p>
				</Card>
			{/if}
		</div>
	{/if}

	{#if activeTab === 'teams'}
		<div class="space-y-4">
			<div class="flex justify-end">
				<Button onclick={() => (showCreateTeamModal = true)}>
					{i18n._('org.teams.create')}
				</Button>
			</div>

			{#each teams as team (team.id)}
				<Card hover>
					<a href="/settings/orgs/{orgId}/teams/{team.id}" class="block">
						<div class="flex items-center justify-between">
							<div>
								<div class="font-medium text-fg">{team.name}</div>
								<div class="text-sm text-fg-muted">{team.slug}</div>
							</div>
							<div class="text-sm text-fg-muted">
								{team.member_count} {i18n._('org.teams.memberCount')}
							</div>
						</div>
					</a>
				</Card>
			{/each}

			{#if teams.length === 0}
				<Card>
					<p class="text-fg-muted text-center">{i18n._('org.teams.empty')}</p>
				</Card>
			{/if}
		</div>
	{/if}

	{#if activeTab === 'api-keys'}
		<div class="space-y-4">
			<div class="flex justify-end">
				<Button onclick={() => (showCreateApiKeyModal = true)}>
					{i18n._('org.apiKeys.create')}
				</Button>
			</div>

			{#each apiKeys as key (key.id)}
				<Card>
					<div class="flex items-center justify-between gap-4">
						<div class="min-w-0">
							<div class="font-medium text-fg">{key.name}</div>
							<div class="text-sm text-fg-muted font-mono">{key.prefix}...</div>
							<div class="flex gap-2 mt-1">
								{#each key.scopes as scope}
									<Badge variant="muted" size="sm">{scope}</Badge>
								{/each}
							</div>
							{#if key.last_used_at}
								<div class="text-xs text-fg-muted mt-1">
									{i18n._('org.apiKeys.lastUsed')}: {formatDate(key.last_used_at)}
								</div>
							{/if}
						</div>
						<Button
							variant="danger"
							size="sm"
							disabled={processingId === key.id}
							loading={processingId === key.id}
							onclick={() => revokeApiKey(key.id)}
						>
							{i18n._('org.apiKeys.revoke')}
						</Button>
					</div>
				</Card>
			{/each}

			{#if apiKeys.length === 0}
				<Card>
					<p class="text-fg-muted text-center">{i18n._('org.apiKeys.empty')}</p>
				</Card>
			{/if}
		</div>
	{/if}

	{#if activeTab === 'settings'}
		<Card>
			<form onsubmit={(e) => { e.preventDefault(); saveOrgSettings(); }} class="space-y-6">
				<Input
					label={i18n._('org.settings.name')}
					bind:value={editOrgName}
				/>

				<div class="w-full">
					<label for="visibility" class="block text-sm font-medium text-fg mb-1.5">
						{i18n._('org.settings.visibility')}
					</label>
					<select
						id="visibility"
						bind:value={editOrgVisibility}
						class="w-full h-10 px-3 rounded-md border border-border bg-bg text-fg"
					>
						<option value="public">{i18n._('org.visibility.public')}</option>
						<option value="private">{i18n._('org.visibility.private')}</option>
					</select>
				</div>

				<div class="w-full">
					<label for="joinPolicy" class="block text-sm font-medium text-fg mb-1.5">
						{i18n._('org.settings.joinPolicy')}
					</label>
					<select
						id="joinPolicy"
						bind:value={editOrgJoinPolicy}
						class="w-full h-10 px-3 rounded-md border border-border bg-bg text-fg"
					>
						<option value="open">{i18n._('org.joinPolicy.open')}</option>
						<option value="request">{i18n._('org.joinPolicy.request')}</option>
						<option value="invite_only">{i18n._('org.joinPolicy.inviteOnly')}</option>
					</select>
				</div>

				{#if saveSuccess}
					<div class="p-3 rounded-md bg-success/10 text-success text-sm">
						{i18n._('org.settings.saved')}
					</div>
				{/if}

				<div class="flex justify-end">
					<Button type="submit" disabled={saving} loading={saving}>
						{i18n._('general.save')}
					</Button>
				</div>
			</form>
		</Card>

		<Card>
			<div class="space-y-4">
				<h3 class="text-lg font-medium text-error">{i18n._('org.settings.dangerZone')}</h3>
				<p class="text-sm text-fg-muted">{i18n._('org.settings.deleteWarning')}</p>
				<Button variant="danger" onclick={deleteOrg}>
					{i18n._('org.settings.delete')}
				</Button>
			</div>
		</Card>
	{/if}
{/if}

{#if showAddMemberModal}
	<div class="fixed inset-0 bg-black/50 flex items-center justify-center z-50" role="dialog" aria-modal="true" onclick={() => (showAddMemberModal = false)} onkeydown={(e) => e.key === 'Escape' && (showAddMemberModal = false)}>
		<div class="bg-bg border border-border rounded-lg p-6 w-full max-w-md" role="document" onclick={(e) => e.stopPropagation()} onkeydown={(e) => e.stopPropagation()}>
			<h2 class="text-lg font-bold text-fg mb-4">{i18n._('org.members.addTitle')}</h2>
			<Input
				label={i18n._('org.members.email')}
				type="email"
				bind:value={newMemberEmail}
				placeholder="user@example.com"
			/>
			<div class="flex justify-end gap-2 mt-4">
				<Button variant="secondary" onclick={() => (showAddMemberModal = false)}>
					{i18n._('general.cancel')}
				</Button>
				<Button onclick={() => { showAddMemberModal = false; }}>
					{i18n._('org.members.invite')}
				</Button>
			</div>
		</div>
	</div>
{/if}

{#if showCreateTeamModal}
	<div class="fixed inset-0 bg-black/50 flex items-center justify-center z-50" role="dialog" aria-modal="true" onclick={() => (showCreateTeamModal = false)} onkeydown={(e) => e.key === 'Escape' && (showCreateTeamModal = false)}>
		<div class="bg-bg border border-border rounded-lg p-6 w-full max-w-md" role="document" onclick={(e) => e.stopPropagation()} onkeydown={(e) => e.stopPropagation()}>
			<h2 class="text-lg font-bold text-fg mb-4">{i18n._('org.teams.createTitle')}</h2>
			<div class="space-y-4">
				<Input
					label={i18n._('org.teams.name')}
					bind:value={newTeamName}
				/>
				<Input
					label={i18n._('org.teams.slug')}
					bind:value={newTeamSlug}
				/>
			</div>
			<div class="flex justify-end gap-2 mt-4">
				<Button variant="secondary" onclick={() => (showCreateTeamModal = false)}>
					{i18n._('general.cancel')}
				</Button>
				<Button onclick={createTeam}>
					{i18n._('org.teams.create')}
				</Button>
			</div>
		</div>
	</div>
{/if}

{#if showCreateApiKeyModal}
	<div class="fixed inset-0 bg-black/50 flex items-center justify-center z-50" role="dialog" aria-modal="true" onclick={closeApiKeyModal} onkeydown={(e) => e.key === 'Escape' && closeApiKeyModal()}>
		<div class="bg-bg border border-border rounded-lg p-6 w-full max-w-md" role="document" onclick={(e) => e.stopPropagation()} onkeydown={(e) => e.stopPropagation()}>
			{#if createdApiKey}
				<h2 class="text-lg font-bold text-fg mb-4">{i18n._('org.apiKeys.created')}</h2>
				<p class="text-sm text-fg-muted mb-2">{i18n._('org.apiKeys.copyWarning')}</p>
				<div class="p-3 bg-bg-muted rounded-md font-mono text-sm break-all">
					{createdApiKey.key}
				</div>
				<div class="flex justify-end mt-4">
					<Button onclick={closeApiKeyModal}>
						{i18n._('general.done')}
					</Button>
				</div>
			{:else}
				<h2 class="text-lg font-bold text-fg mb-4">{i18n._('org.apiKeys.createTitle')}</h2>
				<div class="space-y-4">
					<Input
						label={i18n._('org.apiKeys.name')}
						bind:value={newApiKeyName}
					/>
					<fieldset class="w-full">
						<legend class="block text-sm font-medium text-fg mb-1.5">
							{i18n._('org.apiKeys.scopes')}
						</legend>
						<div class="space-y-2">
							<label class="flex items-center gap-2">
								<input type="checkbox" checked={newApiKeyScopes.includes('read')} onchange={() => {
									if (newApiKeyScopes.includes('read')) {
										newApiKeyScopes = newApiKeyScopes.filter(s => s !== 'read');
									} else {
										newApiKeyScopes = [...newApiKeyScopes, 'read'];
									}
								}} />
								<span class="text-sm text-fg">read</span>
							</label>
							<label class="flex items-center gap-2">
								<input type="checkbox" checked={newApiKeyScopes.includes('write')} onchange={() => {
									if (newApiKeyScopes.includes('write')) {
										newApiKeyScopes = newApiKeyScopes.filter(s => s !== 'write');
									} else {
										newApiKeyScopes = [...newApiKeyScopes, 'write'];
									}
								}} />
								<span class="text-sm text-fg">write</span>
							</label>
						</div>
					</fieldset>
				</div>
				<div class="flex justify-end gap-2 mt-4">
					<Button variant="secondary" onclick={closeApiKeyModal}>
						{i18n._('general.cancel')}
					</Button>
					<Button onclick={createApiKey}>
						{i18n._('org.apiKeys.create')}
					</Button>
				</div>
			{/if}
		</div>
	</div>
{/if}
