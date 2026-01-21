<!--
 Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
 SPDX-License-Identifier: Proprietary
-->
<script lang="ts">
	interface Props {
		id: string;
		rows?: number;
		cols?: number;
		autoplay?: boolean;
		loop?: boolean;
		speed?: number;
		startAt?: number;
		preload?: boolean;
	}

	let {
		id,
		rows = 24,
		cols = 80,
		autoplay = false,
		loop = false,
		speed = 1,
		startAt = 0,
		preload = true,
	}: Props = $props();

	const src = $derived(
		`https://asciinema.org/a/${id}/iframe?` +
			new URLSearchParams({
				autoplay: autoplay ? '1' : '0',
				loop: loop ? '1' : '0',
				speed: speed.toString(),
				t: startAt.toString(),
				preload: preload ? '1' : '0',
				rows: rows.toString(),
				cols: cols.toString(),
			}).toString()
	);
</script>

<div class="asciinema-container">
	<iframe
		{src}
		title="Terminal recording"
		allowfullscreen
		loading="lazy"
		style="width: 100%; height: {rows * 1.2 + 4}em;"
	></iframe>
</div>

<style>
	.asciinema-container {
		margin: var(--space-4) 0;
		border-radius: var(--radius-md);
		overflow: hidden;
		background: var(--color-bg-muted);
		border: 1px solid var(--color-border);
	}

	.asciinema-container iframe {
		border: none;
		display: block;
	}
</style>
