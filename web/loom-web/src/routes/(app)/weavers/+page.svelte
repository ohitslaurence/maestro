<!--
  Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
  SPDX-License-Identifier: Proprietary
-->
<script lang="ts">
	import { goto } from '$app/navigation';
	import { i18n } from '$lib/i18n';
	import { getApiClient } from '$lib/api/client';
	import type { Weaver, WeaverStatus, Org } from '$lib/api/types';
	import { Card, Badge, Button, Input, ThreadDivider } from '$lib/ui';

	const client = getApiClient();

	let weavers = $state<Weaver[]>([]);
	let orgs = $state<Org[]>([]);
	let loading = $state(true);
	let error = $state<string | null>(null);
	let deletingId = $state<string | null>(null);

	let showCreateModal = $state(false);
	let creating = $state(false);
	let createdWeaverId = $state<string | null>(null);
	let createLogLines = $state<string[]>([]);
	let createEventSource: EventSource | null = null;
	let showLogsModal = $state(false);
	let logsWeaver = $state<Weaver | null>(null);
	let logLines = $state<string[]>([]);
	let logsError = $state<string | null>(null);
	let logsConnecting = $state(false);
	let logsEventSource: EventSource | null = null;
	const DEFAULT_WEAVER_IMAGE = 'ghcr.io/ghuntley/loom/weaver:latest';
	const PRESET_IMAGES = [
		{ value: 'ghcr.io/ghuntley/loom/weaver:latest', label: 'Loom Weaver (latest)' },
		{ value: 'ghcr.io/ghuntley/loom/claude-code:latest', label: 'Claude Code (latest)' },
		{ value: 'ghcr.io/ghuntley/loom/ampcode:latest', label: 'Ampcode (latest)' },
		{ value: 'nixos/nix:latest', label: 'NixOS (latest)' },
		{ value: 'ubuntu:24.04', label: 'Ubuntu 24.04' },
		{ value: 'debian:bookworm', label: 'Debian Bookworm' },
		{ value: 'alpine:latest', label: 'Alpine (latest)' },
	];

	let newWeaver = $state({
		image: DEFAULT_WEAVER_IMAGE,
		org_id: '',
		lifetime_hours: 24,
		workdir: '',
	});
	let showImageDropdown = $state(false);

	async function loadWeavers() {
		loading = true;
		error = null;
		try {
			const [weaversResponse, orgsResponse] = await Promise.all([
				client.listWeavers(),
				client.listOrgs(),
			]);
			weavers = weaversResponse.weavers;
			orgs = orgsResponse.orgs;
			if (orgs.length > 0 && !newWeaver.org_id) {
				newWeaver.org_id = orgs[0].id;
			}
		} catch (e) {
			error = e instanceof Error ? e.message : i18n._('general.error');
		} finally {
			loading = false;
		}
	}

	async function createWeaver() {
		if (!newWeaver.image) return;
		creating = true;
		error = null;
		createLogLines = [];
		createdWeaverId = null;

		try {
			const weaver = await client.createWeaver(newWeaver);
			createdWeaverId = weaver.id;

			const url = `/api/weaver/${encodeURIComponent(weaver.id)}/logs?tail=100&timestamps=true`;
			createEventSource = new EventSource(url);

			createEventSource.onmessage = (event) => {
				if (event.data && event.data !== 'keep-alive') {
					createLogLines = [...createLogLines, event.data];
				}
			};

			createEventSource.onerror = () => {
				createEventSource?.close();
				createEventSource = null;
			};

			const pollForRunning = async () => {
				const maxAttempts = 60;
				for (let i = 0; i < maxAttempts; i++) {
					try {
						const updated = await client.getWeaver(weaver.id);
						if (updated.status === 'running') {
							cleanupCreateState();
							goto(`/weavers/${weaver.id}`);
							return;
						}
						if (updated.status === 'failed') {
							error = i18n._('weavers.createFailed');
							creating = false;
							createEventSource?.close();
							createEventSource = null;
							return;
						}
					} catch {
						// Ignore polling errors, keep trying
					}
					await new Promise((r) => setTimeout(r, 1000));
				}
				error = i18n._('weavers.createTimeout');
				creating = false;
				createEventSource?.close();
				createEventSource = null;
			};

			pollForRunning();
		} catch (e) {
			error = e instanceof Error ? e.message : i18n._('general.error');
			creating = false;
		}
	}

	function cleanupCreateState() {
		showCreateModal = false;
		creating = false;
		createdWeaverId = null;
		createLogLines = [];
		const defaultOrgId = orgs.length > 0 ? orgs[0].id : '';
		newWeaver = { image: DEFAULT_WEAVER_IMAGE, org_id: defaultOrgId, lifetime_hours: 24, workdir: '' };
		if (createEventSource) {
			createEventSource.close();
			createEventSource = null;
		}
	}

	async function deleteWeaver(id: string) {
		if (!confirm(i18n._('weavers.deleteConfirm'))) return;
		deletingId = id;
		try {
			await client.deleteWeaver(id);
			weavers = weavers.filter((w) => w.id !== id);
		} catch (e) {
			error = e instanceof Error ? e.message : i18n._('general.error');
		} finally {
			deletingId = null;
		}
	}

	function getStatusVariant(status: WeaverStatus): 'success' | 'warning' | 'error' | 'muted' {
		switch (status) {
			case 'running':
				return 'success';
			case 'pending':
				return 'warning';
			case 'failed':
				return 'error';
			case 'succeeded':
			case 'terminating':
			default:
				return 'muted';
		}
	}

	function formatDate(dateStr: string): string {
		return new Date(dateStr).toLocaleString();
	}

	function formatAge(hours: number | undefined): string {
		if (hours === undefined) return '-';
		if (hours < 1) return `${Math.round(hours * 60)}m`;
		return `${hours.toFixed(1)}h`;
	}

	function closeModal() {
		if (creating && createdWeaverId) {
			return;
		}
		showCreateModal = false;
		const defaultOrgId = orgs.length > 0 ? orgs[0].id : '';
		newWeaver = { image: DEFAULT_WEAVER_IMAGE, org_id: defaultOrgId, lifetime_hours: 24, workdir: '' };
		showImageDropdown = false;
		createdWeaverId = null;
		createLogLines = [];
		creating = false;
		if (createEventSource) {
			createEventSource.close();
			createEventSource = null;
		}
	}

	function selectImage(value: string) {
		newWeaver.image = value;
		showImageDropdown = false;
	}

	function openLogsModal(weaver: Weaver) {
		logsWeaver = weaver;
		logLines = [];
		logsError = null;
		logsConnecting = true;
		showLogsModal = true;

		const url = `/api/weaver/${encodeURIComponent(weaver.id)}/logs?tail=500&timestamps=true`;
		logsEventSource = new EventSource(url);

		logsEventSource.onopen = () => {
			logsConnecting = false;
		};

		logsEventSource.onmessage = (event) => {
			logsConnecting = false;
			if (event.data && event.data !== 'keep-alive') {
				logLines = [...logLines, event.data];
			}
		};

		logsEventSource.onerror = () => {
			logsConnecting = false;
			if (logsEventSource?.readyState === EventSource.CLOSED) {
				logsError = i18n._('weavers.logsClosed');
			} else {
				logsError = i18n._('weavers.logsError');
			}
			logsEventSource?.close();
			logsEventSource = null;
		};
	}

	function closeLogsModal() {
		showLogsModal = false;
		logsWeaver = null;
		logLines = [];
		logsError = null;
		logsConnecting = false;
		if (logsEventSource) {
			logsEventSource.close();
			logsEventSource = null;
		}
	}

	$effect(() => {
		loadWeavers();
		return () => {
			if (logsEventSource) {
				logsEventSource.close();
				logsEventSource = null;
			}
			if (createEventSource) {
				createEventSource.close();
				createEventSource = null;
			}
		};
	});
</script>

<svelte:head>
	<title>{i18n._('weavers.title')} - Loom</title>
</svelte:head>

<div class="weavers-page">
	<div class="header">
		<div>
			<h1 class="title">{i18n._('weavers.title')}</h1>
			<p class="subtitle">{i18n._('weavers.subtitle')}</p>
		</div>
		<Button onclick={() => (showCreateModal = true)}>
			{i18n._('weavers.new')}
		</Button>
	</div>

	<ThreadDivider variant="gradient" />

	{#if loading}
		<Card>
			<div class="loading-state">{i18n._('general.loading')}</div>
		</Card>
	{:else if error}
		<Card>
			<div class="error-state">
				<p class="error-text">{error}</p>
				<Button variant="secondary" onclick={loadWeavers}>
					{i18n._('general.retry')}
				</Button>
			</div>
		</Card>
	{:else if weavers.length === 0}
		<Card>
			<div class="empty-state">
				<p class="empty-text">{i18n._('weavers.empty')}</p>
				<Button onclick={() => (showCreateModal = true)}>
					{i18n._('weavers.new')}
				</Button>
			</div>
		</Card>
	{:else}
		<div class="weaver-list">
			{#each weavers as weaver (weaver.id)}
				<Card>
					<div class="weaver-item">
						<div class="weaver-info">
							<div class="weaver-header">
								<span class="weaver-id">{weaver.id}</span>
								<Badge variant={getStatusVariant(weaver.status)} size="sm">
									{weaver.status}
								</Badge>
							</div>
							{#if weaver.image}
								<div class="weaver-image">
									<span class="label">{i18n._('weavers.image')}:</span> {weaver.image}
								</div>
							{/if}
							<div class="weaver-meta">
								<div>
									<span class="label">{i18n._('weavers.created')}:</span>
									{formatDate(weaver.created_at)}
								</div>
								<div>
									<span class="label">{i18n._('weavers.age')}:</span>
									{formatAge(weaver.age_hours)}
								</div>
								{#if weaver.lifetime_hours}
									<div>
										<span class="label">{i18n._('weavers.lifetime')}:</span>
										{weaver.lifetime_hours}h
									</div>
								{/if}
							</div>
							{#if weaver.tags && Object.keys(weaver.tags).length > 0}
								<div class="weaver-tags">
									{#each Object.entries(weaver.tags) as [key, value]}
										<Badge variant="muted" size="sm">{key}: {value}</Badge>
									{/each}
								</div>
							{/if}
						</div>
						<div class="weaver-actions">
							<Button
								variant="secondary"
								size="sm"
								onclick={() => openLogsModal(weaver)}
							>
								{i18n._('weavers.logs')}
							</Button>
							{#if weaver.status === 'running'}
								<a href="/weavers/{weaver.id}">
									<Button variant="primary" size="sm">
										{i18n._('weavers.attach')}
									</Button>
								</a>
							{/if}
							<Button
								variant="danger"
								size="sm"
								disabled={deletingId === weaver.id}
								loading={deletingId === weaver.id}
								onclick={() => deleteWeaver(weaver.id)}
							>
								{i18n._('weavers.delete')}
							</Button>
						</div>
					</div>
				</Card>
			{/each}
		</div>
	{/if}
</div>

{#if showCreateModal}
	<div
		class="modal-overlay"
		role="dialog"
		aria-modal="true"
		aria-labelledby="create-weaver-title"
		tabindex="-1"
		onclick={closeModal}
		onkeydown={(e) => e.key === 'Escape' && closeModal()}
	>
		<div
			class="modal-content"
			class:modal-expanded={createdWeaverId}
			role="document"
			onclick={(e) => e.stopPropagation()}
			onkeydown={(e) => e.stopPropagation()}
		>
			<div class="modal-header">
				<h2 id="create-weaver-title" class="modal-title">
					{createdWeaverId ? i18n._('weavers.creatingTitle') : i18n._('weavers.createTitle')}
				</h2>

				{#if createdWeaverId}
					<div class="creating-status">
						<p class="status-text">{i18n._('weavers.creatingProgress')}</p>
						<p class="status-id">{createdWeaverId}</p>
					</div>
				{:else}
					<form onsubmit={(e) => { e.preventDefault(); createWeaver(); }} class="create-form">
						<div class="form-field">
							<label for="image" class="form-label">
								{i18n._('weavers.imageName')}
							</label>
							<div class="image-input-wrapper">
								<input
									id="image"
									type="text"
									bind:value={newWeaver.image}
									placeholder="ghcr.io/org/image:tag"
									required
									onfocus={() => (showImageDropdown = true)}
									class="form-input"
								/>
								<button
									type="button"
									onclick={() => (showImageDropdown = !showImageDropdown)}
									class="dropdown-toggle"
								>
									<svg class="icon" fill="none" stroke="currentColor" viewBox="0 0 24 24">
										<path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 9l-7 7-7-7" />
									</svg>
								</button>
								{#if showImageDropdown}
									<div class="dropdown-menu">
										{#each PRESET_IMAGES as preset}
											<button
												type="button"
												onclick={() => selectImage(preset.value)}
												class="dropdown-item"
												class:selected={newWeaver.image === preset.value}
											>
												<span class="preset-label">{preset.label}</span>
												<span class="preset-value">{preset.value}</span>
											</button>
										{/each}
									</div>
								{/if}
							</div>
						</div>

						{#if orgs.length > 1}
							<div class="form-field">
								<label for="org" class="form-label">
									{i18n._('weavers.organization')}
								</label>
								<select
									id="org"
									bind:value={newWeaver.org_id}
									class="form-select"
									required
								>
									{#each orgs as org}
										<option value={org.id}>{org.name}</option>
									{/each}
								</select>
							</div>
						{/if}

						<div class="form-field">
							<label for="lifetime" class="form-label">
								{i18n._('weavers.lifetimeLabel')}
							</label>
							<select
								id="lifetime"
								bind:value={newWeaver.lifetime_hours}
								class="form-select"
							>
								<option value={1}>1 {i18n._('weavers.hour')}</option>
								<option value={4}>4 {i18n._('weavers.hours')}</option>
								<option value={8}>8 {i18n._('weavers.hours')}</option>
								<option value={24}>24 {i18n._('weavers.hours')}</option>
								<option value={48}>48 {i18n._('weavers.hours')}</option>
							</select>
						</div>

						<Input
							label={i18n._('weavers.workdir')}
							bind:value={newWeaver.workdir}
							placeholder="/app"
						/>

						<div class="form-actions">
							<Button variant="secondary" type="button" onclick={closeModal}>
								{i18n._('general.cancel')}
							</Button>
							<Button type="submit" disabled={creating || !newWeaver.image} loading={creating}>
								{i18n._('weavers.create')}
							</Button>
						</div>
					</form>
				{/if}
			</div>

			{#if createdWeaverId}
				<div class="log-container">
					{#if createLogLines.length === 0}
						<p class="log-placeholder">{i18n._('weavers.logsConnecting')}</p>
					{:else}
						<pre class="log-output">{createLogLines.join('\n')}</pre>
					{/if}
				</div>
				<div class="modal-footer">
					<p class="footer-text">{i18n._('weavers.creatingWait')}</p>
				</div>
			{/if}
		</div>
	</div>
{/if}

{#if showLogsModal && logsWeaver}
	<div
		class="modal-overlay"
		role="dialog"
		aria-modal="true"
		aria-labelledby="logs-modal-title"
		tabindex="-1"
		onclick={closeLogsModal}
		onkeydown={(e) => e.key === 'Escape' && closeLogsModal()}
	>
		<div
			class="modal-content modal-large"
			role="document"
			onclick={(e) => e.stopPropagation()}
			onkeydown={(e) => e.stopPropagation()}
		>
			<div class="logs-header">
				<div>
					<h2 id="logs-modal-title" class="modal-title">
						{i18n._('weavers.logsTitle')}
					</h2>
					<p class="logs-id">{logsWeaver.id}</p>
				</div>
				<button
					type="button"
					onclick={closeLogsModal}
					class="close-button"
					aria-label={i18n._('general.close')}
				>
					<svg class="icon" fill="none" stroke="currentColor" viewBox="0 0 24 24">
						<path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" />
					</svg>
				</button>
			</div>

			<div class="logs-container">
				{#if logsConnecting}
					<p class="log-placeholder">{i18n._('weavers.logsConnecting')}</p>
				{:else if logsError}
					<p class="log-error">{logsError}</p>
				{:else if logLines.length === 0}
					<p class="log-placeholder">{i18n._('weavers.logsNoData')}</p>
				{:else}
					<pre class="log-output">{logLines.join('\n')}</pre>
				{/if}
			</div>

			<div class="logs-footer">
				<Button variant="secondary" onclick={closeLogsModal}>
					{i18n._('general.close')}
				</Button>
			</div>
		</div>
	</div>
{/if}

<style>
	.weavers-page {
		max-width: 1280px;
		margin: 0 auto;
		padding: var(--space-6) var(--space-4);
		font-family: var(--font-mono);
	}

	.header {
		display: flex;
		align-items: flex-start;
		justify-content: space-between;
		gap: var(--space-4);
	}

	.title {
		font-size: var(--text-xl);
		font-weight: 600;
		color: var(--color-fg);
		margin-bottom: var(--space-1);
	}

	.subtitle {
		font-size: var(--text-sm);
		color: var(--color-fg-muted);
	}

	.loading-state,
	.empty-state,
	.error-state {
		text-align: center;
		padding: var(--space-8);
	}

	.error-text {
		color: var(--color-error);
		margin-bottom: var(--space-4);
	}

	.empty-text {
		color: var(--color-fg-muted);
		margin-bottom: var(--space-4);
	}

	.weaver-list {
		display: flex;
		flex-direction: column;
		gap: var(--space-3);
	}

	.weaver-item {
		display: flex;
		align-items: flex-start;
		justify-content: space-between;
		gap: var(--space-4);
	}

	.weaver-info {
		min-width: 0;
		flex: 1;
	}

	.weaver-header {
		display: flex;
		align-items: center;
		gap: var(--space-2);
		margin-bottom: var(--space-1);
	}

	.weaver-id {
		font-size: var(--text-sm);
		color: var(--color-fg);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.weaver-image {
		font-size: var(--text-sm);
		color: var(--color-fg-muted);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		margin-bottom: var(--space-2);
	}

	.weaver-meta {
		display: flex;
		flex-wrap: wrap;
		gap: var(--space-4);
		font-size: var(--text-xs);
		color: var(--color-fg-muted);
	}

	.weaver-tags {
		display: flex;
		flex-wrap: wrap;
		gap: var(--space-1);
		margin-top: var(--space-2);
	}

	.weaver-actions {
		display: flex;
		gap: var(--space-2);
		flex-shrink: 0;
	}

	.label {
		font-weight: 500;
	}

	.modal-overlay {
		position: fixed;
		inset: 0;
		background: rgba(0, 0, 0, 0.5);
		display: flex;
		align-items: center;
		justify-content: center;
		z-index: 50;
	}

	.modal-content {
		background: var(--color-bg);
		border: 1px solid var(--color-border);
		border-radius: var(--radius-md);
		width: 100%;
		max-width: 512px;
		display: flex;
		flex-direction: column;
		font-family: var(--font-mono);
	}

	.modal-expanded {
		max-height: 80vh;
	}

	.modal-large {
		max-width: 896px;
		max-height: 80vh;
	}

	.modal-header {
		padding: var(--space-6);
	}

	.modal-title {
		font-size: var(--text-lg);
		font-weight: 600;
		color: var(--color-fg);
		margin-bottom: var(--space-4);
	}

	.creating-status {
		margin-bottom: var(--space-4);
	}

	.status-text {
		font-size: var(--text-sm);
		color: var(--color-fg-muted);
		margin-bottom: var(--space-2);
	}

	.status-id {
		font-size: var(--text-xs);
		color: var(--color-fg-muted);
	}

	.create-form {
		display: flex;
		flex-direction: column;
		gap: var(--space-4);
	}

	.form-field {
		width: 100%;
	}

	.form-label {
		display: block;
		font-size: var(--text-sm);
		font-weight: 500;
		color: var(--color-fg);
		margin-bottom: var(--space-2);
	}

	.image-input-wrapper {
		position: relative;
	}

	.form-input {
		width: 100%;
		height: 40px;
		padding: 0 40px 0 var(--space-3);
		border-radius: var(--radius-md);
		border: 1px solid var(--color-border);
		background: var(--color-bg);
		color: var(--color-fg);
		font-family: var(--font-mono);
		font-size: var(--text-sm);
	}

	.form-input::placeholder {
		color: var(--color-fg-subtle);
	}

	.form-input:focus {
		outline: none;
		border-color: var(--color-accent);
		box-shadow: 0 0 0 2px var(--color-accent-soft);
	}

	.form-select {
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

	.dropdown-toggle {
		position: absolute;
		right: 0;
		top: 0;
		height: 40px;
		width: 40px;
		display: flex;
		align-items: center;
		justify-content: center;
		color: var(--color-fg-muted);
		background: transparent;
		border: none;
		cursor: pointer;
	}

	.dropdown-toggle:hover {
		color: var(--color-fg);
	}

	.dropdown-menu {
		position: absolute;
		z-index: 10;
		width: 100%;
		margin-top: var(--space-1);
		background: var(--color-bg);
		border: 1px solid var(--color-border);
		border-radius: var(--radius-md);
		box-shadow: var(--shadow-lg);
		max-height: 240px;
		overflow: auto;
	}

	.dropdown-item {
		width: 100%;
		padding: var(--space-2) var(--space-3);
		text-align: left;
		background: transparent;
		border: none;
		cursor: pointer;
		display: flex;
		flex-direction: column;
	}

	.dropdown-item:hover {
		background: var(--color-bg-muted);
	}

	.dropdown-item.selected {
		background: var(--color-accent-soft);
	}

	.preset-label {
		color: var(--color-fg);
		font-weight: 500;
		font-size: var(--text-sm);
	}

	.preset-value {
		color: var(--color-fg-muted);
		font-size: var(--text-xs);
	}

	.form-actions {
		display: flex;
		justify-content: flex-end;
		gap: var(--space-2);
		padding-top: var(--space-2);
	}

	.log-container {
		flex: 1;
		overflow: auto;
		margin: 0 var(--space-6) var(--space-4);
		padding: var(--space-3);
		background: var(--color-bg-muted);
		border-radius: var(--radius-md);
		min-height: 200px;
		max-height: 300px;
	}

	.log-placeholder {
		font-size: var(--text-sm);
		color: var(--color-fg-muted);
	}

	.log-error {
		font-size: var(--text-sm);
		color: var(--color-error);
	}

	.log-output {
		font-size: var(--text-xs);
		color: var(--color-success);
		white-space: pre-wrap;
		word-break: break-all;
	}

	.modal-footer {
		padding: var(--space-6);
		padding-top: 0;
		border-top: 1px solid var(--color-border);
		margin-top: auto;
	}

	.footer-text {
		font-size: var(--text-xs);
		color: var(--color-fg-muted);
		text-align: center;
	}

	.logs-header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: var(--space-4);
		border-bottom: 1px solid var(--color-border);
	}

	.logs-id {
		font-size: var(--text-sm);
		color: var(--color-fg-muted);
	}

	.close-button {
		color: var(--color-fg-muted);
		background: transparent;
		border: none;
		padding: var(--space-1);
		cursor: pointer;
	}

	.close-button:hover {
		color: var(--color-fg);
	}

	.logs-container {
		flex: 1;
		overflow: auto;
		padding: var(--space-4);
		background: var(--color-bg-muted);
	}

	.logs-footer {
		display: flex;
		justify-content: flex-end;
		padding: var(--space-4);
		border-top: 1px solid var(--color-border);
	}

	.icon {
		width: 16px;
		height: 16px;
	}

	@media (max-width: 640px) {
		.weaver-item {
			flex-direction: column;
		}

		.weaver-actions {
			width: 100%;
			justify-content: flex-end;
		}
	}
</style>
