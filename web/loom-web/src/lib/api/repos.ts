/**
 * Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
 * SPDX-License-Identifier: Proprietary
 */

import { ApiError } from './types';

export interface Repository {
	id: string;
	owner_type: 'user' | 'org';
	owner_id: string;
	name: string;
	visibility: 'private' | 'public';
	default_branch: string;
	clone_url: string;
	created_at: string;
	updated_at: string;
}

export interface TreeEntry {
	name: string;
	path: string;
	kind: 'file' | 'directory' | 'submodule' | 'symlink';
	sha: string;
	size?: number;
}

export interface CommitInfo {
	sha: string;
	message: string;
	author_name: string;
	author_email: string;
	author_date: string;
	parent_shas: string[];
}

export interface CommitWithDiff extends CommitInfo {
	diff: string;
}

export interface BlameLine {
	line_number: number;
	commit_sha: string;
	author_name: string;
	author_email: string;
	author_date: string;
	content: string;
}

export interface Branch {
	name: string;
	sha: string;
	is_default: boolean;
}

export interface CompareResult {
	base_ref: string;
	head_ref: string;
	commits: CommitInfo[];
	diff: string;
	ahead_by: number;
	behind_by: number;
}

export interface CreateRepoRequest {
	owner_type: 'org' | 'user';
	owner_id: string;
	name: string;
	visibility: 'private' | 'public';
}

export interface ListReposParams {
	limit?: number;
	offset?: number;
}

export interface ListReposResponse {
	repos: Repository[];
	total: number;
}

export interface ListCommitsParams {
	limit?: number;
	offset?: number;
	path?: string;
}

export interface ListCommitsResponse {
	commits: CommitInfo[];
	total: number;
}

class ReposApiClient {
	constructor(private baseUrl: string = '') {}

	private async request<T>(path: string, options: RequestInit = {}): Promise<T> {
		const url = `${this.baseUrl}${path}`;
		const response = await fetch(url, {
			...options,
			headers: {
				'Content-Type': 'application/json',
				...options.headers,
			},
		});

		if (!response.ok) {
			const body = await response.text();
			throw new ApiError(response.status, body);
		}

		if (response.status === 204) {
			return undefined as T;
		}

		return response.json();
	}

	async listRepos(userId: string, params: ListReposParams = {}): Promise<ListReposResponse> {
		const query = new URLSearchParams();
		if (params.limit) query.set('limit', String(params.limit));
		if (params.offset) query.set('offset', String(params.offset));

		const queryStr = query.toString();
		const path = queryStr
			? `/api/users/${encodeURIComponent(userId)}/repos?${queryStr}`
			: `/api/users/${encodeURIComponent(userId)}/repos`;
		return this.request<ListReposResponse>(path);
	}

	async getRepo(owner: string, name: string): Promise<Repository> {
		return this.request<Repository>(
			`/api/repos/${encodeURIComponent(owner)}/${encodeURIComponent(name)}`
		);
	}

	async getTree(owner: string, name: string, ref: string, path: string = ''): Promise<TreeEntry[]> {
		const encodedPath = path ? `/${path.split('/').map(encodeURIComponent).join('/')}` : '';
		return this.request<TreeEntry[]>(
			`/api/repos/${encodeURIComponent(owner)}/${encodeURIComponent(name)}/tree/${encodeURIComponent(ref)}${encodedPath}`
		);
	}

	async getBlob(owner: string, name: string, ref: string, path: string): Promise<string> {
		const encodedPath = path.split('/').map(encodeURIComponent).join('/');
		const url = `${this.baseUrl}/api/repos/${encodeURIComponent(owner)}/${encodeURIComponent(name)}/blob/${encodeURIComponent(ref)}/${encodedPath}`;
		const response = await fetch(url);

		if (!response.ok) {
			const body = await response.text();
			throw new ApiError(response.status, body);
		}

		return response.text();
	}

	async getCommits(
		owner: string,
		name: string,
		ref: string,
		params: ListCommitsParams = {}
	): Promise<ListCommitsResponse> {
		const query = new URLSearchParams();
		if (params.limit) query.set('limit', String(params.limit));
		if (params.offset) query.set('offset', String(params.offset));
		if (params.path) query.set('path', params.path);

		const queryStr = query.toString();
		const path = queryStr
			? `/api/repos/${encodeURIComponent(owner)}/${encodeURIComponent(name)}/commits/${encodeURIComponent(ref)}?${queryStr}`
			: `/api/repos/${encodeURIComponent(owner)}/${encodeURIComponent(name)}/commits/${encodeURIComponent(ref)}`;
		return this.request<ListCommitsResponse>(path);
	}

	async getCommit(owner: string, name: string, sha: string): Promise<CommitWithDiff> {
		return this.request<CommitWithDiff>(
			`/api/repos/${encodeURIComponent(owner)}/${encodeURIComponent(name)}/commit/${encodeURIComponent(sha)}`
		);
	}

	async getBlame(owner: string, name: string, ref: string, path: string): Promise<BlameLine[]> {
		const encodedPath = path.split('/').map(encodeURIComponent).join('/');
		return this.request<BlameLine[]>(
			`/api/repos/${encodeURIComponent(owner)}/${encodeURIComponent(name)}/blame/${encodeURIComponent(ref)}/${encodedPath}`
		);
	}

	async getBranches(owner: string, name: string): Promise<Branch[]> {
		return this.request<Branch[]>(
			`/api/repos/${encodeURIComponent(owner)}/${encodeURIComponent(name)}/branches`
		);
	}

	async compare(owner: string, name: string, base: string, head: string): Promise<CompareResult> {
		return this.request<CompareResult>(
			`/api/repos/${encodeURIComponent(owner)}/${encodeURIComponent(name)}/compare/${encodeURIComponent(base)}...${encodeURIComponent(head)}`
		);
	}

	async createRepo(request: CreateRepoRequest): Promise<Repository> {
		return this.request<Repository>('/api/repos', {
			method: 'POST',
			body: JSON.stringify(request),
		});
	}

	async deleteRepo(id: string): Promise<void> {
		return this.request<void>(`/api/repos/${encodeURIComponent(id)}`, {
			method: 'DELETE',
		});
	}
}

let defaultClient: ReposApiClient | null = null;

export function getReposClient(baseUrl?: string): ReposApiClient {
	if (!defaultClient || baseUrl) {
		defaultClient = new ReposApiClient(baseUrl);
	}
	return defaultClient;
}
