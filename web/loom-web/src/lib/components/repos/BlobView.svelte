<!--
  Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
  SPDX-License-Identifier: Proprietary
-->
<script lang="ts">
	import { i18n } from '$lib/i18n';
	import { Button } from '$lib/ui';

	interface Props {
		content: string;
		path: string;
		owner: string;
		repo: string;
		currentRef: string;
	}

	let { content, path, owner, repo, currentRef }: Props = $props();

	const lines = $derived(content.split('\n'));
	const fileName = $derived(path.split('/').pop() ?? '');
	const extension = $derived(fileName.split('.').pop()?.toLowerCase() ?? '');

	const isImage = $derived(['png', 'jpg', 'jpeg', 'gif', 'svg', 'webp', 'ico'].includes(extension));
	const isBinary = $derived(content.includes('\0') || (content.length > 0 && !/^[\x00-\x7F\u00A0-\u00FF\u0100-\uFFFF\n\r\t]*$/.test(content)));

	const languageClass = $derived(getLanguageClass(extension));

	function getLanguageClass(ext: string): string {
		const langMap: Record<string, string> = {
			js: 'javascript',
			jsx: 'javascript',
			ts: 'typescript',
			tsx: 'typescript',
			py: 'python',
			rb: 'ruby',
			rs: 'rust',
			go: 'go',
			java: 'java',
			c: 'c',
			cpp: 'cpp',
			h: 'c',
			hpp: 'cpp',
			cs: 'csharp',
			php: 'php',
			swift: 'swift',
			kt: 'kotlin',
			scala: 'scala',
			sh: 'bash',
			bash: 'bash',
			zsh: 'bash',
			fish: 'fish',
			ps1: 'powershell',
			sql: 'sql',
			html: 'html',
			htm: 'html',
			css: 'css',
			scss: 'scss',
			sass: 'sass',
			less: 'less',
			json: 'json',
			yaml: 'yaml',
			yml: 'yaml',
			xml: 'xml',
			md: 'markdown',
			mdx: 'markdown',
			toml: 'toml',
			ini: 'ini',
			cfg: 'ini',
			dockerfile: 'dockerfile',
			makefile: 'makefile',
			cmake: 'cmake',
			nix: 'nix',
			svelte: 'svelte',
			vue: 'vue',
			astro: 'astro',
		};
		return langMap[ext] ?? 'plaintext';
	}

	let copied = $state(false);

	async function copyContent() {
		await navigator.clipboard.writeText(content);
		copied = true;
		setTimeout(() => (copied = false), 2000);
	}

	const basePath = $derived(`/repos/${owner}/${repo}`);
</script>

<div class="blob-container">
	<div class="blob-header">
		<div class="blob-header-left">
			<span class="blob-filename">{fileName}</span>
			<span class="blob-meta">{lines.length} {i18n.t('client.repos.blob.lines')}</span>
			{#if !isImage && !isBinary}
				<span class="blob-meta">({new Blob([content]).size} {i18n.t('client.repos.blob.bytes')})</span>
			{/if}
		</div>
		<div class="blob-actions">
			<a href="{basePath}/blame/{currentRef}/{path}">
				<Button variant="ghost" size="sm">{i18n.t('client.repos.blob.blame')}</Button>
			</a>
			{#if !isImage && !isBinary}
				<Button variant="ghost" size="sm" onclick={copyContent}>
					{copied ? i18n.t('client.repos.blob.copied') : i18n.t('client.repos.blob.copy')}
				</Button>
			{/if}
			<a href="/api/repos/{owner}/{repo}/raw/{currentRef}/{path}" target="_blank" rel="noopener">
				<Button variant="ghost" size="sm">{i18n.t('client.repos.blob.raw')}</Button>
			</a>
		</div>
	</div>

	{#if isImage}
		<div class="blob-image-container">
			<img src="/api/repos/{owner}/{repo}/blob/{currentRef}/{path}" alt={fileName} class="blob-image" />
		</div>
	{:else if isBinary}
		<div class="blob-binary-message">
			{i18n.t('client.repos.blob.binary_not_shown')}
		</div>
	{:else}
		<div class="blob-content">
			<table class="blob-table">
				<tbody>
					{#each lines as line, i}
						<tr class="blob-row">
							<td class="blob-line-num">
								<a href="#{i + 1}" id={String(i + 1)} class="line-num-link">{i + 1}</a>
							</td>
							<td class="blob-line-content">
								{line || ' '}
							</td>
						</tr>
					{/each}
				</tbody>
			</table>
		</div>
	{/if}
</div>

<style>
	.blob-container {
		border: 1px solid var(--color-border);
		border-radius: var(--radius-md);
		overflow: hidden;
	}

	.blob-header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: var(--space-2) var(--space-4);
		background: var(--color-bg-muted);
		border-bottom: 1px solid var(--color-border);
	}

	.blob-header-left {
		display: flex;
		align-items: center;
		gap: var(--space-4);
	}

	.blob-filename {
		font-family: var(--font-mono);
		font-size: var(--text-sm);
		font-weight: 500;
		color: var(--color-fg);
	}

	.blob-meta {
		font-family: var(--font-mono);
		font-size: var(--text-xs);
		color: var(--color-fg-muted);
	}

	.blob-actions {
		display: flex;
		align-items: center;
		gap: var(--space-2);
	}

	.blob-image-container {
		display: flex;
		align-items: center;
		justify-content: center;
		padding: var(--space-8);
		background: var(--color-bg);
	}

	.blob-image {
		max-width: 100%;
		max-height: 24rem;
	}

	.blob-binary-message {
		display: flex;
		align-items: center;
		justify-content: center;
		padding: var(--space-8);
		background: var(--color-bg);
		font-family: var(--font-mono);
		color: var(--color-fg-muted);
	}

	.blob-content {
		overflow-x: auto;
	}

	.blob-table {
		width: 100%;
		font-family: var(--font-mono);
		font-size: var(--text-sm);
		border-collapse: collapse;
	}

	.blob-row:hover {
		background: var(--color-bg-muted);
	}

	.blob-line-num {
		width: 3rem;
		padding: 2px var(--space-3);
		text-align: right;
		color: var(--color-fg-muted);
		user-select: none;
		border-right: 1px solid var(--color-border);
		background: var(--color-bg-muted);
		position: sticky;
		left: 0;
	}

	.line-num-link {
		color: inherit;
	}

	.line-num-link:hover {
		color: var(--color-accent);
	}

	.blob-line-content {
		padding: 2px var(--space-4);
		white-space: pre;
		color: var(--color-fg);
	}
</style>
