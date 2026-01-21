/**
 * Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
 * SPDX-License-Identifier: Proprietary
 */

import type {
	Thread,
	ListResponse,
	SearchResponse,
	ThreadVisibility,
	ListParams,
	SearchParams,
	AuthProvidersResponse,
	CurrentUser,
	AuthSuccessResponse,
	MagicLinkRequest,
	DeviceCodeStartResponse,
	DeviceCodePollResponse,
	DeviceCodeCompleteRequest,
	SessionListResponse,
	UpdateProfileRequest,
	ListOrgsResponse,
	Org,
	CreateOrgRequest,
	UpdateOrgRequest,
	OrgMemberListResponse,
	OrgRole,
	Team,
	TeamListResponse,
	TeamMemberListResponse,
	CreateTeamRequest,
	UpdateTeamRequest,
	TeamRole,
	ApiKeyListResponse,
	CreateApiKeyRequest,
	CreateApiKeyResponse,
	SupportAccessRequest,
	SupportAccessApproval,
	SupportAccessResponse,
	ImpersonationState,
	ImpersonateResponse,
	StopImpersonationResponse,
	AdminUserListResponse,
	UpdateUserRolesRequest,
	UpdateUserRolesResponse,
	DeleteUserResponse,
	Weaver,
	ListWeaversResponse,
	CreateWeaverRequest,
	WsTokenResponse,
} from './types';
import { ApiError } from './types';

export class LoomApiClient {
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

	// Thread operations
	async listThreads(params: ListParams = {}): Promise<ListResponse> {
		const query = new URLSearchParams();
		if (params.workspace) query.set('workspace', params.workspace);
		if (params.limit) query.set('limit', String(params.limit));
		if (params.offset) query.set('offset', String(params.offset));

		const queryStr = query.toString();
		const path = queryStr ? `/api/threads?${queryStr}` : '/api/threads';
		return this.request<ListResponse>(path);
	}

	async getThread(id: string): Promise<Thread> {
		return this.request<Thread>(`/api/threads/${encodeURIComponent(id)}`);
	}

	async createOrUpdateThread(thread: Thread, expectedVersion?: number): Promise<Thread> {
		const headers: Record<string, string> = {};
		if (expectedVersion !== undefined) {
			headers['If-Match'] = String(expectedVersion);
		}

		return this.request<Thread>(`/api/threads/${encodeURIComponent(thread.id)}`, {
			method: 'PUT',
			headers,
			body: JSON.stringify(thread),
		});
	}

	async deleteThread(id: string): Promise<void> {
		await this.request<void>(`/api/threads/${encodeURIComponent(id)}`, {
			method: 'DELETE',
		});
	}

	async searchThreads(query: string, params: SearchParams = {}): Promise<SearchResponse> {
		const searchParams = new URLSearchParams({ q: query });
		if (params.limit) searchParams.set('limit', String(params.limit));
		if (params.offset) searchParams.set('offset', String(params.offset));
		if (params.workspace) searchParams.set('workspace', params.workspace);

		return this.request<SearchResponse>(`/api/threads/search?${searchParams}`);
	}

	async updateVisibility(
		id: string,
		visibility: ThreadVisibility,
		expectedVersion?: number
	): Promise<Thread> {
		const headers: Record<string, string> = {};
		if (expectedVersion !== undefined) {
			headers['If-Match'] = String(expectedVersion);
		}

		return this.request<Thread>(`/api/threads/${encodeURIComponent(id)}/visibility`, {
			method: 'POST',
			headers,
			body: JSON.stringify({ visibility }),
		});
	}

	// Health check
	async healthCheck(): Promise<{ status: string }> {
		return this.request<{ status: string }>('/health');
	}

	// Auth operations
	async getAuthProviders(): Promise<AuthProvidersResponse> {
		return this.request<AuthProvidersResponse>('/auth/providers');
	}

	async getCurrentUser(): Promise<CurrentUser> {
		return this.request<CurrentUser>('/auth/me');
	}

	async getWsToken(): Promise<WsTokenResponse> {
		return this.request<WsTokenResponse>('/auth/ws-token');
	}

	async requestMagicLink(email: string): Promise<AuthSuccessResponse> {
		const body: MagicLinkRequest = { email };
		return this.request<AuthSuccessResponse>('/auth/magic-link', {
			method: 'POST',
			body: JSON.stringify(body),
		});
	}

	async logout(): Promise<void> {
		await this.request<void>('/auth/logout', { method: 'POST' });
	}

	async startDeviceCode(): Promise<DeviceCodeStartResponse> {
		return this.request<DeviceCodeStartResponse>('/auth/device/start', {
			method: 'POST',
		});
	}

	async pollDeviceCode(deviceCode: string): Promise<DeviceCodePollResponse> {
		return this.request<DeviceCodePollResponse>('/auth/device/poll', {
			method: 'POST',
			body: JSON.stringify({ device_code: deviceCode }),
		});
	}

	async completeDeviceCode(userCode: string): Promise<AuthSuccessResponse> {
		const body: DeviceCodeCompleteRequest = { user_code: userCode };
		return this.request<AuthSuccessResponse>('/auth/device/complete', {
			method: 'POST',
			body: JSON.stringify(body),
		});
	}

	async listSessions(): Promise<SessionListResponse> {
		return this.request<SessionListResponse>('/api/sessions');
	}

	async revokeSession(sessionId: string): Promise<void> {
		await this.request<void>(`/api/sessions/${encodeURIComponent(sessionId)}`, {
			method: 'DELETE',
		});
	}

	async updateProfile(data: UpdateProfileRequest): Promise<CurrentUser> {
		return this.request<CurrentUser>('/api/users/me', {
			method: 'PATCH',
			body: JSON.stringify(data),
		});
	}

	// Organization operations
	async listOrgs(): Promise<ListOrgsResponse> {
		return this.request<ListOrgsResponse>('/api/orgs');
	}

	async getOrg(id: string): Promise<Org> {
		return this.request<Org>(`/api/orgs/${encodeURIComponent(id)}`);
	}

	async createOrg(data: CreateOrgRequest): Promise<Org> {
		return this.request<Org>('/api/orgs', {
			method: 'POST',
			body: JSON.stringify(data),
		});
	}

	async updateOrg(orgId: string, data: UpdateOrgRequest): Promise<Org> {
		return this.request<Org>(`/api/orgs/${encodeURIComponent(orgId)}`, {
			method: 'PATCH',
			body: JSON.stringify(data),
		});
	}

	async deleteOrg(orgId: string): Promise<void> {
		await this.request<void>(`/api/orgs/${encodeURIComponent(orgId)}`, {
			method: 'DELETE',
		});
	}

	async listOrgMembers(orgId: string): Promise<OrgMemberListResponse> {
		return this.request<OrgMemberListResponse>(`/api/orgs/${encodeURIComponent(orgId)}/members`);
	}

	async removeOrgMember(orgId: string, userId: string): Promise<void> {
		await this.request<void>(`/api/orgs/${encodeURIComponent(orgId)}/members/${encodeURIComponent(userId)}`, {
			method: 'DELETE',
		});
	}

	async updateOrgMemberRole(orgId: string, userId: string, role: OrgRole): Promise<void> {
		await this.request<void>(`/api/orgs/${encodeURIComponent(orgId)}/members/${encodeURIComponent(userId)}`, {
			method: 'PATCH',
			body: JSON.stringify({ role }),
		});
	}

	// Team methods
	async listTeams(orgId: string): Promise<TeamListResponse> {
		return this.request<TeamListResponse>(`/api/orgs/${encodeURIComponent(orgId)}/teams`);
	}

	async getTeam(orgId: string, teamId: string): Promise<Team> {
		return this.request<Team>(`/api/orgs/${encodeURIComponent(orgId)}/teams/${encodeURIComponent(teamId)}`);
	}

	async createTeam(orgId: string, data: CreateTeamRequest): Promise<Team> {
		return this.request<Team>(`/api/orgs/${encodeURIComponent(orgId)}/teams`, {
			method: 'POST',
			body: JSON.stringify(data),
		});
	}

	async updateTeam(orgId: string, teamId: string, data: UpdateTeamRequest): Promise<Team> {
		return this.request<Team>(`/api/orgs/${encodeURIComponent(orgId)}/teams/${encodeURIComponent(teamId)}`, {
			method: 'PATCH',
			body: JSON.stringify(data),
		});
	}

	async deleteTeam(orgId: string, teamId: string): Promise<void> {
		await this.request<void>(`/api/orgs/${encodeURIComponent(orgId)}/teams/${encodeURIComponent(teamId)}`, {
			method: 'DELETE',
		});
	}

	async listTeamMembers(orgId: string, teamId: string): Promise<TeamMemberListResponse> {
		return this.request<TeamMemberListResponse>(`/api/orgs/${encodeURIComponent(orgId)}/teams/${encodeURIComponent(teamId)}/members`);
	}

	async addTeamMember(orgId: string, teamId: string, userId: string, role: TeamRole): Promise<void> {
		await this.request<void>(`/api/orgs/${encodeURIComponent(orgId)}/teams/${encodeURIComponent(teamId)}/members`, {
			method: 'POST',
			body: JSON.stringify({ user_id: userId, role }),
		});
	}

	async removeTeamMember(orgId: string, teamId: string, userId: string): Promise<void> {
		await this.request<void>(`/api/orgs/${encodeURIComponent(orgId)}/teams/${encodeURIComponent(teamId)}/members/${encodeURIComponent(userId)}`, {
			method: 'DELETE',
		});
	}

	async updateTeamMemberRole(orgId: string, teamId: string, userId: string, role: TeamRole): Promise<void> {
		await this.request<void>(`/api/orgs/${encodeURIComponent(orgId)}/teams/${encodeURIComponent(teamId)}/members/${encodeURIComponent(userId)}`, {
			method: 'PATCH',
			body: JSON.stringify({ role }),
		});
	}

	// API Key methods
	async listApiKeys(orgId: string): Promise<ApiKeyListResponse> {
		return this.request<ApiKeyListResponse>(`/api/orgs/${encodeURIComponent(orgId)}/api-keys`);
	}

	async createApiKey(orgId: string, data: CreateApiKeyRequest): Promise<CreateApiKeyResponse> {
		return this.request<CreateApiKeyResponse>(`/api/orgs/${encodeURIComponent(orgId)}/api-keys`, {
			method: 'POST',
			body: JSON.stringify(data),
		});
	}

	async revokeApiKey(orgId: string, keyId: string): Promise<void> {
		await this.request<void>(`/api/orgs/${encodeURIComponent(orgId)}/api-keys/${encodeURIComponent(keyId)}`, {
			method: 'DELETE',
		});
	}

	// Support Access methods
	async requestSupportAccess(threadId: string): Promise<SupportAccessRequest> {
		return this.request<SupportAccessRequest>(
			`/api/threads/${encodeURIComponent(threadId)}/support-access/request`,
			{ method: 'POST' }
		);
	}

	async approveSupportAccess(threadId: string): Promise<SupportAccessApproval> {
		return this.request<SupportAccessApproval>(
			`/api/threads/${encodeURIComponent(threadId)}/support-access/approve`,
			{ method: 'POST' }
		);
	}

	async revokeSupportAccess(threadId: string): Promise<SupportAccessResponse> {
		return this.request<SupportAccessResponse>(
			`/api/threads/${encodeURIComponent(threadId)}/support-access`,
			{ method: 'DELETE' }
		);
	}

	// Admin impersonation methods
	async getImpersonationState(): Promise<ImpersonationState> {
		return this.request<ImpersonationState>('/api/admin/impersonate/state');
	}

	async startImpersonation(userId: string, reason: string = 'Admin impersonation'): Promise<ImpersonateResponse> {
		return this.request<ImpersonateResponse>(
			`/api/admin/users/${encodeURIComponent(userId)}/impersonate`,
			{
				method: 'POST',
				body: JSON.stringify({ reason }),
			}
		);
	}

	async stopImpersonation(): Promise<StopImpersonationResponse> {
		return this.request<StopImpersonationResponse>('/api/admin/impersonate/stop', {
			method: 'POST',
		});
	}

	// Admin user management
	async listAdminUsers(params: { limit?: number; offset?: number; search?: string } = {}): Promise<AdminUserListResponse> {
		const query = new URLSearchParams();
		if (params.limit) query.set('limit', String(params.limit));
		if (params.offset) query.set('offset', String(params.offset));
		if (params.search) query.set('search', params.search);

		const queryStr = query.toString();
		const path = queryStr ? `/api/admin/users?${queryStr}` : '/api/admin/users';
		return this.request<AdminUserListResponse>(path);
	}

	async updateUserRoles(userId: string, data: UpdateUserRolesRequest): Promise<UpdateUserRolesResponse> {
		return this.request<UpdateUserRolesResponse>(
			`/api/admin/users/${encodeURIComponent(userId)}/roles`,
			{
				method: 'PATCH',
				body: JSON.stringify(data),
			}
		);
	}

	async deleteUser(userId: string): Promise<DeleteUserResponse> {
		return this.request<DeleteUserResponse>(
			`/api/admin/users/${encodeURIComponent(userId)}`,
			{ method: 'DELETE' }
		);
	}

	// Weaver methods
	async listWeavers(): Promise<ListWeaversResponse> {
		return this.request<ListWeaversResponse>('/api/weavers');
	}

	async getWeaver(id: string): Promise<Weaver> {
		return this.request<Weaver>(`/api/weaver/${encodeURIComponent(id)}`);
	}

	async createWeaver(data: CreateWeaverRequest): Promise<Weaver> {
		return this.request<Weaver>('/api/weaver', {
			method: 'POST',
			body: JSON.stringify(data),
		});
	}

	async deleteWeaver(id: string): Promise<void> {
		await this.request<void>(`/api/weaver/${encodeURIComponent(id)}`, {
			method: 'DELETE',
		});
	}
}

// Singleton instance for convenience
let defaultClient: LoomApiClient | null = null;

export function getApiClient(baseUrl?: string): LoomApiClient {
	if (!defaultClient || baseUrl) {
		defaultClient = new LoomApiClient(baseUrl);
	}
	return defaultClient;
}
