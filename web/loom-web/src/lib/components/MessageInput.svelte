<!--
  Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
  SPDX-License-Identifier: Proprietary
-->

<script lang="ts">
	import { Button } from '../ui';
	import { i18n } from '$lib/i18n';

	interface Props {
		disabled?: boolean;
		placeholder?: string;
		onSubmit?: (content: string) => void;
	}

	let { disabled = false, placeholder = i18n.t('message.placeholder'), onSubmit }: Props = $props();

	let inputValue = $state('');

	function handleSubmit() {
		const content = inputValue.trim();
		if (content && !disabled) {
			onSubmit?.(content);
			inputValue = '';
		}
	}

	function handleKeydown(event: KeyboardEvent) {
		if (event.key === 'Enter' && !event.shiftKey) {
			event.preventDefault();
			handleSubmit();
		}
	}
</script>

<div class="input-container">
	<div class="input-wrapper">
		<textarea
			bind:value={inputValue}
			{placeholder}
			{disabled}
			rows="1"
			class="message-textarea"
			onkeydown={handleKeydown}
		></textarea>
		<Button variant="primary" disabled={disabled || !inputValue.trim()} onclick={handleSubmit}>
			{i18n.t('message.send')}
		</Button>
	</div>
</div>

<style>
	.input-container {
		border-top: 1px solid var(--color-border);
		padding: var(--space-4);
		background: var(--color-bg);
	}

	.input-wrapper {
		display: flex;
		gap: var(--space-2);
	}

	.message-textarea {
		flex: 1;
		resize: none;
		border-radius: var(--radius-md);
		border: 1px solid var(--color-border);
		background: var(--color-bg-muted);
		padding: var(--space-2) var(--space-3);
		font-family: var(--font-mono);
		font-size: var(--text-base);
		color: var(--color-fg);
		transition: border-color 0.15s ease, box-shadow 0.15s ease;
	}

	.message-textarea::placeholder {
		color: var(--color-fg-subtle);
	}

	.message-textarea:focus {
		outline: none;
		border-color: var(--color-accent);
		box-shadow: 0 0 0 2px var(--color-accent-soft);
	}

	.message-textarea:disabled {
		cursor: not-allowed;
		opacity: 0.5;
	}
</style>
