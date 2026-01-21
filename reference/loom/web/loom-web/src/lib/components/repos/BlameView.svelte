<!--
  Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
  SPDX-License-Identifier: Proprietary
-->
<script lang="ts">
	import type { BlameLine } from '$lib/api/repos';
	import { i18n } from '$lib/i18n';
	import { ThreadDivider } from '$lib/ui';

	interface Props {
		blameLines: BlameLine[];
		path: string;
		owner: string;
		repo: string;
	}

	let { blameLines, path, owner, repo }: Props = $props();

	const basePath = $derived(`/repos/${owner}/${repo}`);
	const fileName = $derived(path.split('/').pop() ?? '');

	interface BlameBlock {
		sha: string;
		authorName: string;
		authorDate: string;
		lines: BlameLine[];
		startLine: number;
	}

	const blameBlocks = $derived.by(() => {
		const blocks: BlameBlock[] = [];
		let currentBlock: BlameBlock | null = null;

		for (const line of blameLines) {
			if (!currentBlock || currentBlock.sha !== line.commit_sha) {
				if (currentBlock) blocks.push(currentBlock);
				currentBlock = {
					sha: line.commit_sha,
					authorName: line.author_name,
					authorDate: line.author_date,
					lines: [line],
					startLine: line.line_number,
				};
			} else {
				currentBlock.lines.push(line);
			}
		}

		if (currentBlock) blocks.push(currentBlock);
		return blocks;
	});

	function formatDate(dateStr: string): string {
		const date = new Date(dateStr);
		const now = new Date();
		const diff = now.getTime() - date.getTime();

		const days = Math.floor(diff / 86400000);
		if (days < 1) return i18n._('client.repos.blame.today');
		if (days < 30) return i18n._('client.repos.blame.days_ago', { count: days });
		if (days < 365) return i18n._('client.repos.blame.months_ago', { count: Math.floor(days / 30) });
		return i18n._('client.repos.blame.years_ago', { count: Math.floor(days / 365) });
	}

	const blockColors = [
		'var(--color-accent-soft)',
		'var(--color-success-soft)',
		'var(--color-warning-soft)',
		'var(--color-error-soft)',
		'var(--color-bg-subtle)',
	];

	function getBlockColor(index: number): string {
		return blockColors[index % blockColors.length];
	}
</script>

<div class="blame-container">
	<div class="blame-header">
		<span class="blame-filename">{fileName}</span>
		<span class="blame-line-count">{blameLines.length} {i18n._('client.repos.blame.lines')}</span>
	</div>

	<div class="blame-content">
		<table class="blame-table">
			<tbody>
				{#each blameBlocks as block, blockIndex}
					{#each block.lines as line, lineIndex}
						<tr class="blame-row" style="--block-color: {getBlockColor(blockIndex)}">
							{#if lineIndex === 0}
								<td
									rowspan={block.lines.length}
									class="blame-commit-cell"
								>
									<div class="blame-commit-info">
										<a
											href="{basePath}/commit/{block.sha}"
											class="blame-sha"
											title={block.sha}
										>
											{block.sha.slice(0, 7)}
										</a>
										<span class="blame-author" title={block.authorName}>{block.authorName}</span>
										<span class="blame-date" title={block.authorDate}>{formatDate(block.authorDate)}</span>
									</div>
								</td>
							{/if}
							<td class="blame-line-num">
								<a href="#{line.line_number}" id={String(line.line_number)} class="line-num-link">
									{line.line_number}
								</a>
							</td>
							<td class="blame-line-content">
								{line.content || ' '}
							</td>
						</tr>
					{/each}
				{/each}
			</tbody>
		</table>
	</div>

	{#if blameLines.length === 0}
		<div class="blame-empty">
			{i18n._('client.repos.blame.empty')}
		</div>
	{/if}
</div>

<style>
	.blame-container {
		border: 1px solid var(--color-border);
		border-radius: var(--radius-md);
		overflow: hidden;
	}

	.blame-header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: var(--space-2) var(--space-4);
		background: var(--color-bg-muted);
		border-bottom: 1px solid var(--color-border);
	}

	.blame-filename {
		font-family: var(--font-mono);
		font-size: var(--text-sm);
		font-weight: 500;
		color: var(--color-fg);
	}

	.blame-line-count {
		font-family: var(--font-mono);
		font-size: var(--text-xs);
		color: var(--color-fg-muted);
	}

	.blame-content {
		overflow-x: auto;
	}

	.blame-table {
		width: 100%;
		font-family: var(--font-mono);
		font-size: var(--text-sm);
		border-collapse: collapse;
	}

	.blame-row {
		background: var(--block-color);
	}

	.blame-row:hover {
		background: var(--color-bg-muted);
	}

	.blame-commit-cell {
		width: 16rem;
		padding: var(--space-1) var(--space-3);
		font-size: var(--text-xs);
		color: var(--color-fg-muted);
		border-right: 1px solid var(--color-border);
		vertical-align: top;
		background: var(--color-bg-muted);
	}

	.blame-commit-info {
		display: flex;
		flex-direction: column;
		gap: 2px;
	}

	.blame-sha {
		font-family: var(--font-mono);
		color: var(--color-accent);
	}

	.blame-sha:hover {
		text-decoration: underline;
	}

	.blame-author {
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.blame-date {
		color: var(--color-fg-subtle);
	}

	.blame-line-num {
		width: 3rem;
		padding: 2px var(--space-3);
		text-align: right;
		color: var(--color-fg-muted);
		user-select: none;
		border-right: 1px solid var(--color-border);
	}

	.line-num-link {
		color: inherit;
	}

	.line-num-link:hover {
		color: var(--color-accent);
	}

	.blame-line-content {
		padding: 2px var(--space-4);
		white-space: pre;
		color: var(--color-fg);
	}

	.blame-empty {
		padding: var(--space-8) var(--space-4);
		text-align: center;
		font-family: var(--font-mono);
		color: var(--color-fg-muted);
	}
</style>
