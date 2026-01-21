<!--
 Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
 SPDX-License-Identifier: Proprietary
-->
<script lang="ts">
	import { Button, Input } from '$lib/ui';
	import { i18n } from '$lib/i18n';
	import { getReposClient, type Repository } from '$lib/api/repos';

	interface Props {
		open: boolean;
		onclose: () => void;
		oncreate: (repo: Repository) => void;
		userId: string;
	}

	let { open, onclose, oncreate, userId }: Props = $props();

	let name = $state('');
	let visibility = $state<'private' | 'public'>('private');
	let loading = $state(false);
	let error = $state<string | null>(null);
	let nameError = $state<string | null>(null);

	const NAME_PATTERN = /^[a-zA-Z0-9][a-zA-Z0-9._-]*$/;

	function validateName(value: string): boolean {
		if (!value) {
			nameError = null;
			return false;
		}
		if (!NAME_PATTERN.test(value)) {
			nameError = i18n.t('client.repos.create.name_invalid');
			return false;
		}
		nameError = null;
		return true;
	}

	function handleNameInput() {
		validateName(name);
	}

	async function handleSubmit(e: SubmitEvent) {
		e.preventDefault();

		if (!validateName(name)) {
			if (!name) {
				nameError = i18n.t('client.repos.create.name_invalid');
			}
			return;
		}

		loading = true;
		error = null;

		try {
			const client = getReposClient();
			const repo = await client.createRepo({
				owner_type: 'user',
				owner_id: userId,
				name,
				visibility,
			});
			oncreate(repo);
			resetForm();
		} catch (err) {
			error = i18n.t('client.repos.create.error');
		} finally {
			loading = false;
		}
	}

	function resetForm() {
		name = '';
		visibility = 'private';
		error = null;
		nameError = null;
	}

	function handleClose() {
		resetForm();
		onclose();
	}

	function handleBackdropClick(e: MouseEvent) {
		if (e.target === e.currentTarget) {
			handleClose();
		}
	}

	function handleKeydown(e: KeyboardEvent) {
		if (e.key === 'Escape') {
			handleClose();
		}
	}
</script>

{#if open}
	<div
		class="modal-overlay"
		role="dialog"
		aria-modal="true"
		aria-label={i18n.t('client.repos.create.title')}
		tabindex="-1"
		onclick={handleBackdropClick}
		onkeydown={handleKeydown}
	>
		<div class="modal">
			<div class="modal-header">
				<h2 class="modal-title">{i18n.t('client.repos.create.title')}</h2>
			</div>

			<form onsubmit={handleSubmit}>
				<div class="modal-body">
					<Input
						label={i18n.t('client.repos.create.name')}
						placeholder={i18n.t('client.repos.create.name_placeholder')}
						bind:value={name}
						oninput={handleNameInput}
						error={nameError ?? undefined}
						disabled={loading}
					/>

					<div class="field">
						<label class="field-label" for="visibility">
							{i18n.t('client.repos.create.visibility')}
						</label>
						<select
							id="visibility"
							class="select"
							bind:value={visibility}
							disabled={loading}
						>
							<option value="private">{i18n.t('client.repos.create.private')}</option>
							<option value="public">{i18n.t('client.repos.create.public')}</option>
						</select>
					</div>

					{#if error}
						<p class="error-message">{error}</p>
					{/if}
				</div>

				<div class="modal-footer">
					<Button variant="secondary" onclick={handleClose} disabled={loading}>
						{i18n.t('client.repos.create.cancel')}
					</Button>
					<Button type="submit" variant="primary" {loading} disabled={loading || !name}>
						{i18n.t('client.repos.create.submit')}
					</Button>
				</div>
			</form>
		</div>
	</div>
{/if}

<style>
	.modal-overlay {
		position: fixed;
		inset: 0;
		z-index: 1000;
		display: flex;
		align-items: center;
		justify-content: center;
		background: rgba(0, 0, 0, 0.6);
	}

	.modal {
		position: relative;
		width: 90%;
		max-width: 480px;
		background: var(--color-bg);
		border: 1px solid var(--color-border);
		border-radius: var(--radius-lg);
		box-shadow: var(--shadow-lg);
		overflow: hidden;
	}

	.modal-header {
		padding: var(--space-4);
		border-bottom: 1px solid var(--color-border);
	}

	.modal-title {
		margin: 0;
		font-family: var(--font-mono);
		font-size: var(--text-lg);
		font-weight: 600;
		color: var(--color-fg);
	}

	.modal-body {
		padding: var(--space-4);
		display: flex;
		flex-direction: column;
		gap: var(--space-4);
	}

	.modal-footer {
		display: flex;
		justify-content: flex-end;
		gap: var(--space-3);
		padding: var(--space-4);
		border-top: 1px solid var(--color-border);
		background: var(--color-bg-muted);
	}

	.field {
		width: 100%;
	}

	.field-label {
		display: block;
		font-family: var(--font-mono);
		font-size: var(--text-sm);
		font-weight: 500;
		color: var(--color-fg);
		margin-bottom: var(--space-1);
	}

	.select {
		width: 100%;
		height: 40px;
		padding: 0 var(--space-3);
		font-family: var(--font-mono);
		font-size: var(--text-sm);
		background: var(--color-bg-muted);
		color: var(--color-fg);
		border: 1px solid var(--color-border);
		border-radius: var(--radius-md);
		cursor: pointer;
		transition: border-color 0.15s ease, box-shadow 0.15s ease;
	}

	.select:focus {
		outline: none;
		border-color: var(--color-accent);
		box-shadow: 0 0 0 2px var(--color-accent-soft);
	}

	.select:disabled {
		cursor: not-allowed;
		opacity: 0.5;
	}

	.error-message {
		margin: 0;
		font-family: var(--font-mono);
		font-size: var(--text-sm);
		color: var(--color-error);
	}
</style>
