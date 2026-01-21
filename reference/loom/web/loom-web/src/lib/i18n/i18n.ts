/**
 * Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
 * SPDX-License-Identifier: Proprietary
 */

import { i18n } from '@lingui/core';
import { compileMessage } from '@lingui/message-utils/compileMessage';

// Enable runtime message compilation for uncompiled catalogs.
// This is required because the message catalogs use simple string format
// with ICU placeholders like {time}. Without this, placeholders won't be
// interpolated in production builds.
(i18n as unknown as { setMessagesCompiler: (fn: typeof compileMessage) => void }).setMessagesCompiler(compileMessage);

export type Locale = 'en' | 'es' | 'ar' | 'fr' | 'ru' | 'ja' | 'ko' | 'pt' | 'sv' | 'nl' | 'zh-CN' | 'he' | 'it' | 'el' | 'et' | 'hi' | 'bn' | 'id';

export const locales: Locale[] = ['en', 'es', 'ar', 'fr', 'ru', 'ja', 'ko', 'pt', 'sv', 'nl', 'zh-CN', 'he', 'it', 'el', 'et', 'hi', 'bn', 'id'];
export const defaultLocale: Locale = 'en';

export const localeNames: Record<Locale, string> = {
	en: 'English',
	es: 'Español',
	ar: 'العربية',
	fr: 'Français',
	ru: 'Русский',
	ja: '日本語',
	ko: '한국어',
	pt: 'Português',
	sv: 'Svenska',
	nl: 'Nederlands',
	'zh-CN': '简体中文',
	he: 'עברית',
	it: 'Italiano',
	el: 'Ελληνικά',
	et: 'Eesti',
	hi: 'हिन्दी',
	bn: 'বাংলা',
	id: 'Bahasa Indonesia',
};

export const rtlLocales: Locale[] = ['ar', 'he'];

export function isRtl(locale: Locale): boolean {
	return rtlLocales.includes(locale);
}

export async function loadCatalog(locale: Locale): Promise<void> {
	let messages;

	switch (locale) {
		case 'es':
			messages = (await import('../../locales/es/messages')).messages;
			break;
		case 'ar':
			messages = (await import('../../locales/ar/messages')).messages;
			break;
		case 'fr':
			messages = (await import('../../locales/fr/messages')).messages;
			break;
		case 'ru':
			messages = (await import('../../locales/ru/messages')).messages;
			break;
		case 'ja':
			messages = (await import('../../locales/ja/messages')).messages;
			break;
		case 'ko':
			messages = (await import('../../locales/ko/messages')).messages;
			break;
		case 'pt':
			messages = (await import('../../locales/pt/messages')).messages;
			break;
		case 'sv':
			messages = (await import('../../locales/sv/messages')).messages;
			break;
		case 'nl':
			messages = (await import('../../locales/nl/messages')).messages;
			break;
		case 'zh-CN':
			messages = (await import('../../locales/zh-CN/messages')).messages;
			break;
		case 'he':
			messages = (await import('../../locales/he/messages')).messages;
			break;
		case 'it':
			messages = (await import('../../locales/it/messages')).messages;
			break;
		case 'el':
			messages = (await import('../../locales/el/messages')).messages;
			break;
		case 'et':
			messages = (await import('../../locales/et/messages')).messages;
			break;
		case 'hi':
			messages = (await import('../../locales/hi/messages')).messages;
			break;
		case 'bn':
			messages = (await import('../../locales/bn/messages')).messages;
			break;
		case 'id':
			messages = (await import('../../locales/id/messages')).messages;
			break;
		case 'en':
		default:
			messages = (await import('../../locales/en/messages')).messages;
			break;
	}

	i18n.load(locale, messages);
	i18n.activate(locale);
}

export function getPreferredLocale(): Locale {
	if (typeof window === 'undefined') return defaultLocale;

	const stored = localStorage.getItem('loom-locale');
	if (stored && locales.includes(stored as Locale)) return stored as Locale;

	const browserLang = navigator.language.split('-')[0];
	if (locales.includes(browserLang as Locale)) return browserLang as Locale;
	return 'en';
}

export function setLocale(locale: Locale): void {
	if (typeof window !== 'undefined') {
		localStorage.setItem('loom-locale', locale);
	}
	loadCatalog(locale);
}

export function getCurrentLocale(): Locale {
	return (i18n.locale as Locale) || defaultLocale;
}

export { i18n };
