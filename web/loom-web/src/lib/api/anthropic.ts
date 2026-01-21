/**
 * Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
 * SPDX-License-Identifier: Proprietary
 */

import { getApiClient } from './client';

export type AnthropicAccountStatus = 'available' | 'cooling_down' | 'disabled';

export interface AnthropicAccount {
	id: string;
	status: AnthropicAccountStatus;
	cooldown_remaining_secs?: number;
	last_error?: string;
	expires_at?: string;
}

export interface AccountsSummary {
	total: number;
	available: number;
	cooling_down: number;
	disabled: number;
}

export interface AnthropicAccountsResponse {
	accounts: AnthropicAccount[];
	summary: AccountsSummary;
}

export interface InitiateOAuthResponse {
	redirect_url: string;
	state: string;
}

export interface AddAccountResponse {
	account_id: string;
}

export async function listAnthropicAccounts(): Promise<AnthropicAccountsResponse> {
	const response = await fetch('/api/admin/anthropic/accounts', {
		headers: { 'Content-Type': 'application/json' },
	});
	if (!response.ok) {
		throw new Error(`Failed to list accounts: ${response.status}`);
	}
	return response.json();
}

export async function initiateAnthropicOAuth(redirectAfter?: string): Promise<InitiateOAuthResponse> {
	const response = await fetch('/api/admin/anthropic/oauth/initiate', {
		method: 'POST',
		headers: { 'Content-Type': 'application/json' },
		body: JSON.stringify({ redirect_after: redirectAfter }),
	});
	if (!response.ok) {
		throw new Error(`Failed to initiate OAuth: ${response.status}`);
	}
	return response.json();
}

export async function completeAnthropicOAuth(code: string, state: string): Promise<AddAccountResponse> {
	const response = await fetch('/api/admin/anthropic/oauth/complete', {
		method: 'POST',
		headers: { 'Content-Type': 'application/json' },
		body: JSON.stringify({ code, state }),
	});
	if (!response.ok) {
		const errorData = await response.json().catch(() => ({}));
		throw new Error(errorData.message || `Failed to complete OAuth: ${response.status}`);
	}
	return response.json();
}

export async function removeAnthropicAccount(accountId: string): Promise<void> {
	const response = await fetch(`/api/admin/anthropic/accounts/${encodeURIComponent(accountId)}`, {
		method: 'DELETE',
		headers: { 'Content-Type': 'application/json' },
	});
	if (!response.ok) {
		throw new Error(`Failed to remove account: ${response.status}`);
	}
}
