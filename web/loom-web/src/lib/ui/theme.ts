/**
 * Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
 * SPDX-License-Identifier: Proprietary
 */

import { writable } from 'svelte/store';
import { browser } from '$app/environment';

export type ThemeMode = 'light' | 'dark' | 'system';

const STORAGE_KEY = 'loom-theme';

function getInitialTheme(): ThemeMode {
	if (!browser) return 'dark';

	const stored = localStorage.getItem(STORAGE_KEY);
	if (stored === 'light' || stored === 'dark' || stored === 'system') {
		return stored;
	}
	return 'dark';
}

function getSystemTheme(): 'light' | 'dark' {
	if (!browser) return 'dark';
	return window.matchMedia('(prefers-color-scheme: light)').matches ? 'light' : 'dark';
}

function applyTheme(mode: ThemeMode): void {
	if (!browser) return;

	const effectiveTheme = mode === 'system' ? getSystemTheme() : mode;

	if (effectiveTheme === 'light') {
		document.documentElement.classList.add('light');
	} else {
		document.documentElement.classList.remove('light');
	}
}

function createThemeStore() {
	const { subscribe, set, update } = writable<ThemeMode>(getInitialTheme());

	return {
		subscribe,
		set: (mode: ThemeMode) => {
			if (browser) {
				localStorage.setItem(STORAGE_KEY, mode);
			}
			applyTheme(mode);
			set(mode);
		},
		toggle: () => {
			update((current) => {
				const next = current === 'dark' ? 'light' : current === 'light' ? 'system' : 'dark';
				if (browser) {
					localStorage.setItem(STORAGE_KEY, next);
				}
				applyTheme(next);
				return next;
			});
		},
		init: () => {
			const initial = getInitialTheme();
			applyTheme(initial);

			if (browser) {
				window.matchMedia('(prefers-color-scheme: light)').addEventListener('change', () => {
					const current = getInitialTheme();
					if (current === 'system') {
						applyTheme('system');
					}
				});
			}
		},
	};
}

export const themeStore = createThemeStore();
