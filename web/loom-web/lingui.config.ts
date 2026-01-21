/**
 * Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
 * SPDX-License-Identifier: Proprietary
 */

import type { LinguiConfig } from '@lingui/conf';

const config: LinguiConfig = {
	locales: ['en', 'es', 'ar', 'fr', 'ru', 'ja', 'ko', 'pt', 'sv', 'nl', 'zh-CN', 'he', 'it', 'el', 'et', 'hi', 'bn', 'id'],
	sourceLocale: 'en',
	catalogs: [
		{
			path: '<rootDir>/src/locales/{locale}/messages',
			include: ['src'],
		},
	],
	format: 'po',
	compileNamespace: 'ts',
};

export default config;
