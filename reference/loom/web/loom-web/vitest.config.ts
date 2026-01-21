/**
 * Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
 * SPDX-License-Identifier: Proprietary
 */

import { defineConfig } from 'vitest/config';
import { svelte } from '@sveltejs/vite-plugin-svelte';
import path from 'path';

export default defineConfig({
	plugins: [
		svelte({
			hot: !process.env.VITEST,
			compilerOptions: {
				dev: true,
			},
		}),
	],
	resolve: {
		conditions: ['browser'],
		alias: {
			$lib: path.resolve(__dirname, './src/lib'),
			'$app/navigation': path.resolve(__dirname, './src/test/mocks/app-navigation.ts'),
			'$app/stores': path.resolve(__dirname, './src/test/mocks/app-stores.ts'),
			'$app/environment': path.resolve(__dirname, './src/test/mocks/app-environment.ts'),
		},
	},
	test: {
		include: ['src/**/*.test.ts'],
		globals: true,
		environment: 'jsdom',
		setupFiles: ['src/test/setup.ts'],
	},
});
