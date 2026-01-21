<!--
  Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
  SPDX-License-Identifier: Proprietary
-->
<script lang="ts">
	import { page } from '$app/stores';
	import { goto } from '$app/navigation';
	import { i18n } from '$lib/i18n';
	import { getApiClient } from '$lib/api/client';
	import type { Team, TeamMember, OrgMember, TeamRole } from '$lib/api/types';
	import { Card, Button, Input, Badge } from '$lib/ui';

	const orgId = $derived($page.params.orgId ?? '');
	const teamId = $derived($page.params.teamId ?? '');

	let team = $state<Team | null>(null);
	let members = $state<TeamMember[]>([]);
	let orgMembers = $state<OrgMember[]>([]);
	let loading = $state(true);
	let error = $state<string | null>(null);

	let editedName = $state('');
	let saving = $state(false);
	let saveSuccess = $state<string | null>(null);
	let saveError = $state<string | null>(null);

	let deleting = $state(false);
	let showAddMember = $state(false);
	let selectedUserId = $state('');
	let selectedRole = $state<TeamRole>('member');
	let adding = $state(false);
	let removingId = $state<string | null>(null);
	let updatingRoleId = $state<string | null>(null);

	const client = getApiClient();

	const availableMembers = $derived(
		orgMembers.filter((om) => !members.some((m) => m.user_id === om.user_id))
	);

	async function loadData() {
		if (!orgId || !teamId) return;
		loading = true;
		error = null;
		try {
			const [teamData, membersData, orgMembersData] = await Promise.all([
				client.getTeam(orgId, teamId),
				client.listTeamMembers(orgId, teamId),
				client.listOrgMembers(orgId),
			]);
			team = teamData;
			members = membersData.members;
			orgMembers = orgMembersData.members;
			editedName = team.name;
		} catch (e) {
			error = e instanceof Error ? e.message : i18n._('general.error');
		} finally {
			loading = false;
		}
	}

	async function handleSave() {
		if (!team || !orgId || !teamId || editedName === team.name) return;
		saving = true;
		saveSuccess = null;
		saveError = null;
		try {
			team = await client.updateTeam(orgId, teamId, { name: editedName });
			saveSuccess = i18n._('teams.saved');
		} catch (e) {
			saveError = e instanceof Error ? e.message : i18n._('general.error');
		} finally {
			saving = false;
		}
	}

	async function handleDelete() {
		if (!orgId || !teamId || !confirm(i18n._('teams.deleteTeamConfirm'))) return;
		deleting = true;
		try {
			await client.deleteTeam(orgId, teamId);
			goto(`/settings/orgs/${orgId}`);
		} catch (e) {
			saveError = e instanceof Error ? e.message : i18n._('general.error');
			deleting = false;
		}
	}

	async function handleAddMember() {
		if (!selectedUserId || !orgId || !teamId) return;
		adding = true;
		try {
			await client.addTeamMember(orgId, teamId, selectedUserId, selectedRole);
			const membersData = await client.listTeamMembers(orgId, teamId);
			members = membersData.members;
			showAddMember = false;
			selectedUserId = '';
			selectedRole = 'member';
		} catch (e) {
			saveError = e instanceof Error ? e.message : i18n._('general.error');
		} finally {
			adding = false;
		}
	}

	async function handleRemoveMember(userId: string) {
		if (!orgId || !teamId || !confirm(i18n._('teams.removeMemberConfirm'))) return;
		removingId = userId;
		try {
			await client.removeTeamMember(orgId, teamId, userId);
			members = members.filter((m) => m.user_id !== userId);
		} catch (e) {
			saveError = e instanceof Error ? e.message : i18n._('general.error');
		} finally {
			removingId = null;
		}
	}

	async function handleChangeRole(userId: string, newRole: TeamRole) {
		if (!orgId || !teamId) return;
		updatingRoleId = userId;
		try {
			await client.updateTeamMemberRole(orgId, teamId, userId, newRole);
			members = members.map((m) => (m.user_id === userId ? { ...m, role: newRole } : m));
		} catch (e) {
			saveError = e instanceof Error ? e.message : i18n._('general.error');
		} finally {
			updatingRoleId = null;
		}
	}

	function getRoleBadgeVariant(role: TeamRole): 'accent' | 'muted' {
		return role === 'maintainer' ? 'accent' : 'muted';
	}

	$effect(() => {
		loadData();
	});
</script>

<svelte:head>
	<title>{team?.name ?? i18n._('teams.title')} - Loom</title>
</svelte:head>

<div>
	<a
		href="/settings/orgs/{orgId}"
		class="inline-flex items-center text-sm text-fg-muted hover:text-fg mb-4"
	>
		<svg class="w-4 h-4 mr-1" fill="none" stroke="currentColor" viewBox="0 0 24 24">
			<path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 19l-7-7 7-7" />
		</svg>
		{i18n._('teams.backToOrg')}
	</a>

	{#if loading}
		<div class="text-fg-muted">{i18n._('general.loading')}</div>
	{:else if error}
		<Card>
			<div class="text-error">{error}</div>
			<Button variant="secondary" onclick={loadData} class="mt-2">
				{i18n._('general.retry')}
			</Button>
		</Card>
	{:else if team}
		<h1 class="text-2xl font-bold text-fg mb-6">{team.name}</h1>

		<!-- Team Members Section -->
		<section class="mb-8">
			<div class="flex items-center justify-between mb-4">
				<h2 class="text-lg font-semibold text-fg">{i18n._('teams.members')}</h2>
				<Button size="sm" onclick={() => (showAddMember = !showAddMember)}>
					{i18n._('teams.addMember')}
				</Button>
			</div>

			{#if showAddMember}
				<div class="mb-4">
					<Card>
						<div class="flex flex-col sm:flex-row gap-4">
							<div class="flex-1">
								<label for="add-member-select" class="block text-sm font-medium text-fg mb-1.5">
									{i18n._('teams.selectMember')}
								</label>
								{#if availableMembers.length === 0}
									<p class="text-sm text-fg-muted">{i18n._('teams.noAvailableMembers')}</p>
								{:else}
									<select
										id="add-member-select"
										bind:value={selectedUserId}
										class="w-full h-10 px-3 rounded-md border border-border bg-bg text-fg focus:outline-none focus:ring-2 focus:ring-accent"
									>
										<option value="">{i18n._('teams.selectMember')}</option>
										{#each availableMembers as member}
											<option value={member.user_id}>
												{member.display_name}
												{#if member.email}({member.email}){/if}
											</option>
										{/each}
									</select>
								{/if}
							</div>
							<div class="sm:w-40">
								<label for="add-role-select" class="block text-sm font-medium text-fg mb-1.5">
									{i18n._('teams.selectRole')}
								</label>
								<select
									id="add-role-select"
									bind:value={selectedRole}
									class="w-full h-10 px-3 rounded-md border border-border bg-bg text-fg focus:outline-none focus:ring-2 focus:ring-accent"
								>
									<option value="member">{i18n._('teams.role.member')}</option>
									<option value="maintainer">{i18n._('teams.role.maintainer')}</option>
								</select>
							</div>
							<div class="flex items-end gap-2">
								<Button
									onclick={handleAddMember}
									disabled={!selectedUserId || adding}
									loading={adding}
								>
									{adding ? i18n._('teams.adding') : i18n._('teams.add')}
								</Button>
								<Button variant="secondary" onclick={() => (showAddMember = false)}>
									{i18n._('general.cancel')}
								</Button>
							</div>
						</div>
					</Card>
				</div>
			{/if}

			{#if members.length === 0}
				<Card>
					<p class="text-fg-muted">{i18n._('teams.noMembers')}</p>
				</Card>
			{:else}
				<div class="space-y-2">
					{#each members as member (member.user_id)}
						<Card padding="sm">
							<div class="flex items-center justify-between gap-4">
								<div class="flex items-center gap-3 min-w-0">
									{#if member.avatar_url}
										<img
											src={member.avatar_url}
											alt=""
											class="w-10 h-10 rounded-full"
										/>
									{:else}
										<div
											class="w-10 h-10 rounded-full bg-bg-muted flex items-center justify-center text-fg-muted font-medium"
										>
											{member.display_name.charAt(0).toUpperCase()}
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
									<select
										value={member.role}
										onchange={(e) =>
											handleChangeRole(member.user_id, (e.target as HTMLSelectElement).value as TeamRole)}
										disabled={updatingRoleId === member.user_id}
										class="h-8 px-2 text-sm rounded-md border border-border bg-bg text-fg focus:outline-none focus:ring-2 focus:ring-accent disabled:opacity-50"
									>
										<option value="member">{i18n._('teams.role.member')}</option>
										<option value="maintainer">{i18n._('teams.role.maintainer')}</option>
									</select>

									<Badge variant={getRoleBadgeVariant(member.role)} size="sm">
										{member.role === 'maintainer'
											? i18n._('teams.role.maintainer')
											: i18n._('teams.role.member')}
									</Badge>

									<Button
										variant="danger"
										size="sm"
										onclick={() => handleRemoveMember(member.user_id)}
										disabled={removingId === member.user_id}
										loading={removingId === member.user_id}
									>
										{i18n._('teams.removeMember')}
									</Button>
								</div>
							</div>
						</Card>
					{/each}
				</div>
			{/if}
		</section>

		<!-- Team Settings Section -->
		<section>
			<h2 class="text-lg font-semibold text-fg mb-4">{i18n._('teams.settings')}</h2>

			<Card>
				<form
					onsubmit={(e) => {
						e.preventDefault();
						handleSave();
					}}
					class="space-y-6"
				>
					<Input label={i18n._('teams.teamName')} bind:value={editedName} />

					{#if saveSuccess}
						<div class="p-3 rounded-md bg-success/10 text-success text-sm">
							{saveSuccess}
						</div>
					{/if}

					{#if saveError}
						<div class="p-3 rounded-md bg-error/10 text-error text-sm">
							{saveError}
						</div>
					{/if}

					<div class="flex justify-end">
						<Button type="submit" disabled={saving || editedName === team.name} loading={saving}>
							{saving ? i18n._('teams.saving') : i18n._('teams.save')}
						</Button>
					</div>
				</form>
			</Card>

			<div class="mt-4">
				<Card>
					<div class="space-y-4">
						<div>
							<h3 class="text-base font-semibold text-error">{i18n._('teams.deleteTeam')}</h3>
							<p class="text-sm text-fg-muted mt-1">{i18n._('teams.deleteTeamWarning')}</p>
						</div>
						<Button variant="danger" onclick={handleDelete} disabled={deleting} loading={deleting}>
							{deleting ? i18n._('teams.deleting') : i18n._('teams.deleteTeam')}
						</Button>
					</div>
				</Card>
			</div>
		</section>
	{/if}
</div>
