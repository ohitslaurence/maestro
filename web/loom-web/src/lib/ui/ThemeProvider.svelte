<script lang="ts">
	import type { Snippet } from 'svelte';
	import { themeStore } from './theme';

	interface Props {
		children: Snippet;
	}

	let { children }: Props = $props();

	$effect(() => {
		themeStore.init();
	});
</script>

{@render children()}
