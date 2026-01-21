<!--
  Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
  SPDX-License-Identifier: Proprietary
-->
<script lang="ts">
	import { page } from '$app/stores';
	import { getApiClient } from '$lib/api/client';
	import { i18n } from '$lib/i18n';

	let userCode = $state($page.url.searchParams.get('code') ?? '');
	let isSubmitting = $state(false);
	let result = $state<'idle' | 'success' | 'error'>('idle');
	let errorMessage = $state('');

	async function submitCode(e: SubmitEvent) {
		e.preventDefault();
		if (!userCode.trim()) return;

		isSubmitting = true;
		result = 'idle';
		errorMessage = '';

		try {
			const client = getApiClient();
			await client.completeDeviceCode(userCode.trim());
			result = 'success';
		} catch (err) {
			result = 'error';
			errorMessage = i18n._('auth.device.error');
		} finally {
			isSubmitting = false;
		}
	}

	function formatUserCode(value: string): string {
		const cleaned = value.replace(/[^a-zA-Z0-9]/g, '').toUpperCase();
		const parts = [];
		for (let i = 0; i < cleaned.length && i < 9; i += 3) {
			parts.push(cleaned.slice(i, i + 3));
		}
		return parts.join('-');
	}

	function handleInput(e: Event) {
		const input = e.target as HTMLInputElement;
		userCode = formatUserCode(input.value);
	}
</script>

<svelte:head>
	<title>{i18n._('auth.device.title')} - Loom</title>
</svelte:head>

<div class="min-h-screen flex items-center justify-center bg-gray-50 dark:bg-gray-900 px-4">
	<div class="max-w-md w-full space-y-8">
		<div class="text-center">
			<div class="inline-flex items-center justify-center w-16 h-16 rounded-full bg-blue-100 dark:bg-blue-900 mb-4">
				<svg class="w-8 h-8 text-blue-600 dark:text-blue-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
					<path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M8 9l3 3-3 3m5 0h3M5 20h14a2 2 0 002-2V6a2 2 0 00-2-2H5a2 2 0 00-2 2v12a2 2 0 002 2z"/>
				</svg>
			</div>
			<h1 class="text-3xl font-bold text-gray-900 dark:text-white">{i18n._('auth.device.title')}</h1>
			<p class="mt-2 text-sm text-gray-600 dark:text-gray-400">
				{i18n._('auth.device.subtitle')}
			</p>
		</div>

		<div class="bg-white dark:bg-gray-800 rounded-lg shadow-sm p-8">
			{#if result === 'success'}
				<div class="text-center py-4">
					<div class="inline-flex items-center justify-center w-12 h-12 rounded-full bg-green-100 dark:bg-green-900 mb-4">
						<svg class="w-6 h-6 text-green-600 dark:text-green-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
							<path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M5 13l4 4L19 7"/>
						</svg>
					</div>
					<h3 class="text-lg font-medium text-gray-900 dark:text-white">{i18n._('auth.device.success')}</h3>
					<p class="mt-1 text-sm text-gray-600 dark:text-gray-400">
						{i18n._('auth.device.successMessage')}
					</p>
				</div>
			{:else}
				<form onsubmit={submitCode} class="space-y-6">
					<div>
						<label for="userCode" class="block text-sm font-medium text-gray-700 dark:text-gray-300 text-center mb-2">
							{i18n._('auth.device.inputLabel')}
						</label>
						<input
							type="text"
							id="userCode"
							value={userCode}
							oninput={handleInput}
							required
							disabled={isSubmitting}
							placeholder={i18n._('auth.device.placeholder')}
							autocomplete="off"
							spellcheck="false"
							class="block w-full px-4 py-4 text-center text-2xl font-mono tracking-widest border border-gray-300 dark:border-gray-600 rounded-lg shadow-sm bg-white dark:bg-gray-700 text-gray-900 dark:text-white placeholder-gray-400 focus:outline-none focus:ring-2 focus:ring-blue-500 focus:border-blue-500 disabled:opacity-50 uppercase"
						/>
					</div>

					{#if result === 'error'}
						<p class="text-sm text-red-600 dark:text-red-400 text-center">{errorMessage}</p>
					{/if}

					<button
						type="submit"
						disabled={isSubmitting || userCode.length < 11}
						class="w-full flex justify-center py-3 px-4 border border-transparent rounded-lg shadow-sm text-sm font-medium text-white bg-blue-600 hover:bg-blue-700 focus:outline-none focus:ring-2 focus:ring-offset-2 focus:ring-blue-500 disabled:opacity-50 disabled:cursor-not-allowed transition-colors"
					>
						{#if isSubmitting}
							{i18n._('auth.device.authorizing')}
						{:else}
							{i18n._('auth.device.authorize')}
						{/if}
					</button>
				</form>
			{/if}
		</div>

		<p class="text-center text-xs text-gray-500 dark:text-gray-400">
			{i18n._('auth.device.noCode')} <code class="bg-gray-100 dark:bg-gray-800 px-1 py-0.5 rounded">{i18n._('auth.device.cliCommand')}</code> {i18n._('auth.device.inTerminal')}
		</p>
	</div>
</div>
