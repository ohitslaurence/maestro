<!--
  Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
  SPDX-License-Identifier: Proprietary
-->
<script lang="ts">
	import type { CommitWithDiff } from '$lib/api/repos';
	import { i18n } from '$lib/i18n';
	import { ThreadDivider } from '$lib/ui';

	interface Props {
		commit: CommitWithDiff;
		owner: string;
		repo: string;
	}

	let { commit, owner, repo }: Props = $props();

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
		type: 'context' | 'addition' | 'deletion' | 'header';
		content: string;
		oldLineNum?: number;
		newLineNum?: number;
	}

	const parsedDiff = $derived(parseDiff(commit.diff));

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

	function formatDate(dateStr: string): string {
		return new Date(dateStr).toLocaleString('en-US', {
			year: 'numeric',
			month: 'short',
			day: 'numeric',
			hour: '2-digit',
			minute: '2-digit',
		});
	}

	function getCommitTitle(message: string): string {
		return message.split('\n')[0];
	}

	function getCommitBody(message: string): string | null {
		const lines = message.split('\n');
		if (lines.length <= 1) return null;
		return lines.slice(1).join('\n').trim();
	}
</script>

<div class="commit-diff">
	<div class="commit-info">
		<h2 class="commit-title">{getCommitTitle(commit.message)}</h2>

		{#if getCommitBody(commit.message)}
			<pre class="commit-body">{getCommitBody(commit.message)}</pre>
		{/if}

		<div class="commit-meta">
			<div class="commit-author">
				<span class="author-name">{commit.author_name}</span>
				<span class="author-email">&lt;{commit.author_email}&gt;</span>
			</div>
			<span class="commit-date">{formatDate(commit.author_date)}</span>
		</div>

		<div class="commit-refs">
			<code class="commit-sha">{commit.sha}</code>
			{#if commit.parent_shas.length > 0}
				<span class="parent-refs">
					{commit.parent_shas.length > 1 ? i18n._('client.repos.diff.parents') : i18n._('client.repos.diff.parent')}:
					{#each commit.parent_shas as parent, i}
						<a href="/repos/{owner}/{repo}/commit/{parent}" class="parent-link">
							{parent.slice(0, 7)}
						</a>{i < commit.parent_shas.length - 1 ? ', ' : ''}
					{/each}
				</span>
			{/if}
		</div>
	</div>

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
</div>

<style>
	.commit-diff {
		display: flex;
		flex-direction: column;
		gap: var(--space-6);
	}

	.commit-info {
		background: var(--color-bg-muted);
		border: 1px solid var(--color-border);
		border-radius: var(--radius-md);
		padding: var(--space-4);
	}

	.commit-title {
		font-family: var(--font-mono);
		font-size: var(--text-lg);
		font-weight: 700;
		color: var(--color-fg);
		margin-bottom: var(--space-2);
	}

	.commit-body {
		font-family: var(--font-mono);
		font-size: var(--text-sm);
		color: var(--color-fg-muted);
		white-space: pre-wrap;
		margin-bottom: var(--space-4);
	}

	.commit-meta {
		display: flex;
		align-items: center;
		gap: var(--space-4);
		font-family: var(--font-mono);
		font-size: var(--text-sm);
	}

	.commit-author {
		display: flex;
		align-items: center;
		gap: var(--space-2);
	}

	.author-name {
		font-weight: 500;
		color: var(--color-fg);
	}

	.author-email {
		color: var(--color-fg-muted);
	}

	.commit-date {
		color: var(--color-fg-muted);
	}

	.commit-refs {
		display: flex;
		align-items: center;
		gap: var(--space-4);
		margin-top: var(--space-3);
		font-family: var(--font-mono);
		font-size: var(--text-sm);
	}

	.commit-sha {
		font-family: var(--font-mono);
		font-size: var(--text-xs);
		background: var(--color-bg);
		padding: var(--space-1) var(--space-2);
		border-radius: var(--radius-md);
		color: var(--color-fg);
	}

	.parent-refs {
		color: var(--color-fg-muted);
	}

	.parent-link {
		font-family: var(--font-mono);
		color: var(--color-accent);
	}

	.parent-link:hover {
		text-decoration: underline;
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
</style>
