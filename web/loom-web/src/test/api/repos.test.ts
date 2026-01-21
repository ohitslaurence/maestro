/**
 * Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
 * SPDX-License-Identifier: Proprietary
 */

import { describe, it, expect, vi, beforeEach } from 'vitest';
import { getReposClient, type Repository, type CreateRepoRequest } from '../../lib/api/repos';
import { ApiError } from '../../lib/api/types';

const mockFetch = vi.fn();
global.fetch = mockFetch;

beforeEach(() => {
	mockFetch.mockReset();
});

describe('ReposApiClient', () => {
	const client = getReposClient('http://localhost:3000');

	describe('createRepo', () => {
		it('sends correct POST request with repo data', async () => {
			const mockRepo: Repository = {
				id: 'repo-123',
				owner_type: 'user',
				owner_id: 'user-456',
				name: 'my-repo',
				visibility: 'private',
				default_branch: 'main',
				clone_url: 'https://example.com/user-456/my-repo.git',
				created_at: '2025-01-01T00:00:00Z',
				updated_at: '2025-01-01T00:00:00Z',
			};
			mockFetch.mockResolvedValueOnce({
				ok: true,
				status: 201,
				json: () => Promise.resolve(mockRepo),
			});

			const request: CreateRepoRequest = {
				owner_type: 'user',
				owner_id: 'user-456',
				name: 'my-repo',
				visibility: 'private',
			};

			const result = await client.createRepo(request);

			expect(mockFetch).toHaveBeenCalledWith(
				'http://localhost:3000/api/repos',
				expect.objectContaining({
					method: 'POST',
					headers: { 'Content-Type': 'application/json' },
					body: JSON.stringify(request),
				})
			);
			expect(result).toEqual(mockRepo);
		});

		it('creates org-owned repository', async () => {
			const mockRepo: Repository = {
				id: 'repo-789',
				owner_type: 'org',
				owner_id: 'org-123',
				name: 'org-repo',
				visibility: 'public',
				default_branch: 'main',
				clone_url: 'https://example.com/org-123/org-repo.git',
				created_at: '2025-01-01T00:00:00Z',
				updated_at: '2025-01-01T00:00:00Z',
			};
			mockFetch.mockResolvedValueOnce({
				ok: true,
				status: 201,
				json: () => Promise.resolve(mockRepo),
			});

			const request: CreateRepoRequest = {
				owner_type: 'org',
				owner_id: 'org-123',
				name: 'org-repo',
				visibility: 'public',
			};

			const result = await client.createRepo(request);

			expect(mockFetch).toHaveBeenCalledWith(
				'http://localhost:3000/api/repos',
				expect.objectContaining({
					method: 'POST',
					body: JSON.stringify(request),
				})
			);
			expect(result.owner_type).toBe('org');
			expect(result.visibility).toBe('public');
		});

		it('throws ApiError on 400 bad request', async () => {
			mockFetch.mockResolvedValueOnce({
				ok: false,
				status: 400,
				text: () => Promise.resolve('Invalid repository name'),
			});

			const request: CreateRepoRequest = {
				owner_type: 'user',
				owner_id: 'user-456',
				name: '',
				visibility: 'private',
			};

			try {
				await client.createRepo(request);
				expect.fail('Expected an error to be thrown');
			} catch (error) {
				expect(error).toBeInstanceOf(ApiError);
				expect((error as ApiError).status).toBe(400);
			}
		});

		it('throws ApiError on 401 unauthorized', async () => {
			mockFetch.mockResolvedValueOnce({
				ok: false,
				status: 401,
				text: () => Promise.resolve('Unauthorized'),
			});

			const request: CreateRepoRequest = {
				owner_type: 'user',
				owner_id: 'user-456',
				name: 'my-repo',
				visibility: 'private',
			};

			try {
				await client.createRepo(request);
				expect.fail('Expected an error to be thrown');
			} catch (error) {
				expect(error).toBeInstanceOf(ApiError);
				expect((error as ApiError).status).toBe(401);
			}
		});

		it('throws ApiError on 403 forbidden', async () => {
			mockFetch.mockResolvedValueOnce({
				ok: false,
				status: 403,
				text: () => Promise.resolve('Forbidden'),
			});

			const request: CreateRepoRequest = {
				owner_type: 'org',
				owner_id: 'org-123',
				name: 'org-repo',
				visibility: 'private',
			};

			try {
				await client.createRepo(request);
				expect.fail('Expected an error to be thrown');
			} catch (error) {
				expect(error).toBeInstanceOf(ApiError);
				expect((error as ApiError).status).toBe(403);
			}
		});

		it('throws ApiError on 409 conflict (duplicate name)', async () => {
			mockFetch.mockResolvedValueOnce({
				ok: false,
				status: 409,
				text: () => Promise.resolve('Repository already exists'),
			});

			const request: CreateRepoRequest = {
				owner_type: 'user',
				owner_id: 'user-456',
				name: 'existing-repo',
				visibility: 'private',
			};

			try {
				await client.createRepo(request);
				expect.fail('Expected an error to be thrown');
			} catch (error) {
				expect(error).toBeInstanceOf(ApiError);
				expect((error as ApiError).status).toBe(409);
			}
		});
	});

	describe('listRepos', () => {
		it('returns repos list', async () => {
			const mockResponse = {
				repos: [
					{
						id: 'repo-1',
						owner_type: 'user',
						owner_id: 'user-123',
						name: 'repo-one',
						visibility: 'private',
						default_branch: 'main',
						clone_url: 'https://example.com/user-123/repo-one.git',
						created_at: '2025-01-01T00:00:00Z',
						updated_at: '2025-01-01T00:00:00Z',
					},
				],
				total: 1,
			};
			mockFetch.mockResolvedValueOnce({
				ok: true,
				status: 200,
				json: () => Promise.resolve(mockResponse),
			});

			const result = await client.listRepos('user-123');

			expect(mockFetch).toHaveBeenCalledWith(
				'http://localhost:3000/api/users/user-123/repos',
				expect.objectContaining({
					headers: { 'Content-Type': 'application/json' },
				})
			);
			expect(result).toEqual(mockResponse);
		});

		it('sends pagination params', async () => {
			const mockResponse = { repos: [], total: 0 };
			mockFetch.mockResolvedValueOnce({
				ok: true,
				status: 200,
				json: () => Promise.resolve(mockResponse),
			});

			await client.listRepos('user-123', { limit: 10, offset: 20 });

			expect(mockFetch).toHaveBeenCalledWith(
				'http://localhost:3000/api/users/user-123/repos?limit=10&offset=20',
				expect.anything()
			);
		});
	});

	describe('getRepo', () => {
		it('returns repository by owner and name', async () => {
			const mockRepo: Repository = {
				id: 'repo-123',
				owner_type: 'user',
				owner_id: 'alice',
				name: 'my-project',
				visibility: 'public',
				default_branch: 'main',
				clone_url: 'https://example.com/alice/my-project.git',
				created_at: '2025-01-01T00:00:00Z',
				updated_at: '2025-01-01T00:00:00Z',
			};
			mockFetch.mockResolvedValueOnce({
				ok: true,
				status: 200,
				json: () => Promise.resolve(mockRepo),
			});

			const result = await client.getRepo('alice', 'my-project');

			expect(mockFetch).toHaveBeenCalledWith(
				'http://localhost:3000/api/repos/alice/my-project',
				expect.objectContaining({
					headers: { 'Content-Type': 'application/json' },
				})
			);
			expect(result).toEqual(mockRepo);
		});

		it('throws on 404 not found', async () => {
			mockFetch.mockResolvedValueOnce({
				ok: false,
				status: 404,
				text: () => Promise.resolve('Repository not found'),
			});

			try {
				await client.getRepo('alice', 'nonexistent');
				expect.fail('Expected an error to be thrown');
			} catch (error) {
				expect(error).toBeInstanceOf(ApiError);
				expect((error as ApiError).status).toBe(404);
			}
		});
	});
});
