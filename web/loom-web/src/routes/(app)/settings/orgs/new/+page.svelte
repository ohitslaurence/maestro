<!--
  Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
  SPDX-License-Identifier: Proprietary
-->
<script lang="ts">
	import { goto } from '$app/navigation';
	import { i18n } from '$lib/i18n';
	import { getApiClient } from '$lib/api/client';
	import type { OrgVisibility } from '$lib/api/types';
	import { Card, Button, Input } from '$lib/ui';

	let name = $state('');
	let slug = $state('');
	let visibility = $state<OrgVisibility>('public');
	let saving = $state(false);
	let error = $state<string | null>(null);
	let slugManuallyEdited = $state(false);

	const client = getApiClient();

	function generateSlug(name: string): string {
		return name
			.toLowerCase()
			.replace(/[^a-z0-9]+/g, '-')
			.replace(/^-+|-+$/g, '')
			.substring(0, 50);
	}

	function handleNameChange(event: Event) {
		const target = event.target as HTMLInputElement;
		name = target.value;
		if (!slugManuallyEdited) {
			slug = generateSlug(name);
		}
	}

	function handleSlugChange(event: Event) {
		const target = event.target as HTMLInputElement;
		slug = target.value;
		slugManuallyEdited = true;
	}

	function handleVisibilityChange(event: Event) {
		const target = event.target as HTMLSelectElement;
		visibility = target.value as OrgVisibility;
	}

	async function handleSubmit() {
		if (!name.trim() || !slug.trim()) {
			error = i18n._('settings.orgs.new.requiredFields');
			return;
		}

		saving = true;
		error = null;

		try {
			const org = await client.createOrg({
				name: name.trim(),
				slug: slug.trim(),
				visibility,
			});
			goto(`/settings/orgs/${org.id}`);
		} catch (e) {
			if (e instanceof Error) {
				try {
					const parsed = JSON.parse(e.message.replace(/^API Error \d+: /, ''));
					error = parsed.message || e.message;
				} catch {
					error = e.message;
				}
			} else {
				error = i18n._('settings.orgs.new.error');
			}
		} finally {
			saving = false;
		}
	}
</script>

<svelte:head>
	<title>{i18n._('settings.orgs.new.title')} - Loom</title>
</svelte:head>

<div>
	<h1 class="text-2xl font-bold text-fg mb-6">
		{i18n._('settings.orgs.new.title')}
	</h1>

	<Card>
		<form onsubmit={(e) => { e.preventDefault(); handleSubmit(); }} class="space-y-6">
			<div class="w-full">
				<label for="name" class="block text-sm font-medium text-fg mb-1.5">
					{i18n._('settings.orgs.new.name')}
				</label>
				<input
					id="name"
					type="text"
					value={name}
					oninput={handleNameChange}
					placeholder={i18n._('settings.orgs.new.namePlaceholder')}
					class="w-full h-10 px-3 rounded-md border border-border bg-bg text-fg
								 focus:outline-none focus:ring-2 focus:ring-accent focus:ring-offset-2 focus:ring-offset-bg"
				/>
			</div>

			<div class="w-full">
				<label for="slug" class="block text-sm font-medium text-fg mb-1.5">
					{i18n._('settings.orgs.new.slug')}
				</label>
				<input
					id="slug"
					type="text"
					value={slug}
					oninput={handleSlugChange}
					placeholder={i18n._('settings.orgs.new.slugPlaceholder')}
					class="w-full h-10 px-3 rounded-md border border-border bg-bg text-fg font-mono
								 focus:outline-none focus:ring-2 focus:ring-accent focus:ring-offset-2 focus:ring-offset-bg"
				/>
				<p class="mt-1.5 text-sm text-fg-muted">
					{i18n._('settings.orgs.new.slugHint')}
				</p>
			</div>

			<div class="w-full">
				<label for="visibility" class="block text-sm font-medium text-fg mb-1.5">
					{i18n._('settings.orgs.new.visibility')}
				</label>
				<select
					id="visibility"
					value={visibility}
					onchange={handleVisibilityChange}
					class="w-full h-10 px-3 rounded-md border border-border bg-bg text-fg
								 focus:outline-none focus:ring-2 focus:ring-accent focus:ring-offset-2 focus:ring-offset-bg"
				>
					<option value="public">{i18n._('settings.orgs.visibility.public')}</option>
					<option value="unlisted">{i18n._('settings.orgs.visibility.unlisted')}</option>
					<option value="private">{i18n._('settings.orgs.visibility.private')}</option>
				</select>
			</div>

			{#if error}
				<div class="p-3 rounded-md bg-error/10 text-error text-sm">
					{error}
				</div>
			{/if}

			<div class="flex justify-end gap-3">
				<a href="/settings/orgs">
					<Button type="button" variant="secondary">
						{i18n._('general.cancel')}
					</Button>
				</a>
				<Button type="submit" disabled={saving} loading={saving}>
					{saving ? i18n._('settings.orgs.new.creating') : i18n._('settings.orgs.new.create')}
				</Button>
			</div>
		</form>
	</Card>
</div>
