/**
 * Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
 * SPDX-License-Identifier: Proprietary
 */

export {
	i18n,
	loadCatalog,
	getPreferredLocale,
	setLocale,
	getCurrentLocale,
	locales,
	localeNames,
	defaultLocale,
	rtlLocales,
	isRtl,
	type Locale,
} from './i18n';
export { default as I18nProvider } from './I18nProvider.svelte';
export { default as LocaleSwitcher } from './LocaleSwitcher.svelte';
