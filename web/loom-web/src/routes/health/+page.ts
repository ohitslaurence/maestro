/**
 * Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
 * SPDX-License-Identifier: Proprietary
 */

// This route is handled by loom-server, not the SPA.
// Redirect to force a full page load so nginx proxies to the backend.
export const prerender = false;
export const ssr = false;

export function load() {
	if (typeof window !== 'undefined') {
		window.location.href = '/health';
	}
	return {};
}
