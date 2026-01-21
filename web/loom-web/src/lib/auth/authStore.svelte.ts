/**
 * Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
 * SPDX-License-Identifier: Proprietary
 */

import { createActor } from 'xstate';
import { authMachine } from './authMachine';
import type { CurrentUser } from '$lib/api/types';

class AuthStore {
	private actor = createActor(authMachine);

	user = $state<CurrentUser | null>(null);
	isAuthenticated = $derived(this.user !== null);
	isLoading = $state(false);
	error = $state<string | null>(null);

	constructor() {
		this.actor.subscribe((snapshot) => {
			this.user = snapshot.context.user;
			this.error = snapshot.context.error;
			this.isLoading = snapshot.matches('loading') || snapshot.matches('loggingOut');
		});
	}

	start() {
		this.actor.start();
	}

	init() {
		this.actor.send({ type: 'INIT' });
	}

	refresh() {
		this.actor.send({ type: 'REFRESH' });
	}

	logout() {
		this.actor.send({ type: 'LOGOUT' });
	}

	loginSuccess(user: CurrentUser) {
		this.actor.send({ type: 'LOGIN_SUCCESS', user });
	}
}

export const authStore = new AuthStore();
