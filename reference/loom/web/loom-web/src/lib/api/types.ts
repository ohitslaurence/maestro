/**
 * Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
 * SPDX-License-Identifier: Proprietary
 */

// Core API types for loom-web

export interface Thread {
	id: string;
	title: string | null;
	created_at: string;
	updated_at: string;
	message_count: number;
	metadata?: Record<string, unknown>;
}

export interface ThreadSummary {
	id: string;
	title: string | null;
	created_at: string;
	updated_at: string;
	message_count: number;
	last_message_preview?: string;
}

export interface MessageSnapshot {
	id: string;
	role: 'user' | 'assistant' | 'system' | 'tool';
	content: string;
	created_at: string;
	tool_calls?: ToolCall[];
	tool_call_id?: string;
}

export interface ToolCall {
	id: string;
	name: string;
	arguments: string;
}

export interface LlmResponse {
	id: string;
	model: string;
	content: string;
	tool_calls?: ToolCall[];
	usage?: {
		prompt_tokens: number;
		completion_tokens: number;
		total_tokens: number;
	};
	finish_reason: 'stop' | 'tool_calls' | 'length' | 'content_filter' | null;
}

export type AgentStateKind =
	| 'idle'
	| 'thinking'
	| 'streaming'
	| 'tool_pending'
	| 'tool_executing'
	| 'waiting_input'
	| 'error';

export interface ToolExecutionStatus {
	call_id: string;
	tool_name: string;
	status: 'pending' | 'running' | 'completed' | 'failed';
	started_at?: string;
	completed_at?: string;
	result?: unknown;
	error?: string;
}

export interface ToolProgress {
	call_id: string;
	progress: number;
	message?: string;
}

export interface ToolExecutionOutcome {
	call_id: string;
	success: boolean;
	result?: unknown;
	error?: string;
}

export interface CurrentUser {
	id: string;
	display_name: string;
	email: string | null;
	avatar_url: string | null;
	locale: string | null;
	global_roles: string[];
	created_at: string;
}

// Organization types
export type OrgVisibility = 'public' | 'unlisted' | 'private';
export type OrgRole = 'owner' | 'admin' | 'member';
export type OrgJoinPolicy = 'open' | 'request' | 'invite_only';

export interface Org {
	id: string;
	name: string;
	slug: string;
	visibility: OrgVisibility;
	join_policy: OrgJoinPolicy;
	is_personal: boolean;
	created_at: string;
	updated_at: string;
	member_count: number | null;
}

export interface ListOrgsResponse {
	orgs: Org[];
}

export interface CreateOrgRequest {
	name: string;
	slug: string;
	visibility?: OrgVisibility;
}

export interface UpdateOrgRequest {
	name?: string;
	visibility?: OrgVisibility;
	join_policy?: OrgJoinPolicy;
}

export interface OrgMember {
	user_id: string;
	display_name: string;
	email: string | null;
	avatar_url: string | null;
	role: OrgRole;
	joined_at: string;
}

export interface OrgMemberListResponse {
	members: OrgMember[];
}

// Team types
export type TeamRole = 'maintainer' | 'member';

export interface Team {
	id: string;
	org_id: string;
	name: string;
	slug: string;
	created_at: string;
	updated_at: string;
	member_count: number;
}

export interface TeamMember {
	user_id: string;
	display_name: string;
	email: string | null;
	avatar_url: string | null;
	role: TeamRole;
	joined_at: string;
}

export interface TeamListResponse {
	teams: Team[];
}

export interface TeamMemberListResponse {
	members: TeamMember[];
}

export interface CreateTeamRequest {
	name: string;
	slug: string;
}

export interface UpdateTeamRequest {
	name?: string;
}

// API Key types
export interface ApiKey {
	id: string;
	name: string;
	prefix: string;
	scopes: string[];
	created_at: string;
	last_used_at: string | null;
	created_by: string;
}

export interface ApiKeyListResponse {
	api_keys: ApiKey[];
}

export interface CreateApiKeyRequest {
	name: string;
	scopes: string[];
}

export interface CreateApiKeyResponse {
	id: string;
	name: string;
	key: string;
	prefix: string;
	scopes: string[];
	created_at: string;
}

// API request/response types
export interface ListParams {
	workspace?: string;
	limit?: number;
	offset?: number;
}

export interface SearchParams {
	workspace?: string;
	limit?: number;
	offset?: number;
}

export interface ListResponse {
	threads: ThreadSummary[];
	total: number;
	limit: number;
	offset: number;
}

export interface SearchResponse {
	hits: SearchHit[];
	limit: number;
	offset: number;
}

export interface SearchHit {
	id: string;
	title: string | null;
	score: number;
	created_at: string;
	updated_at: string;
}

export type ThreadVisibility = 'public' | 'private' | 'unlisted';

// Auth types
export interface AuthProvidersResponse {
	providers: string[];
}

export interface AuthSuccessResponse {
	message: string;
}

export interface MagicLinkRequest {
	email: string;
}

export interface DeviceCodeStartResponse {
	device_code: string;
	user_code: string;
	verification_url: string;
	expires_in: number;
	interval: number;
}

export type DeviceCodePollStatus = 'pending' | 'completed' | 'expired' | 'denied';

export interface DeviceCodePollResponse {
	status: DeviceCodePollStatus;
	access_token?: string;
}

export interface DeviceCodeCompleteRequest {
	user_code: string;
}

export interface WsTokenResponse {
	token: string;
	expires_in: number;
}

export interface Session {
	id: string;
	session_type: 'web' | 'cli' | 'vscode';
	created_at: string;
	last_used_at: string;
	ip_address: string | null;
	user_agent: string | null;
	geo_location: string | null;
	is_current: boolean;
}

export interface SessionListResponse {
	sessions: Session[];
}

export interface UpdateProfileRequest {
	display_name?: string;
	username?: string;
	locale?: string;
}

// Impersonation types
export interface ImpersonationState {
	is_impersonating: boolean;
	original_user?: {
		id: string;
		display_name: string;
	};
	impersonated_user?: {
		id: string;
		display_name: string;
	};
}

export interface ImpersonateResponse {
	message: string;
	impersonated_user: {
		id: string;
		display_name: string;
	};
}

export interface StopImpersonationResponse {
	message: string;
}

// Admin user list types
export interface AdminUser {
	id: string;
	display_name: string;
	primary_email: string | null;
	avatar_url: string | null;
	is_system_admin: boolean;
	is_support: boolean;
	is_auditor: boolean;
	created_at: string;
	updated_at: string;
	deleted_at: string | null;
}

export interface AdminUserListResponse {
	users: AdminUser[];
	total: number;
	limit: number;
	offset: number;
}

// Admin role update types
export interface UpdateUserRolesRequest {
	is_system_admin?: boolean;
	is_support?: boolean;
	is_auditor?: boolean;
}

export interface UpdateUserRolesResponse {
	id: string;
	display_name: string;
	primary_email: string | null;
	avatar_url: string | null;
	is_system_admin: boolean;
	is_support: boolean;
	is_auditor: boolean;
	created_at: string;
	updated_at: string;
	deleted_at: string | null;
}

export interface DeleteUserResponse {
	message: string;
	user_id: string;
}

// Weaver types
export type WeaverStatus = 'pending' | 'running' | 'succeeded' | 'failed' | 'terminating';

export interface Weaver {
	id: string;
	pod_name: string;
	status: WeaverStatus;
	created_at: string;
	image?: string;
	tags?: Record<string, string>;
	lifetime_hours?: number;
	age_hours?: number;
	owner_user_id?: string;
}

export interface ListWeaversResponse {
	weavers: Weaver[];
	count: number;
}

export interface CreateWeaverRequest {
	image: string;
	org_id: string;
	env?: Record<string, string>;
	resources?: {
		memory_limit?: string;
		cpu_limit?: string;
	};
	tags?: Record<string, string>;
	lifetime_hours?: number;
	command?: string[];
	args?: string[];
	workdir?: string;
}

// Support Access types
export type SupportAccessStatus = 'pending' | 'approved' | 'revoked' | 'expired';

export interface SupportAccessRequest {
	request_id: string;
	thread_id: string;
	requested_at: string;
	status: SupportAccessStatus;
}

export interface SupportAccessApproval {
	thread_id: string;
	granted_to: string;
	approved_at: string;
	expires_at: string;
}

export interface SupportAccessResponse {
	message: string;
}

export interface SupportAccessErrorResponse {
	message: string;
	code: string;
}

// Health check types
export type HealthStatus = 'healthy' | 'degraded' | 'unhealthy' | 'unknown';

export interface DatabaseHealth {
	status: HealthStatus;
	latency_ms: number;
	error?: string;
}

export interface BinDirHealth {
	status: HealthStatus;
	latency_ms: number;
	path: string;
	exists: boolean;
	is_dir: boolean;
	file_count?: number;
	error?: string;
}

export interface AnthropicAccountHealth {
	id: string;
	status: 'available' | 'cooling_down' | 'disabled';
	cooldown_remaining_secs?: number;
	last_error?: string;
}

export interface AnthropicPoolHealth {
	accounts_total: number;
	accounts_available: number;
	accounts_cooling: number;
	accounts_disabled: number;
	accounts: AnthropicAccountHealth[];
}

export interface LlmProviderHealth {
	name: string;
	status: HealthStatus;
	mode?: string;
	pool?: AnthropicPoolHealth;
	latency_ms?: number;
	error?: string;
}

export interface LlmProvidersHealth {
	status: HealthStatus;
	providers: LlmProviderHealth[];
}

export interface GoogleCseHealth {
	status: HealthStatus;
	latency_ms: number;
	configured: boolean;
	error?: string;
}

export interface SerperHealth {
	status: HealthStatus;
	latency_ms: number;
	configured: boolean;
	error?: string;
}

export interface GithubAppHealth {
	status: HealthStatus;
	latency_ms: number;
	configured: boolean;
	error?: string;
}

export interface KubernetesHealth {
	status: HealthStatus;
	latency_ms: number;
	namespace: string;
	reachable: boolean;
	error?: string;
}

export interface SmtpHealth {
	status: HealthStatus;
	latency_ms: number;
	configured: boolean;
	healthy: boolean;
	error?: string;
}

export interface GeoIpHealth {
	status: HealthStatus;
	latency_ms: number;
	configured: boolean;
	healthy: boolean;
	database_path?: string;
	database_type?: string;
	error?: string;
}

export interface JobsHealth {
	status: HealthStatus;
	jobs_total: number;
	jobs_healthy: number;
	jobs_failing: number;
	failing_jobs?: string[];
}

export interface AuthProviderHealth {
	name: string;
	status: HealthStatus;
	configured: boolean;
	error?: string;
}

export interface AuthProvidersHealth {
	status: HealthStatus;
	providers: AuthProviderHealth[];
}

export interface HealthComponents {
	database: DatabaseHealth;
	bin_dir: BinDirHealth;
	llm_providers: LlmProvidersHealth;
	google_cse: GoogleCseHealth;
	serper: SerperHealth;
	github_app: GithubAppHealth;
	kubernetes?: KubernetesHealth;
	smtp: SmtpHealth;
	geoip: GeoIpHealth;
	jobs?: JobsHealth;
	auth_providers: AuthProvidersHealth;
}

export interface HealthVersionInfo {
	git_sha: string;
}

export interface HealthResponse {
	status: HealthStatus;
	timestamp: string;
	duration_ms: number;
	version: HealthVersionInfo;
	components: HealthComponents;
}

// Server log types
export type LogLevel = 'trace' | 'debug' | 'info' | 'warn' | 'error';

export interface LogEntry {
	id: number;
	timestamp: string;
	level: LogLevel;
	target: string;
	message: string;
	fields?: [string, string][];
}

export interface ListLogsResponse {
	entries: LogEntry[];
	buffer_size: number;
	buffer_capacity: number;
	current_id: number;
}

// Error class for API errors
export class ApiError extends Error {
	constructor(
		public readonly status: number,
		public readonly body: string
	) {
		super(`API Error ${status}: ${body}`);
		this.name = 'ApiError';
	}

	get statusCode(): number {
		return this.status;
	}

	get isForbidden(): boolean {
		return this.status === 403;
	}

	get isNotFound(): boolean {
		return this.status === 404;
	}

	getErrorCode(): string | null {
		try {
			const parsed = JSON.parse(this.body);
			return parsed.code || null;
		} catch {
			return null;
		}
	}
}
