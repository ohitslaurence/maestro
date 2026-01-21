<!--
  Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
  SPDX-License-Identifier: Proprietary
-->

<script lang="ts">
	import LoomFrame from './LoomFrame.svelte';

	interface Props {
		variant?: 'corners' | 'full';
		text?: string;
	}

	let { variant = 'corners', text = 'Featured content' }: Props = $props();
</script>

<div style="width: 300px;">
	<LoomFrame {variant}>
		<p class="text-fg">{text}</p>
	</LoomFrame>
</div>
