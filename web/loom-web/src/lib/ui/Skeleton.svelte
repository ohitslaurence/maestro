<script lang="ts">
	interface Props {
		width?: string;
		height?: string;
		rounded?: 'sm' | 'md' | 'lg' | 'full';
	}

	let { width = '100%', height = '1rem', rounded = 'md' }: Props = $props();

	const roundedMap: Record<string, string> = {
		sm: 'var(--radius-sm)',
		md: 'var(--radius-md)',
		lg: 'var(--radius-lg)',
		full: '9999px',
	};
</script>

<div
	class="skeleton"
	style="width: {width}; height: {height}; border-radius: {roundedMap[rounded]};"
></div>

<style>
	.skeleton {
		background: var(--color-bg-subtle);
		position: relative;
		overflow: hidden;
	}

	.skeleton::after {
		content: '';
		position: absolute;
		top: 0;
		left: 0;
		right: 0;
		bottom: 0;
		background: linear-gradient(
			90deg,
			transparent 0%,
			var(--color-border-subtle) 25%,
			var(--color-bg-elevated) 50%,
			var(--color-border-subtle) 75%,
			transparent 100%
		);
		animation: thread-weave 1.5s ease-in-out infinite;
		transform: translateX(-100%);
	}

	@keyframes thread-weave {
		0% {
			transform: translateX(-100%);
		}
		100% {
			transform: translateX(100%);
		}
	}
</style>
