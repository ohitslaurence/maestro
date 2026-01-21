<!--
  Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
  SPDX-License-Identifier: Proprietary
-->
<script lang="ts">
	import { page } from '$app/stores';
	import { i18n, locales, localeNames, setLocale, isRtl, type Locale } from '$lib/i18n';
	import { invalidateAll } from '$app/navigation';
	import { getApiClient } from '$lib/api/client';
	import { authStore } from '$lib/auth';
	import { Card, Button, Input, ThreadDivider } from '$lib/ui';

	const parentData = $derived($page.data as { user: import('$lib/api/types').CurrentUser & { username?: string } });
	const user = $derived(parentData.user);

	let displayName = $state(user?.display_name ?? '');
	let username = $state(user?.username ?? '');
	let usernameError = $state<string | null>(null);
	let selectedLocale = $state<Locale>((user?.locale as Locale) ?? 'en');
	let saving = $state(false);
	let successMessage = $state<string | null>(null);
	let errorMessage = $state<string | null>(null);

	$effect(() => {
		if (user) {
			displayName = user.display_name ?? '';
			username = user.username ?? '';
			selectedLocale = (user.locale as Locale) ?? 'en';
		}
	});

	function validateUsername(value: string): string | null {
		if (value.length > 0 && value.length < 3) {
			return i18n._('settings.profile.usernameTooShort');
		}
		if (value.length > 39) {
			return i18n._('settings.profile.usernameTooLong');
		}
		if (value && !/^[a-zA-Z0-9_]+$/.test(value)) {
			return i18n._('settings.profile.usernameInvalid');
		}
		return null;
	}

	function handleUsernameInput(event: Event) {
		const target = event.target as HTMLInputElement;
		username = target.value;
		usernameError = validateUsername(username);
	}

	const client = getApiClient();

	async function handleSave() {
		if (usernameError) {
			return;
		}

		saving = true;
		successMessage = null;
		errorMessage = null;

		const previousLocale = (user?.locale as Locale) ?? 'en';

		try {
			const updatedUser = await client.updateProfile({
				display_name: displayName,
				username: username || undefined,
				locale: selectedLocale,
			});
			authStore.loginSuccess(updatedUser);

			// Apply locale change
			setLocale(selectedLocale);

			// Hard refresh if locale changed to ensure all translations reload
			if (selectedLocale !== previousLocale) {
				window.location.reload();
				return;
			}

			// No locale change - just invalidate and show success
			if (typeof document !== 'undefined') {
				document.documentElement.dir = isRtl(selectedLocale) ? 'rtl' : 'ltr';
				document.documentElement.lang = selectedLocale;
			}
			await invalidateAll();
			successMessage = i18n._('settings.profile.saved');
		} catch (e) {
			if (e instanceof Error && e.message.includes('taken')) {
				usernameError = i18n._('settings.profile.usernameTaken');
			} else {
				errorMessage = e instanceof Error ? e.message : i18n._('settings.profile.error');
			}
		} finally {
			saving = false;
		}
	}

	function handleLocaleChange(event: Event) {
		const target = event.target as HTMLSelectElement;
		selectedLocale = target.value as Locale;
	}
</script>

<svelte:head>
	<title>{i18n._('settings.profile.title')} - Loom</title>
</svelte:head>

<div class="profile-page">
	<h1 class="page-title">
		{i18n._('settings.profile.title')}
	</h1>

	<ThreadDivider variant="gradient" />

	<Card>
		<form onsubmit={(e) => { e.preventDefault(); handleSave(); }} class="profile-form">
			<Input
				label={i18n._('settings.profile.displayName')}
				bind:value={displayName}
			/>

			<Input
				label={i18n._('settings.profile.username')}
				placeholder={i18n._('settings.profile.usernamePlaceholder')}
				value={username}
				oninput={handleUsernameInput}
				error={usernameError ?? undefined}
			/>
			{#if !usernameError}
				<p class="field-hint">
					{i18n._('settings.profile.usernameHint')}
				</p>
			{/if}

			<div class="form-field">
				<label for="email" class="field-label">
					{i18n._('settings.profile.email')}
				</label>
				<input
					id="email"
					type="email"
					value={user?.email ?? ''}
					disabled
					class="field-input field-disabled"
				/>
				<p class="field-hint">
					{i18n._('settings.profile.emailHint')}
				</p>
			</div>

			<div class="form-field">
				<label for="locale" class="field-label">
					{i18n._('settings.profile.locale')}
				</label>
				<select
					id="locale"
					value={selectedLocale}
					onchange={handleLocaleChange}
					class="field-select"
				>
					{#each locales as locale}
						<option value={locale}>{localeNames[locale]}</option>
					{/each}
				</select>
			</div>

			{#if successMessage}
				<div class="message message-success">
					{successMessage}
				</div>
			{/if}

			{#if errorMessage}
				<div class="message message-error">
					{errorMessage}
				</div>
			{/if}

			<div class="form-actions">
				<Button type="submit" disabled={saving || !!usernameError} loading={saving}>
					{saving ? i18n._('settings.profile.saving') : i18n._('settings.profile.save')}
				</Button>
			</div>
		</form>
	</Card>
</div>

<style>
	.profile-page {
		font-family: var(--font-mono);
	}

	.page-title {
		font-size: var(--text-2xl);
		font-weight: 600;
		color: var(--color-fg);
		margin-bottom: var(--space-4);
	}

	.profile-form {
		display: flex;
		flex-direction: column;
		gap: var(--space-6);
	}

	.form-field {
		width: 100%;
	}

	.field-label {
		display: block;
		font-size: var(--text-sm);
		font-weight: 500;
		color: var(--color-fg);
		margin-bottom: var(--space-2);
	}

	.field-input {
		width: 100%;
		height: 40px;
		padding: 0 var(--space-3);
		border-radius: var(--radius-md);
		border: 1px solid var(--color-border);
		background: var(--color-bg);
		color: var(--color-fg);
		font-family: var(--font-mono);
		font-size: var(--text-sm);
	}

	.field-input:focus {
		outline: none;
		border-color: var(--color-accent);
		box-shadow: 0 0 0 2px var(--color-accent-soft);
	}

	.field-disabled {
		background: var(--color-bg-muted);
		color: var(--color-fg-muted);
		cursor: not-allowed;
		opacity: 0.6;
	}

	.field-select {
		width: 100%;
		height: 40px;
		padding: 0 var(--space-3);
		border-radius: var(--radius-md);
		border: 1px solid var(--color-border);
		background: var(--color-bg);
		color: var(--color-fg);
		font-family: var(--font-mono);
		font-size: var(--text-sm);
	}

	.field-select:focus {
		outline: none;
		border-color: var(--color-accent);
		box-shadow: 0 0 0 2px var(--color-accent-soft);
	}

	.field-hint {
		margin-top: var(--space-2);
		font-size: var(--text-sm);
		color: var(--color-fg-muted);
	}

	.message {
		padding: var(--space-3);
		border-radius: var(--radius-md);
		font-size: var(--text-sm);
	}

	.message-success {
		background: var(--color-success-soft);
		color: var(--color-success);
	}

	.message-error {
		background: var(--color-error-soft);
		color: var(--color-error);
	}

	.form-actions {
		display: flex;
		justify-content: flex-end;
	}
</style>
