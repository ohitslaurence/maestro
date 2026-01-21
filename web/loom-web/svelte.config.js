/**
 * Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
 * SPDX-License-Identifier: Proprietary
 */

import adapter from '@sveltejs/adapter-static';
import { vitePreprocess } from '@sveltejs/vite-plugin-svelte';
import { mdsvex } from 'mdsvex';
import remarkGfm from 'remark-gfm';
import rehypeSlug from 'rehype-slug';
import rehypeAutolinkHeadings from 'rehype-autolink-headings';
import rehypePrettyCode from 'rehype-pretty-code';
import { readFileSync } from 'fs';

const threadworkDark = JSON.parse(
	readFileSync('./src/lib/docs/themes/threadwork-dark.json', 'utf8')
);

/** @type {import('mdsvex').MdsvexOptions} */
const mdsvexConfig = {
	extensions: ['.svx'],
	remarkPlugins: [remarkGfm],
	rehypePlugins: [
		rehypeSlug,
		[rehypeAutolinkHeadings, { behavior: 'wrap' }],
		[rehypePrettyCode, { theme: threadworkDark, keepBackground: false }],
	],
};

/** @type {import('@sveltejs/kit').Config} */
const config = {
	extensions: ['.svelte', '.svx'],
	preprocess: [mdsvex(mdsvexConfig), vitePreprocess()],
	kit: {
		adapter: adapter({
			pages: 'build',
			assets: 'build',
			fallback: 'index.html',
			precompress: false,
			strict: false,
		}),
		prerender: {
			entries: [
				'/docs',
				'/docs/tutorials',
				'/docs/tutorials/getting-started',
				'/docs/tutorials/first-thread',
				'/docs/how-to',
				'/docs/how-to/configure-auth',
				'/docs/reference',
				'/docs/reference/cli',
				'/docs/explanation',
				'/docs/explanation/architecture',
			],
		},
		alias: {
			$lib: './src/lib',
			'$lib/*': './src/lib/*',
		},
	},
};

export default config;
