<!--
  Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
  SPDX-License-Identifier: Proprietary
-->
<script lang="ts">
	import type { CompareResult } from '$lib/api/repos';
	import { i18n } from '$lib/i18n';
	import { ThreadDivider } from '$lib/ui';
	import CommitList from './CommitList.svelte';

	interface Props {
		result: CompareResult;
		owner: string;
		repo: string;
	}

	let { result, owner, repo }: Props = $props();

	interface DiffFile {
		header: string;
		oldPath: string;
		newPath: string;
		hunks: DiffHunk[];
		additions: number;
		deletions: number;
	}

	interface DiffHunk {
		header: string;
		lines: DiffLine[];
	}

	interface DiffLine {
		type: 'context' | 'addition' | 'deletion';
		content: string;
		oldLineNum?: number;
		newLineNum?: number;
	}

	const parsedDiff = $derived(parseDiff(result.diff));

	function parseDiff(diff: string): DiffFile[] {
		const files: DiffFile[] = [];
		const fileChunks = diff.split(/^diff --git /m).filter(Boolean);

		for (const chunk of fileChunks) {
			const lines = chunk.split('\n');
			const headerMatch = lines[0]?.match(/a\/(.+) b\/(.+)/);
			if (!headerMatch) continue;

			const file: DiffFile = {
				header: `diff --git ${lines[0]}`,
				oldPath: headerMatch[1],
				newPath: headerMatch[2],
				hunks: [],
				additions: 0,
				deletions: 0,
			};

			let currentHunk: DiffHunk | null = null;
			let oldLine = 0;
			let newLine = 0;

			for (const line of lines.slice(1)) {
				if (line.startsWith('@@')) {
					if (currentHunk) file.hunks.push(currentHunk);
					const match = line.match(/@@ -(\d+),?\d* \+(\d+),?\d* @@/);
					oldLine = match ? parseInt(match[1]) : 0;
					newLine = match ? parseInt(match[2]) : 0;
					currentHunk = { header: line, lines: [] };
					continue;
				}

				if (!currentHunk) continue;

				if (line.startsWith('+') && !line.startsWith('+++')) {
					currentHunk.lines.push({ type: 'addition', content: line.slice(1), newLineNum: newLine++ });
					file.additions++;
				} else if (line.startsWith('-') && !line.startsWith('---')) {
					currentHunk.lines.push({ type: 'deletion', content: line.slice(1), oldLineNum: oldLine++ });
					file.deletions++;
				} else if (line.startsWith(' ')) {
					currentHunk.lines.push({ type: 'context', content: line.slice(1), oldLineNum: oldLine++, newLineNum: newLine++ });
				}
			}

			if (currentHunk) file.hunks.push(currentHunk);
			files.push(file);
		}

		return files;
	}

	const totalAdditions = $derived(parsedDiff.reduce((sum, f) => sum + f.additions, 0));
	const totalDeletions = $derived(parsedDiff.reduce((sum, f) => sum + f.deletions, 0));
</script>

<div class="compare-view">
	<div class="compare-header">
		<div class="compare-refs">
			<code class="ref-badge">{result.base_ref}</code>
			<svg class="arrow-icon" fill="none" stroke="currentColor" viewBox="0 0 24 24">
				<path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M14 5l7 7m0 0l-7 7m7-7H3" />
			</svg>
			<code class="ref-badge">{result.head_ref}</code>
		</div>

		<div class="compare-stats">
			{#if result.ahead_by > 0}
				<span class="stat-ahead">
					<strong>{result.ahead_by}</strong> {result.ahead_by !== 1 ? i18n._('client.repos.compare.commits') : i18n._('client.repos.compare.commit')} {i18n._('client.repos.compare.ahead')}
				</span>
			{/if}
			{#if result.behind_by > 0}
				<span class="stat-behind">
					<strong>{result.behind_by}</strong> {result.behind_by !== 1 ? i18n._('client.repos.compare.commits') : i18n._('client.repos.compare.commit')} {i18n._('client.repos.compare.behind')}
				</span>
			{/if}
		</div>
	</div>

	{#if result.commits.length > 0}
		<div class="commits-section">
			<h3 class="section-heading">{i18n._('client.repos.compare.commits_heading')}</h3>
			<CommitList commits={result.commits} {owner} {repo} />
		</div>
	{/if}

	<ThreadDivider variant="gradient" />

	<div class="diff-summary">
		<span class="diff-summary-text">
			{i18n._('client.repos.diff.showing')} <strong>{parsedDiff.length}</strong> {parsedDiff.length !== 1 ? i18n._('client.repos.diff.changed_files') : i18n._('client.repos.diff.changed_file')}
		</span>
		<span class="diff-additions">+{totalAdditions}</span>
		<span class="diff-deletions">-{totalDeletions}</span>
	</div>

	{#each parsedDiff as file}
		<div class="diff-file">
			<div class="diff-file-header">
				<span class="diff-file-path">{file.newPath}</span>
				<div class="diff-file-stats">
					<span class="diff-additions">+{file.additions}</span>
					<span class="diff-deletions">-{file.deletions}</span>
				</div>
			</div>

			<div class="diff-content">
				<table class="diff-table">
					<tbody>
						{#each file.hunks as hunk}
							<tr class="hunk-header-row">
								<td colspan="3" class="hunk-header">{hunk.header}</td>
							</tr>
							{#each hunk.lines as line}
								<tr class="diff-line diff-line-{line.type}">
									<td class="diff-line-num">
										{line.oldLineNum ?? ''}
									</td>
									<td class="diff-line-num">
										{line.newLineNum ?? ''}
									</td>
									<td class="diff-line-content">
										<span class="diff-line-prefix">{line.type === 'addition' ? '+' : line.type === 'deletion' ? '-' : ' '}</span>{line.content}
									</td>
								</tr>
							{/each}
						{/each}
					</tbody>
				</table>
			</div>
		</div>
	{/each}

	{#if parsedDiff.length === 0 && result.commits.length === 0}
		<div class="compare-empty">
			{i18n._('client.repos.compare.identical')}
		</div>
	{/if}
</div>

<style>
	.compare-view {
		display: flex;
		flex-direction: column;
		gap: var(--space-6);
	}

	.compare-header {
		background: var(--color-bg-muted);
		border: 1px solid var(--color-border);
		border-radius: var(--radius-md);
		padding: var(--space-4);
	}

	.compare-refs {
		display: flex;
		align-items: center;
		gap: var(--space-2);
		font-family: var(--font-mono);
		font-size: var(--text-lg);
		font-weight: 500;
		color: var(--color-fg);
	}

	.ref-badge {
		font-family: var(--font-mono);
		font-size: var(--text-sm);
		background: var(--color-bg);
		padding: var(--space-1) var(--space-2);
		border-radius: var(--radius-md);
	}

	.arrow-icon {
		width: 1.25rem;
		height: 1.25rem;
		color: var(--color-fg-muted);
	}

	.compare-stats {
		display: flex;
		align-items: center;
		gap: var(--space-4);
		margin-top: var(--space-3);
		font-family: var(--font-mono);
		font-size: var(--text-sm);
		color: var(--color-fg-muted);
	}

	.stat-ahead strong {
		color: var(--color-success);
	}

	.stat-behind strong {
		color: var(--color-error);
	}

	.commits-section {
		display: flex;
		flex-direction: column;
		gap: var(--space-3);
	}

	.section-heading {
		font-family: var(--font-mono);
		font-size: var(--text-lg);
		font-weight: 500;
		color: var(--color-fg);
	}

	.diff-summary {
		display: flex;
		align-items: center;
		gap: var(--space-4);
		font-family: var(--font-mono);
		font-size: var(--text-sm);
	}

	.diff-summary-text {
		color: var(--color-fg-muted);
	}

	.diff-summary-text strong {
		color: var(--color-fg);
	}

	.diff-additions {
		color: var(--color-success);
	}

	.diff-deletions {
		color: var(--color-error);
	}

	.diff-file {
		border: 1px solid var(--color-border);
		border-radius: var(--radius-md);
		overflow: hidden;
	}

	.diff-file-header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: var(--space-2) var(--space-4);
		background: var(--color-bg-muted);
		border-bottom: 1px solid var(--color-border);
	}

	.diff-file-path {
		font-family: var(--font-mono);
		font-size: var(--text-sm);
		color: var(--color-fg);
	}

	.diff-file-stats {
		display: flex;
		align-items: center;
		gap: var(--space-2);
		font-family: var(--font-mono);
		font-size: var(--text-sm);
	}

	.diff-content {
		overflow-x: auto;
	}

	.diff-table {
		width: 100%;
		font-family: var(--font-mono);
		font-size: var(--text-sm);
		border-collapse: collapse;
	}

	.hunk-header-row {
		background: var(--color-accent-soft);
	}

	.hunk-header {
		padding: var(--space-1) var(--space-4);
		font-size: var(--text-xs);
		color: var(--color-fg-muted);
	}

	.diff-line-addition {
		background: var(--color-success-soft);
	}

	.diff-line-deletion {
		background: var(--color-error-soft);
	}

	.diff-line-num {
		width: 3rem;
		padding: 0 var(--space-2);
		text-align: right;
		font-size: var(--text-xs);
		color: var(--color-fg-muted);
		user-select: none;
		border-right: 1px solid var(--color-border);
	}

	.diff-line-content {
		padding: 0 var(--space-4);
		white-space: pre;
	}

	.diff-line-addition .diff-line-content {
		color: var(--color-success);
	}

	.diff-line-deletion .diff-line-content {
		color: var(--color-error);
	}

	.diff-line-context .diff-line-content {
		color: var(--color-fg);
	}

	.diff-line-prefix {
		display: inline-block;
		width: 1rem;
	}

	.compare-empty {
		text-align: center;
		font-family: var(--font-mono);
		color: var(--color-fg-muted);
		padding: var(--space-8);
	}
</style>
