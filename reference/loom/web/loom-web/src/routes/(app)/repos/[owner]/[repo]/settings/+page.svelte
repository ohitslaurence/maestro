<!--
  Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
  SPDX-License-Identifier: Proprietary
-->
<script lang="ts">
	import { goto } from '$app/navigation';
	import { getReposClient, type Repository } from '$lib/api/repos';
	import { Card, Button, Input, Badge } from '$lib/ui';
	import { i18n } from '$lib/i18n';

	interface Props {
		data: {
			repo: Repository;
		};
	}

	let { data }: Props = $props();

	let repoName = $state('');
	let defaultBranch = $state('');
	let visibility = $state<'private' | 'public'>('private');

	$effect(() => {
		repoName = data.repo.name;
		defaultBranch = data.repo.default_branch;
		visibility = data.repo.visibility;
	});
	let saving = $state(false);
	let deleting = $state(false);
	let showDeleteConfirm = $state(false);
	let deleteConfirmInput = $state('');
	let deleteError = $state('');

	const expectedDeleteConfirm = $derived(`${data.repo.owner_id}/${data.repo.name}`);
	const canDelete = $derived(deleteConfirmInput === expectedDeleteConfirm);

	async function handleDelete() {
		if (!canDelete) return;

		deleting = true;
		deleteError = '';

		try {
			const client = getReposClient();
			await client.deleteRepo(data.repo.id);
			await goto('/repos');
		} catch (err) {
			deleteError = err instanceof Error ? err.message : i18n._('client.repos.settings.delete_error');
			deleting = false;
		}
	}

	function closeDeleteModal() {
		showDeleteConfirm = false;
		deleteConfirmInput = '';
		deleteError = '';
	}
</script>

<svelte:head>
	<title>Settings - {data.repo.owner_id}/{data.repo.name}</title>
</svelte:head>

<div class="space-y-8 max-w-2xl">
	<Card>
		<h2 class="text-lg font-medium text-fg mb-4">{i18n._('client.repos.settings.general')}</h2>

		<div class="space-y-4">
			<Input
				label={i18n._('client.repos.settings.name')}
				bind:value={repoName}
			/>

			<div>
				<label for="default-branch" class="block text-sm font-medium text-fg mb-1.5">
					{i18n._('client.repos.settings.default_branch')}
				</label>
				<select
					id="default-branch"
					bind:value={defaultBranch}
					class="w-full h-10 px-3 rounded-md border border-border bg-bg text-fg"
				>
					<option value={data.repo.default_branch}>{data.repo.default_branch}</option>
				</select>
			</div>

			<fieldset>
				<legend class="block text-sm font-medium text-fg mb-1.5">
					{i18n._('client.repos.settings.visibility')}
				</legend>
				<div class="flex gap-4">
					<label class="flex items-center gap-2 cursor-pointer">
						<input
							type="radio"
							name="visibility"
							value="private"
							bind:group={visibility}
							class="text-accent"
						/>
						<span class="text-sm text-fg">{i18n._('client.repos.settings.private')}</span>
					</label>
					<label class="flex items-center gap-2 cursor-pointer">
						<input
							type="radio"
							name="visibility"
							value="public"
							bind:group={visibility}
							class="text-accent"
						/>
						<span class="text-sm text-fg">{i18n._('client.repos.settings.public')}</span>
					</label>
				</div>
				<p class="text-xs text-fg-muted mt-1">
					{visibility === 'public'
						? i18n._('client.repos.settings.visibility_public_desc')
						: i18n._('client.repos.settings.visibility_private_desc')}
				</p>
			</fieldset>

			<div class="pt-4">
				<Button variant="primary" loading={saving} disabled={saving}>
					{i18n._('client.repos.settings.save')}
				</Button>
			</div>
		</div>
	</Card>

	<Card>
		<h2 class="text-lg font-medium text-fg mb-4">{i18n._('client.repos.settings.clone_url')}</h2>
		<div class="flex gap-2">
			<input
				type="text"
				readonly
				value={data.repo.clone_url}
				class="flex-1 font-mono text-sm bg-bg-muted border border-border rounded px-3 py-2"
			/>
			<Button variant="secondary" onclick={() => navigator.clipboard.writeText(data.repo.clone_url)}>
				{i18n._('client.repos.settings.copy')}
			</Button>
		</div>
	</Card>

	<Card>
		<h2 class="text-lg font-medium text-error mb-4">{i18n._('client.repos.settings.danger_zone')}</h2>

		<div class="border border-error/30 rounded-lg p-4">
			<div class="flex items-start justify-between gap-4">
				<div>
					<h3 class="font-medium text-fg">{i18n._('client.repos.settings.delete_title')}</h3>
					<p class="text-sm text-fg-muted mt-1">
						{i18n._('client.repos.settings.delete_warning')}
					</p>
				</div>
				<Button variant="danger" onclick={() => (showDeleteConfirm = true)}>
					{i18n._('client.repos.settings.delete_button')}
				</Button>
			</div>
		</div>

		{#if showDeleteConfirm}
			<div class="fixed inset-0 bg-black/50 flex items-center justify-center z-50">
				<div class="bg-bg border border-border rounded-lg p-6 max-w-md w-full mx-4">
					<h3 class="text-lg font-medium text-fg mb-2">{i18n._('client.repos.settings.delete_confirm')}</h3>
					<p class="text-sm text-fg-muted mb-4">
						{i18n._('client.repos.settings.delete_confirm_desc')}
					</p>
					<div class="mb-4">
						<label for="delete-confirm-input" class="block text-sm text-fg-muted mb-1.5">
							{i18n._('client.repos.settings.delete_type_name')} <strong class="text-fg">{expectedDeleteConfirm}</strong>
						</label>
						<input
							id="delete-confirm-input"
							type="text"
							bind:value={deleteConfirmInput}
							class="w-full h-10 px-3 rounded-md border border-border bg-bg text-fg font-mono text-sm"
							placeholder={expectedDeleteConfirm}
							autocomplete="off"
						/>
					</div>
					{#if deleteError}
						<p class="text-sm text-error mb-4">{deleteError}</p>
					{/if}
					<div class="flex justify-end gap-2">
						<Button variant="secondary" onclick={closeDeleteModal} disabled={deleting}>
							{i18n._('client.repos.settings.cancel')}
						</Button>
						<Button variant="danger" loading={deleting} disabled={deleting || !canDelete} onclick={handleDelete}>
							{i18n._('client.repos.settings.delete_confirm_button')}
						</Button>
					</div>
				</div>
			</div>
		{/if}
	</Card>
</div>
