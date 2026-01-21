<!--
  Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
  SPDX-License-Identifier: Proprietary
-->
<script lang="ts">
	import { page } from '$app/stores';
	import { i18n } from '$lib/i18n';
	import type { Snippet } from 'svelte';

	interface Props {
		children: Snippet;
	}

	let { children }: Props = $props();

	const navItems = [
		{ href: '/settings/sessions', label: 'settings.nav.sessions' },
		{ href: '/settings/profile', label: 'settings.nav.profile' },
		{ href: '/settings/orgs', label: 'settings.nav.orgs' },
	];
</script>

<div class="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8 py-8">
	<div class="flex flex-col md:flex-row gap-8">
		<nav class="w-full md:w-48 flex-shrink-0">
			<ul class="space-y-1">
				{#each navItems as item}
					<li>
						<a
							href={item.href}
							class="block px-3 py-2 rounded-md text-sm font-medium transition-colors
								{$page.url.pathname === item.href
									? 'bg-accent text-white'
									: 'text-fg-muted hover:bg-bg-muted hover:text-fg'}"
						>
							{i18n._(item.label)}
						</a>
					</li>
				{/each}
			</ul>
		</nav>

		<div class="flex-1 min-w-0">
			{@render children()}
		</div>
	</div>
</div>
