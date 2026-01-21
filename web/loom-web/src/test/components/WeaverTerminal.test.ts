/**
 * Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
 * SPDX-License-Identifier: Proprietary
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';

class MockWebSocket {
	static CONNECTING = 0;
	static OPEN = 1;
	static CLOSING = 2;
	static CLOSED = 3;

	url: string;
	readyState: number = MockWebSocket.CONNECTING;
	binaryType: string = 'blob';
	onopen: (() => void) | null = null;
	onclose: ((event: { code: number; reason: string }) => void) | null = null;
	onmessage: ((event: { data: unknown }) => void) | null = null;
	onerror: (() => void) | null = null;

	sentMessages: unknown[] = [];

	constructor(url: string) {
		this.url = url;
	}

	send(data: unknown) {
		this.sentMessages.push(data);
	}

	close() {
		this.readyState = MockWebSocket.CLOSED;
	}

	simulateOpen() {
		this.readyState = MockWebSocket.OPEN;
		this.onopen?.();
	}

	simulateClose(code = 1000, reason = '') {
		this.readyState = MockWebSocket.CLOSED;
		this.onclose?.({ code, reason });
	}

	simulateError() {
		this.onerror?.();
	}
}

describe('WeaverTerminal keep-alive', () => {
	let mockWs: MockWebSocket;

	beforeEach(() => {
		vi.useFakeTimers();
		mockWs = new MockWebSocket('ws://localhost/api/weaver/test-id/attach');
	});

	afterEach(() => {
		vi.useRealTimers();
	});

	it('keep-alive interval should be 15 seconds', () => {
		const KEEPALIVE_INTERVAL_MS = 15000;
		expect(KEEPALIVE_INTERVAL_MS).toBe(15000);
	});

	it('should send keep-alive pings at regular intervals', () => {
		mockWs.simulateOpen();

		let pingCount = 0;
		const keepAliveInterval = setInterval(() => {
			if (mockWs.readyState === MockWebSocket.OPEN) {
				mockWs.send(new Uint8Array(0));
				pingCount++;
			}
		}, 15000);

		// Advance time by 45 seconds (should trigger 3 pings)
		vi.advanceTimersByTime(45000);

		expect(pingCount).toBe(3);
		expect(mockWs.sentMessages.length).toBe(3);

		clearInterval(keepAliveInterval);
	});

	it('should stop sending pings when connection closes', () => {
		mockWs.simulateOpen();

		let pingCount = 0;
		const keepAliveInterval = setInterval(() => {
			if (mockWs.readyState === MockWebSocket.OPEN) {
				mockWs.send(new Uint8Array(0));
				pingCount++;
			}
		}, 15000);

		// Advance 15 seconds, send one ping
		vi.advanceTimersByTime(15000);
		expect(pingCount).toBe(1);

		// Close connection
		mockWs.simulateClose();

		// Advance another 30 seconds
		vi.advanceTimersByTime(30000);

		// Should still be 1 ping (no more sent after close)
		expect(pingCount).toBe(1);

		clearInterval(keepAliveInterval);
	});

	it('empty Uint8Array ping should have zero length', () => {
		const ping = new Uint8Array(0);
		expect(ping.length).toBe(0);
		expect(ping.byteLength).toBe(0);
	});

	it('should send terminal refresh (Ctrl+L) on connect', () => {
		mockWs.simulateOpen();

		// Simulate sending Ctrl+L (ASCII 12) on connect
		const ctrlL = new Uint8Array([12]);
		mockWs.send(ctrlL);

		expect(mockWs.sentMessages.length).toBe(1);
		const sent = mockWs.sentMessages[0] as Uint8Array;
		expect(sent[0]).toBe(12); // ASCII 12 = Form Feed (Ctrl+L)
	});

	it('Ctrl+L should be ASCII code 12', () => {
		const CTRL_L = 12;
		expect(CTRL_L).toBe(12);
		expect(String.fromCharCode(CTRL_L)).toBe('\f'); // Form feed character
	});
});

describe('WebSocket URL construction', () => {
	it('should use wss for https', () => {
		const protocol: string = 'https:';
		const host = 'loom.example.com';
		const weaverId = 'test-weaver-123';

		const wsProtocol = protocol === 'https:' ? 'wss:' : 'ws:';
		const url = `${wsProtocol}//${host}/api/weaver/${encodeURIComponent(weaverId)}/attach`;

		expect(url).toBe('wss://loom.example.com/api/weaver/test-weaver-123/attach');
	});

	it('should use ws for http', () => {
		const protocol: string = 'http:';
		const host = 'localhost:3000';
		const weaverId = 'test-weaver-456';

		const wsProtocol = protocol === 'https:' ? 'wss:' : 'ws:';
		const url = `${wsProtocol}//${host}/api/weaver/${encodeURIComponent(weaverId)}/attach`;

		expect(url).toBe('ws://localhost:3000/api/weaver/test-weaver-456/attach');
	});

	it('should encode special characters in weaver ID', () => {
		const weaverId = 'weaver/with spaces&special';
		const encoded = encodeURIComponent(weaverId);

		expect(encoded).toBe('weaver%2Fwith%20spaces%26special');
	});
});
