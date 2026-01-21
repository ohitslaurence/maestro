/**
 * Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
 * SPDX-License-Identifier: Proprietary
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { LoomApiClient } from '../../lib/api/client';
import { ApiError } from '../../lib/api/types';

const mockFetch = vi.fn();
global.fetch = mockFetch;

beforeEach(() => {
	mockFetch.mockReset();
});

describe('LoomApiClient auth methods', () => {
	const client = new LoomApiClient('http://localhost:3000');

	describe('getAuthProviders', () => {
		it('returns providers array', async () => {
			const mockResponse = { providers: ['github', 'google', 'magic-link'] };
			mockFetch.mockResolvedValueOnce({
				ok: true,
				status: 200,
				json: () => Promise.resolve(mockResponse),
			});

			const result = await client.getAuthProviders();

			expect(mockFetch).toHaveBeenCalledWith(
				'http://localhost:3000/auth/providers',
				expect.objectContaining({
					headers: { 'Content-Type': 'application/json' },
				})
			);
			expect(result).toEqual(mockResponse);
		});
	});

	describe('getCurrentUser', () => {
		it('returns user object', async () => {
			const mockUser = {
				id: 'user-123',
				display_name: 'Test User',
				email: 'test@example.com',
				avatar_url: null,
				locale: 'en',
				global_roles: ['user'],
				created_at: '2025-01-01T00:00:00Z',
			};
			mockFetch.mockResolvedValueOnce({
				ok: true,
				status: 200,
				json: () => Promise.resolve(mockUser),
			});

			const result = await client.getCurrentUser();

			expect(mockFetch).toHaveBeenCalledWith(
				'http://localhost:3000/auth/me',
				expect.objectContaining({
					headers: { 'Content-Type': 'application/json' },
				})
			);
			expect(result).toEqual(mockUser);
		});

		it('throws on 401', async () => {
			mockFetch.mockResolvedValueOnce({
				ok: false,
				status: 401,
				text: () => Promise.resolve('Unauthorized'),
			});

			try {
				await client.getCurrentUser();
				expect.fail('Expected an error to be thrown');
			} catch (error) {
				expect(error).toBeInstanceOf(ApiError);
				expect((error as ApiError).status).toBe(401);
			}
		});
	});

	describe('requestMagicLink', () => {
		it('sends correct payload', async () => {
			const mockResponse = { message: 'Magic link sent' };
			mockFetch.mockResolvedValueOnce({
				ok: true,
				status: 200,
				json: () => Promise.resolve(mockResponse),
			});

			const result = await client.requestMagicLink('test@example.com');

			expect(mockFetch).toHaveBeenCalledWith(
				'http://localhost:3000/auth/magic-link',
				expect.objectContaining({
					method: 'POST',
					headers: { 'Content-Type': 'application/json' },
					body: JSON.stringify({ email: 'test@example.com' }),
				})
			);
			expect(result).toEqual(mockResponse);
		});
	});

	describe('logout', () => {
		it('makes POST request', async () => {
			mockFetch.mockResolvedValueOnce({
				ok: true,
				status: 204,
			});

			await client.logout();

			expect(mockFetch).toHaveBeenCalledWith(
				'http://localhost:3000/auth/logout',
				expect.objectContaining({
					method: 'POST',
					headers: { 'Content-Type': 'application/json' },
				})
			);
		});
	});

	describe('listSessions', () => {
		it('returns sessions array', async () => {
			const mockResponse = {
				sessions: [
					{
						id: 'session-1',
						session_type: 'web',
						created_at: '2025-01-01T00:00:00Z',
						last_used_at: '2025-01-02T00:00:00Z',
						ip_address: '127.0.0.1',
						user_agent: 'Mozilla/5.0',
						geo_location: null,
						is_current: true,
					},
				],
			};
			mockFetch.mockResolvedValueOnce({
				ok: true,
				status: 200,
				json: () => Promise.resolve(mockResponse),
			});

			const result = await client.listSessions();

			expect(mockFetch).toHaveBeenCalledWith(
				'http://localhost:3000/api/sessions',
				expect.objectContaining({
					headers: { 'Content-Type': 'application/json' },
				})
			);
			expect(result).toEqual(mockResponse);
		});
	});

	describe('revokeSession', () => {
		it('makes DELETE request', async () => {
			mockFetch.mockResolvedValueOnce({
				ok: true,
				status: 204,
			});

			await client.revokeSession('session-123');

			expect(mockFetch).toHaveBeenCalledWith(
				'http://localhost:3000/api/sessions/session-123',
				expect.objectContaining({
					method: 'DELETE',
					headers: { 'Content-Type': 'application/json' },
				})
			);
		});
	});

	describe('updateProfile', () => {
		it('sends PATCH with correct body', async () => {
			const mockUser = {
				id: 'user-123',
				display_name: 'Updated Name',
				email: 'test@example.com',
				avatar_url: null,
				locale: 'es',
				global_roles: ['user'],
				created_at: '2025-01-01T00:00:00Z',
			};
			mockFetch.mockResolvedValueOnce({
				ok: true,
				status: 200,
				json: () => Promise.resolve(mockUser),
			});

			const result = await client.updateProfile({
				display_name: 'Updated Name',
				locale: 'es',
			});

			expect(mockFetch).toHaveBeenCalledWith(
				'http://localhost:3000/api/users/me',
				expect.objectContaining({
					method: 'PATCH',
					headers: { 'Content-Type': 'application/json' },
					body: JSON.stringify({ display_name: 'Updated Name', locale: 'es' }),
				})
			);
			expect(result).toEqual(mockUser);
		});
	});
});
