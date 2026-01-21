<!--
  Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
  SPDX-License-Identifier: Proprietary
-->
<script lang="ts">
	import { Marked } from 'marked';
	import { markedEmoji } from 'marked-emoji';
	import { nameToEmoji } from 'gemoji';

	interface Props {
		content: string;
	}

	let { content }: Props = $props();

	// GitHub-specific custom emojis (not part of Unicode)
	// These are rendered as images from GitHub's CDN
	const githubCustomEmojis: Record<string, string> = {
		accessibility: 'https://github.githubassets.com/images/icons/emoji/accessibility.png?v8',
		atom: 'https://github.githubassets.com/images/icons/emoji/atom.png?v8',
		basecamp: 'https://github.githubassets.com/images/icons/emoji/basecamp.png?v8',
		basecampy: 'https://github.githubassets.com/images/icons/emoji/basecampy.png?v8',
		bowtie: 'https://github.githubassets.com/images/icons/emoji/bowtie.png?v8',
		copilot: 'https://github.githubassets.com/images/icons/emoji/copilot.png?v8',
		dependabot: 'https://github.githubassets.com/images/icons/emoji/dependabot.png?v8',
		electron: 'https://github.githubassets.com/images/icons/emoji/electron.png?v8',
		feelsgood: 'https://github.githubassets.com/images/icons/emoji/feelsgood.png?v8',
		finnadie: 'https://github.githubassets.com/images/icons/emoji/finnadie.png?v8',
		fishsticks: 'https://github.githubassets.com/images/icons/emoji/fishsticks.png?v8',
		goberserk: 'https://github.githubassets.com/images/icons/emoji/goberserk.png?v8',
		godmode: 'https://github.githubassets.com/images/icons/emoji/godmode.png?v8',
		hurtrealbad: 'https://github.githubassets.com/images/icons/emoji/hurtrealbad.png?v8',
		neckbeard: 'https://github.githubassets.com/images/icons/emoji/neckbeard.png?v8',
		octocat: 'https://github.githubassets.com/images/icons/emoji/octocat.png?v8',
		rage1: 'https://github.githubassets.com/images/icons/emoji/rage1.png?v8',
		rage2: 'https://github.githubassets.com/images/icons/emoji/rage2.png?v8',
		rage3: 'https://github.githubassets.com/images/icons/emoji/rage3.png?v8',
		rage4: 'https://github.githubassets.com/images/icons/emoji/rage4.png?v8',
		shipit: 'https://github.githubassets.com/images/icons/emoji/shipit.png?v8',
		suspect: 'https://github.githubassets.com/images/icons/emoji/suspect.png?v8',
		trollface: 'https://github.githubassets.com/images/icons/emoji/trollface.png?v8'
	};

	// Merge gemoji unicode emojis with GitHub custom emojis
	const allEmojis: Record<string, string> = { ...nameToEmoji, ...githubCustomEmojis };

	const html = $derived.by(() => {
		const markedInstance = new Marked({
			gfm: true,
			breaks: true
		});
		markedInstance.use(
			markedEmoji({
				emojis: allEmojis,
				renderer: (token) => {
					// Check if emoji is a URL (custom GitHub emoji) or unicode
					if (token.emoji.startsWith('https://')) {
						return `<img src="${token.emoji}" alt=":${token.name}:" class="emoji" />`;
					}
					return token.emoji;
				}
			})
		);
		return markedInstance.parse(content) as string;
	});
</script>

<div class="markdown-content prose">
	{@html html}
</div>

<style>
	.markdown-content {
		padding: var(--space-6);
		background: var(--color-bg);
		border: 1px solid var(--color-border);
		border-radius: var(--radius-md);
	}

	.prose :global(h1) {
		font-family: var(--font-mono);
		font-size: var(--text-2xl);
		font-weight: 600;
		color: var(--color-fg);
		margin: 0 0 var(--space-4) 0;
		padding-bottom: var(--space-2);
		border-bottom: 1px solid var(--color-border);
	}

	.prose :global(h2) {
		font-family: var(--font-mono);
		font-size: var(--text-xl);
		font-weight: 600;
		color: var(--color-fg);
		margin: var(--space-6) 0 var(--space-3) 0;
		padding-bottom: var(--space-2);
		border-bottom: 1px solid var(--color-border);
	}

	.prose :global(h3) {
		font-family: var(--font-mono);
		font-size: var(--text-lg);
		font-weight: 600;
		color: var(--color-fg);
		margin: var(--space-5) 0 var(--space-2) 0;
	}

	.prose :global(h4),
	.prose :global(h5),
	.prose :global(h6) {
		font-family: var(--font-mono);
		font-size: var(--text-base);
		font-weight: 600;
		color: var(--color-fg);
		margin: var(--space-4) 0 var(--space-2) 0;
	}

	.prose :global(p) {
		font-family: var(--font-mono);
		font-size: var(--text-sm);
		color: var(--color-fg);
		line-height: 1.7;
		margin: 0 0 var(--space-4) 0;
	}

	.prose :global(ul),
	.prose :global(ol) {
		font-family: var(--font-mono);
		font-size: var(--text-sm);
		color: var(--color-fg);
		line-height: 1.7;
		margin: 0 0 var(--space-4) 0;
		padding-left: var(--space-6);
	}

	.prose :global(li) {
		margin-bottom: var(--space-1);
	}

	.prose :global(li > ul),
	.prose :global(li > ol) {
		margin: var(--space-1) 0;
	}

	.prose :global(pre) {
		background: var(--color-bg-muted);
		border: 1px solid var(--color-border);
		border-radius: var(--radius-md);
		padding: var(--space-4);
		margin: 0 0 var(--space-4) 0;
		overflow-x: auto;
	}

	.prose :global(code) {
		font-family: var(--font-mono);
		font-size: var(--text-xs);
	}

	.prose :global(:not(pre) > code) {
		background: var(--color-bg-muted);
		padding: 2px 6px;
		border-radius: var(--radius-sm);
		color: var(--color-accent);
	}

	.prose :global(a) {
		color: var(--color-accent);
		text-decoration: none;
	}

	.prose :global(a:hover) {
		text-decoration: underline;
	}

	.prose :global(blockquote) {
		border-left: 3px solid var(--color-accent);
		padding-left: var(--space-4);
		margin: 0 0 var(--space-4) 0;
		color: var(--color-fg-muted);
		font-style: italic;
	}

	.prose :global(blockquote p) {
		margin-bottom: 0;
	}

	.prose :global(table) {
		width: 100%;
		border-collapse: collapse;
		margin: 0 0 var(--space-4) 0;
		font-family: var(--font-mono);
		font-size: var(--text-xs);
	}

	.prose :global(th),
	.prose :global(td) {
		border: 1px solid var(--color-border);
		padding: var(--space-2) var(--space-3);
		text-align: left;
	}

	.prose :global(th) {
		background: var(--color-bg-muted);
		font-weight: 600;
	}

	.prose :global(hr) {
		border: none;
		border-top: 1px solid var(--color-border);
		margin: var(--space-6) 0;
	}

	.prose :global(img) {
		max-width: 100%;
		height: auto;
		border-radius: var(--radius-md);
	}

	.prose :global(input[type='checkbox']) {
		margin-right: var(--space-2);
	}

	.prose :global(del) {
		color: var(--color-fg-muted);
	}

	/* Emoji images (GitHub custom emojis) */
	.prose :global(.emoji) {
		height: 1.2em;
		width: 1.2em;
		vertical-align: -0.2em;
		display: inline-block;
		border-radius: 0;
	}
</style>
