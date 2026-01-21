<!--
  Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
  SPDX-License-Identifier: Proprietary
-->
<script lang="ts">
	import { LoomFrame } from '$lib/ui';
	import { i18n } from '$lib/i18n';
</script>

<div class="threads-empty">
	<LoomFrame variant="full">
		<div class="content">
			<p class="title">{i18n.t('thread.selectToView')}</p>
			<p class="subtitle">{i18n.t('thread.createToStart')}</p>
		</div>
	</LoomFrame>
</div>

<style>
	.threads-empty {
		display: flex;
		align-items: center;
		justify-content: center;
		height: 100%;
		font-family: var(--font-mono);
		background: var(--color-bg);
	}

	.content {
		text-align: center;
		padding: var(--space-8);
	}

	.title {
		font-size: var(--text-lg);
		color: var(--color-fg-muted);
		margin-bottom: var(--space-2);
	}

	.subtitle {
		font-size: var(--text-sm);
		color: var(--color-fg-subtle);
	}
</style>
