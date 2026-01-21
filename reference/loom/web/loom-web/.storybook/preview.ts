/**
 * Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
 * SPDX-License-Identifier: Proprietary
 */

import type { Preview } from '@storybook/svelte';
import '../src/app.css';

const preview: Preview = {
	parameters: {
		controls: {
			matchers: {
				color: /(background|color)$/i,
				date: /Date$/i,
			},
		},
		backgrounds: {
			default: 'Loom Black',
			values: [
				{ name: 'Loom Black', value: '#0D0C0B' },
				{ name: 'Raw Linen', value: '#F7F4F0' },
			],
		},
	},
	globalTypes: {
		theme: {
			name: 'Theme',
			description: 'Threadwork theme switcher',
			defaultValue: 'dark',
			toolbar: {
				icon: 'paintbrush',
				items: [
					{ value: 'dark', icon: 'moon', title: 'Dark (Loom Black)' },
					{ value: 'light', icon: 'sun', title: 'Light (Raw Linen)' },
				],
				dynamicTitle: true,
			},
		},
	},
	decorators: [
		(Story, context) => {
			const theme = context.globals.theme || 'dark';
			const isLight = theme === 'light';

			if (typeof document !== 'undefined') {
				document.documentElement.classList.toggle('light', isLight);
				document.body.style.backgroundColor = isLight ? '#F7F4F0' : '#0D0C0B';
			}

			return Story();
		},
	],
};

export default preview;
