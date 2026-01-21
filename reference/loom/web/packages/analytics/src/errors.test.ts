/**
 * Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
 * SPDX-License-Identifier: Proprietary
 */

import { describe, it, expect } from 'vitest';
import {
	AnalyticsError,
	InvalidApiKeyError,
	InvalidBaseUrlError,
	AuthenticationError,
	RateLimitedError,
	ClientClosedError,
	CaptureError,
	IdentifyError,
	StorageError,
	ValidationError,
	NetworkError,
	ServerError
} from './errors';

describe('AnalyticsError', () => {
	it('should create a basic error', () => {
		const error = new AnalyticsError('test error');
		expect(error.message).toBe('test error');
		expect(error.name).toBe('AnalyticsError');
		expect(error.isRetryable()).toBe(false);
	});

	it('should support cause', () => {
		const cause = new Error('original');
		const error = new AnalyticsError('wrapped', { cause });
		expect(error.cause).toBe(cause);
	});
});

describe('InvalidApiKeyError', () => {
	it('should have default message', () => {
		const error = new InvalidApiKeyError();
		expect(error.message).toBe('Invalid API key format');
		expect(error.name).toBe('InvalidApiKeyError');
		expect(error.isRetryable()).toBe(false);
	});

	it('should accept custom message', () => {
		const error = new InvalidApiKeyError('Custom message');
		expect(error.message).toBe('Custom message');
	});
});

describe('InvalidBaseUrlError', () => {
	it('should have default message', () => {
		const error = new InvalidBaseUrlError();
		expect(error.message).toBe('Invalid base URL');
		expect(error.name).toBe('InvalidBaseUrlError');
	});
});

describe('AuthenticationError', () => {
	it('should have default message', () => {
		const error = new AuthenticationError();
		expect(error.message).toBe('Authentication failed');
		expect(error.name).toBe('AuthenticationError');
	});
});

describe('RateLimitedError', () => {
	it('should have default message', () => {
		const error = new RateLimitedError();
		expect(error.message).toBe('Rate limited');
		expect(error.name).toBe('RateLimitedError');
		expect(error.isRetryable()).toBe(true);
	});

	it('should store retryAfterSecs', () => {
		const error = new RateLimitedError('Rate limited', 60);
		expect(error.retryAfterSecs).toBe(60);
	});
});

describe('ClientClosedError', () => {
	it('should have default message', () => {
		const error = new ClientClosedError();
		expect(error.message).toBe('Analytics client has been closed');
		expect(error.name).toBe('ClientClosedError');
	});
});

describe('CaptureError', () => {
	it('should include event name', () => {
		const error = new CaptureError('button_clicked');
		expect(error.message).toBe('Failed to capture event: button_clicked');
		expect(error.eventName).toBe('button_clicked');
		expect(error.isRetryable()).toBe(true);
	});

	it('should accept custom message', () => {
		const error = new CaptureError('button_clicked', 'Custom message');
		expect(error.message).toBe('Custom message');
	});
});

describe('IdentifyError', () => {
	it('should include distinct_id and user_id', () => {
		const error = new IdentifyError('anon123', 'user@example.com');
		expect(error.message).toBe('Failed to identify user: user@example.com');
		expect(error.distinctId).toBe('anon123');
		expect(error.userId).toBe('user@example.com');
		expect(error.isRetryable()).toBe(true);
	});
});

describe('StorageError', () => {
	it('should store message', () => {
		const error = new StorageError('localStorage unavailable');
		expect(error.message).toBe('localStorage unavailable');
		expect(error.name).toBe('StorageError');
	});
});

describe('ValidationError', () => {
	it('should include field name', () => {
		const error = new ValidationError('event_name', 'Event name too long');
		expect(error.message).toBe('Event name too long');
		expect(error.field).toBe('event_name');
	});
});

describe('NetworkError', () => {
	it('should have default message', () => {
		const error = new NetworkError();
		expect(error.message).toBe('Network request failed');
		expect(error.isRetryable()).toBe(true);
	});
});

describe('ServerError', () => {
	it('should include status code', () => {
		const error = new ServerError(500);
		expect(error.message).toBe('Server error: 500');
		expect(error.statusCode).toBe(500);
	});

	it('should be retryable for 5xx errors', () => {
		expect(new ServerError(500).isRetryable()).toBe(true);
		expect(new ServerError(502).isRetryable()).toBe(true);
		expect(new ServerError(503).isRetryable()).toBe(true);
	});

	it('should not be retryable for 4xx errors', () => {
		expect(new ServerError(400).isRetryable()).toBe(false);
		expect(new ServerError(401).isRetryable()).toBe(false);
		expect(new ServerError(404).isRetryable()).toBe(false);
	});
});
