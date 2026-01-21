<!--
  Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
  SPDX-License-Identifier: Proprietary
-->
<script lang="ts">
	import { onMount } from 'svelte';
	import { i18n } from '$lib/i18n';
	import { Button } from '$lib/ui';

	interface Props {
		weaverId: string;
		onDisconnect?: () => void;
	}

	let { weaverId, onDisconnect }: Props = $props();

	let terminalContainer: HTMLDivElement;
	let terminal: import('@xterm/xterm').Terminal | null = null;
	let fitAddon: import('@xterm/addon-fit').FitAddon | null = null;
	let ws: WebSocket | null = null;
	let keepAliveInterval: ReturnType<typeof setInterval> | null = null;

	const KEEPALIVE_INTERVAL_MS = 15000;

	let connectionStatus = $state<'connecting' | 'connected' | 'disconnected' | 'error'>('connecting');
	let errorMessage = $state<string | null>(null);

	function getWebSocketUrl(): string {
		const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
		return `${protocol}//${window.location.host}/api/weaver/${encodeURIComponent(weaverId)}/attach`;
	}

	async function initTerminal() {
		const { Terminal } = await import('@xterm/xterm');
		const { FitAddon } = await import('@xterm/addon-fit');
		const { WebLinksAddon } = await import('@xterm/addon-web-links');

		terminal = new Terminal({
			cursorBlink: true,
			fontSize: 14,
			fontFamily: 'var(--font-mono)',
			theme: {
				background: 'var(--color-bg)',
				foreground: 'var(--color-fg)',
				cursor: 'var(--color-fg)',
				cursorAccent: 'var(--color-bg)',
				selectionBackground: 'var(--color-accent-soft)',
				black: '#0D0C0B',
				red: '#A63D2F',
				green: '#4A7C59',
				yellow: '#C9A227',
				blue: '#4A6FA5',
				magenta: '#8B3A62',
				cyan: '#4A6FA5',
				white: '#F7F4F0',
				brightBlack: '#6B6560',
				brightRed: '#C94F3F',
				brightGreen: '#5A9C69',
				brightYellow: '#D9B237',
				brightBlue: '#5A8FC5',
				brightMagenta: '#AB4A72',
				brightCyan: '#6A8FC5',
				brightWhite: '#FFFFFF',
			},
		});

		fitAddon = new FitAddon();
		terminal.loadAddon(fitAddon);
		terminal.loadAddon(new WebLinksAddon());

		terminal.open(terminalContainer);
		fitAddon.fit();

		terminal.onData((data) => {
			if (ws && ws.readyState === WebSocket.OPEN) {
				ws.send(data);
			}
		});

		terminal.onBinary((data) => {
			if (ws && ws.readyState === WebSocket.OPEN) {
				const buffer = new Uint8Array(data.length);
				for (let i = 0; i < data.length; i++) {
					buffer[i] = data.charCodeAt(i);
				}
				ws.send(buffer);
			}
		});

		connectWebSocket();
	}

	function connectWebSocket() {
		const url = getWebSocketUrl();
		connectionStatus = 'connecting';
		errorMessage = null;

		ws = new WebSocket(url);
		ws.binaryType = 'arraybuffer';

		ws.onopen = () => {
			connectionStatus = 'connected';
			terminal?.focus();
			startKeepAlive();
			sendTerminalRefresh();
		};

		ws.onmessage = (event) => {
			if (event.data instanceof ArrayBuffer) {
				const decoder = new TextDecoder();
				terminal?.write(decoder.decode(event.data));
			} else if (typeof event.data === 'string') {
				terminal?.write(event.data);
			}
		};

		ws.onclose = (event) => {
			connectionStatus = 'disconnected';
			stopKeepAlive();
			if (event.code !== 1000) {
				errorMessage = `${i18n._('weavers.terminal.connectionClosed')} ${event.reason || i18n._('weavers.terminal.unknownReason')}`;
			}
			onDisconnect?.();
		};

		ws.onerror = () => {
			connectionStatus = 'error';
			stopKeepAlive();
			errorMessage = i18n._('weavers.terminal.connectFailed');
		};
	}

	function sendTerminalRefresh() {
		if (ws && ws.readyState === WebSocket.OPEN) {
			ws.send(new Uint8Array([12]));
		}
	}

	function startKeepAlive() {
		stopKeepAlive();
		keepAliveInterval = setInterval(() => {
			if (ws && ws.readyState === WebSocket.OPEN) {
				ws.send(new Uint8Array(0));
			}
		}, KEEPALIVE_INTERVAL_MS);
	}

	function stopKeepAlive() {
		if (keepAliveInterval) {
			clearInterval(keepAliveInterval);
			keepAliveInterval = null;
		}
	}

	function reconnect() {
		if (ws) {
			ws.close();
		}
		terminal?.clear();
		connectWebSocket();
	}

	function handleResize() {
		if (fitAddon) {
			fitAddon.fit();
		}
	}

	onMount(() => {
		initTerminal();

		const resizeObserver = new ResizeObserver(() => {
			handleResize();
		});
		resizeObserver.observe(terminalContainer);

		return () => {
			resizeObserver.disconnect();
			stopKeepAlive();
			if (ws) {
				ws.close();
			}
			if (terminal) {
				terminal.dispose();
			}
		};
	});
</script>

<svelte:head>
	<link rel="stylesheet" href="https://cdn.jsdelivr.net/npm/@xterm/xterm@5.5.0/css/xterm.min.css" />
</svelte:head>

<div class="terminal-wrapper">
	<div class="terminal-header">
		<div class="terminal-status">
			<div
				class="terminal-status-dot"
				class:terminal-status-connected={connectionStatus === 'connected'}
				class:terminal-status-connecting={connectionStatus === 'connecting'}
				class:terminal-status-error={connectionStatus === 'error' || connectionStatus === 'disconnected'}
			></div>
			<span class="terminal-status-text">
				{#if connectionStatus === 'connected'}
					{i18n._('weavers.terminal.connected')}
				{:else if connectionStatus === 'connecting'}
					{i18n._('weavers.terminal.connecting')}
				{:else if connectionStatus === 'error'}
					{i18n._('weavers.terminal.error')}
				{:else}
					{i18n._('weavers.terminal.disconnected')}
				{/if}
			</span>
		</div>
		{#if connectionStatus === 'disconnected' || connectionStatus === 'error'}
			<Button
				variant="secondary"
				size="sm"
				onclick={reconnect}
			>
				{i18n._('weavers.terminal.reconnect')}
			</Button>
		{/if}
	</div>

	{#if errorMessage}
		<div class="terminal-error">
			{errorMessage}
		</div>
	{/if}

	<div
		bind:this={terminalContainer}
		class="terminal-container"
	></div>
</div>

<style>
	.terminal-wrapper {
		display: flex;
		flex-direction: column;
		height: 100%;
		font-family: var(--font-mono);
	}

	.terminal-header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: var(--space-2) var(--space-3);
		background: var(--color-bg-muted);
		border-bottom: 1px solid var(--color-border);
	}

	.terminal-status {
		display: flex;
		align-items: center;
		gap: var(--space-2);
	}

	.terminal-status-dot {
		width: 8px;
		height: 8px;
		border-radius: var(--radius-full);
	}

	.terminal-status-connected {
		background: var(--color-success);
	}

	.terminal-status-connecting {
		background: var(--color-warning);
		animation: pulse 1.5s ease-in-out infinite;
	}

	.terminal-status-error {
		background: var(--color-error);
	}

	.terminal-status-text {
		font-size: var(--text-sm);
		color: var(--color-fg-muted);
	}

	.terminal-error {
		padding: var(--space-2) var(--space-3);
		background: var(--color-error-soft);
		color: var(--color-error);
		font-size: var(--text-sm);
	}

	.terminal-container {
		flex: 1;
		min-height: 0;
		background: var(--color-bg);
	}

	:global(.xterm) {
		height: 100%;
		padding: var(--space-2);
	}

	:global(.xterm-viewport) {
		overflow-y: auto !important;
	}

	@keyframes pulse {
		0%, 100% {
			opacity: 1;
		}
		50% {
			opacity: 0.5;
		}
	}
</style>
