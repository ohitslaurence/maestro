<!--
  Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
  SPDX-License-Identifier: Proprietary
-->
<script lang="ts">
	import { goto } from '$app/navigation';
	import { page } from '$app/stores';
	import { getApiClient } from '$lib/api/client';
	import { i18n } from '$lib/i18n';
	import { LoomFrame, ThreadDivider, Button, Input } from '$lib/ui';

	let email = $state('');
	let magicLinkSent = $state(false);
	let isSubmitting = $state(false);
	let oauthLoading = $state<string | null>(null);

	// Check for error in URL query params (e.g., from OAuth callback)
	const urlError = $derived.by(() => {
		const errorCode = $page.url.searchParams.get('error');
		if (errorCode === 'signups_disabled') {
			return i18n._('auth.login.signups_disabled');
		} else if (errorCode === 'internal_error') {
			return i18n._('auth.login.error');
		}
		return null;
	});

	let error = $state<string | null>(null);

	// Combine URL error with form error
	const displayError = $derived(error || urlError);

	const redirectTo = $derived.by(() => {
		const param = $page.url.searchParams.get('redirectTo') ?? '/';
		if (param.startsWith('/') && !param.startsWith('//')) {
			return param;
		}
		return '/';
	});

	async function requestMagicLink(e: SubmitEvent) {
		e.preventDefault();
		if (!email.trim()) return;

		isSubmitting = true;
		error = null;

		try {
			const client = getApiClient();
			await client.requestMagicLink(email);
			magicLinkSent = true;
		} catch (err) {
			error = i18n._('auth.login.error');
		} finally {
			isSubmitting = false;
		}
	}

	async function handleOAuthLogin(provider: string) {
		if (oauthLoading) return;
		oauthLoading = provider;
		error = null;

		try {
			const params = new URLSearchParams();
			params.set('redirect', redirectTo);
			const response = await fetch(`/auth/login/${provider}?${params.toString()}`);
			const data = await response.json();

			if (!response.ok) {
				error = data.message || i18n._('auth.login.error');
				return;
			}

			if (data.redirect_url) {
				window.location.href = data.redirect_url;
			} else {
				error = i18n._('auth.login.error');
			}
		} catch (err) {
			error = i18n._('auth.login.error');
		} finally {
			oauthLoading = null;
		}
	}
</script>

<svelte:head>
	<title>{i18n._('auth.login.title')}</title>
</svelte:head>

<div class="login-container">
	<div class="login-card-wrapper">
		<div class="login-header">
			<h1 class="login-title">{i18n._('auth.login.title')}</h1>
			<p class="login-subtitle">
				{i18n._('auth.login.subtitle')}
			</p>
		</div>

		<LoomFrame variant="full" class="login-card">
			<div class="login-content">
				<div class="oauth-buttons">
					<button
						type="button"
						onclick={() => handleOAuthLogin('github')}
						disabled={oauthLoading !== null}
						class="oauth-btn"
					>
						<svg class="oauth-icon" viewBox="0 0 24 24" fill="currentColor">
							<path d="M12 0C5.374 0 0 5.373 0 12c0 5.302 3.438 9.8 8.207 11.387.599.111.793-.261.793-.577v-2.234c-3.338.726-4.033-1.416-4.033-1.416-.546-1.387-1.333-1.756-1.333-1.756-1.089-.745.083-.729.083-.729 1.205.084 1.839 1.237 1.839 1.237 1.07 1.834 2.807 1.304 3.492.997.107-.775.418-1.305.762-1.604-2.665-.305-5.467-1.334-5.467-5.931 0-1.311.469-2.381 1.236-3.221-.124-.303-.535-1.524.117-3.176 0 0 1.008-.322 3.301 1.23A11.509 11.509 0 0112 5.803c1.02.005 2.047.138 3.006.404 2.291-1.552 3.297-1.23 3.297-1.23.653 1.653.242 2.874.118 3.176.77.84 1.235 1.911 1.235 3.221 0 4.609-2.807 5.624-5.479 5.921.43.372.823 1.102.823 2.222v3.293c0 .319.192.694.801.576C20.566 21.797 24 17.3 24 12c0-6.627-5.373-12-12-12z"/>
						</svg>
						{#if oauthLoading === 'github'}
							{i18n._('auth.login.redirecting')}
						{:else}
							{i18n._('auth.login.github')}
						{/if}
					</button>

					<button
						type="button"
						onclick={() => handleOAuthLogin('google')}
						disabled={oauthLoading !== null}
						class="oauth-btn"
					>
						<svg class="oauth-icon" viewBox="0 0 24 24">
							<path fill="#4285F4" d="M22.56 12.25c0-.78-.07-1.53-.2-2.25H12v4.26h5.92c-.26 1.37-1.04 2.53-2.21 3.31v2.77h3.57c2.08-1.92 3.28-4.74 3.28-8.09z"/>
							<path fill="#34A853" d="M12 23c2.97 0 5.46-.98 7.28-2.66l-3.57-2.77c-.98.66-2.23 1.06-3.71 1.06-2.86 0-5.29-1.93-6.16-4.53H2.18v2.84C3.99 20.53 7.7 23 12 23z"/>
							<path fill="#FBBC05" d="M5.84 14.09c-.22-.66-.35-1.36-.35-2.09s.13-1.43.35-2.09V7.07H2.18C1.43 8.55 1 10.22 1 12s.43 3.45 1.18 4.93l2.85-2.22.81-.62z"/>
							<path fill="#EA4335" d="M12 5.38c1.62 0 3.06.56 4.21 1.64l3.15-3.15C17.45 2.09 14.97 1 12 1 7.7 1 3.99 3.47 2.18 7.07l3.66 2.84c.87-2.6 3.3-4.53 6.16-4.53z"/>
						</svg>
						{#if oauthLoading === 'google'}
							{i18n._('auth.login.redirecting')}
						{:else}
							{i18n._('auth.login.google')}
						{/if}
					</button>
				</div>

				<div class="divider-wrapper">
					<ThreadDivider variant="knot" />
					<span class="divider-text">{i18n._('auth.login.or')}</span>
				</div>

				{#if magicLinkSent}
					<div class="magic-link-sent">
						<div class="success-icon">
							<svg viewBox="0 0 24 24" fill="none" stroke="currentColor">
								<path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M3 8l7.89 5.26a2 2 0 002.22 0L21 8M5 19h14a2 2 0 002-2V7a2 2 0 00-2-2H5a2 2 0 00-2 2v10a2 2 0 002 2z"/>
							</svg>
						</div>
						<h3 class="success-title">{i18n._('auth.login.checkEmail')}</h3>
						<p class="success-message">
							{i18n._('auth.login.magicLinkSent')} <strong>{email}</strong>
						</p>
						<button
							type="button"
							onclick={() => { magicLinkSent = false; email = ''; }}
							class="try-again-link"
						>
							{i18n._('auth.login.useDifferentEmail')}
						</button>
					</div>
				{:else}
					<form onsubmit={requestMagicLink} class="magic-link-form">
						<div class="form-field">
							<label for="email" class="form-label">
								{i18n._('auth.login.emailLabel')}
							</label>
							<input
								type="email"
								id="email"
								bind:value={email}
								required
								disabled={isSubmitting}
								placeholder={i18n._('auth.login.emailPlaceholder')}
								class="form-input"
							/>
						</div>

						{#if displayError}
							<p class="error-text">{displayError}</p>
						{/if}

						<Button
							type="submit"
							variant="primary"
							disabled={isSubmitting || !email.trim()}
							class="submit-btn"
						>
							{#if isSubmitting}
								{i18n._('auth.login.sending')}
							{:else}
								{i18n._('auth.login.sendMagicLink')}
							{/if}
						</Button>
					</form>
				{/if}
			</div>
		</LoomFrame>
	</div>
</div>

<style>
	.login-container {
		min-height: 100vh;
		display: flex;
		align-items: center;
		justify-content: center;
		background: var(--color-bg);
		padding: var(--space-4);
	}

	.login-card-wrapper {
		max-width: 400px;
		width: 100%;
	}

	.login-header {
		text-align: center;
		margin-bottom: var(--space-8);
	}

	.login-title {
		font-size: var(--text-2xl);
		font-weight: 600;
		color: var(--color-fg);
		font-family: var(--font-mono);
	}

	.login-subtitle {
		margin-top: var(--space-2);
		font-size: var(--text-sm);
		color: var(--color-fg-muted);
		font-family: var(--font-mono);
	}

	:global(.login-card) {
		background: var(--color-bg-muted);
		border-radius: var(--radius-md);
	}

	.login-content {
		display: flex;
		flex-direction: column;
		gap: var(--space-6);
	}

	.oauth-buttons {
		display: flex;
		flex-direction: column;
		gap: var(--space-3);
	}

	.oauth-btn {
		display: flex;
		align-items: center;
		justify-content: center;
		gap: var(--space-3);
		padding: var(--space-3) var(--space-4);
		border: 1px solid var(--color-border);
		border-radius: var(--radius-md);
		background: transparent;
		color: var(--color-fg);
		text-decoration: none;
		font-family: var(--font-mono);
		font-size: var(--text-sm);
		cursor: pointer;
		transition: background 0.15s ease, border-color 0.15s ease;
	}

	.oauth-btn:hover:not(:disabled) {
		background: var(--color-bg-subtle);
		border-color: var(--color-border);
	}

	.oauth-btn:disabled {
		opacity: 0.5;
		cursor: not-allowed;
	}

	.oauth-icon {
		width: 20px;
		height: 20px;
	}

	.divider-wrapper {
		position: relative;
	}

	.divider-text {
		position: absolute;
		left: 50%;
		top: 50%;
		transform: translate(-50%, -50%);
		padding: 0 var(--space-3);
		background: var(--color-bg-muted);
		color: var(--color-fg-subtle);
		font-size: var(--text-xs);
		font-family: var(--font-mono);
	}

	.magic-link-sent {
		text-align: center;
		padding: var(--space-4) 0;
	}

	.success-icon {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		width: 48px;
		height: 48px;
		border-radius: var(--radius-full);
		background: var(--color-success-soft);
		margin-bottom: var(--space-4);
	}

	.success-icon svg {
		width: 24px;
		height: 24px;
		color: var(--color-success);
	}

	.success-title {
		font-size: var(--text-lg);
		font-weight: 500;
		color: var(--color-fg);
		font-family: var(--font-mono);
	}

	.success-message {
		margin-top: var(--space-1);
		font-size: var(--text-sm);
		color: var(--color-fg-muted);
		font-family: var(--font-mono);
	}

	.success-message strong {
		color: var(--color-fg);
	}

	.try-again-link {
		margin-top: var(--space-4);
		font-size: var(--text-sm);
		color: var(--color-accent);
		background: transparent;
		border: none;
		cursor: pointer;
		font-family: var(--font-mono);
	}

	.try-again-link:hover {
		text-decoration: underline;
	}

	.magic-link-form {
		display: flex;
		flex-direction: column;
		gap: var(--space-4);
	}

	.form-field {
		display: flex;
		flex-direction: column;
		gap: var(--space-1);
	}

	.form-label {
		font-size: var(--text-sm);
		font-weight: 500;
		color: var(--color-fg-muted);
		font-family: var(--font-mono);
	}

	.form-input {
		width: 100%;
		padding: var(--space-2) var(--space-3);
		border: 1px solid var(--color-border);
		border-radius: var(--radius-md);
		background: var(--color-bg);
		color: var(--color-fg);
		font-family: var(--font-mono);
		font-size: var(--text-sm);
		transition: border-color 0.15s ease;
	}

	.form-input::placeholder {
		color: var(--color-fg-subtle);
	}

	.form-input:focus {
		outline: none;
		border-color: var(--color-accent);
	}

	.form-input:disabled {
		opacity: 0.5;
		cursor: not-allowed;
	}

	.error-text {
		font-size: var(--text-sm);
		color: var(--color-error);
		font-family: var(--font-mono);
	}

	:global(.submit-btn) {
		width: 100%;
	}
</style>
