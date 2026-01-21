/**
 * Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
 * SPDX-License-Identifier: Proprietary
 */

/**
 * Local in-memory cache for feature flag states.
 *
 * The cache stores the current state of all flags and kill switches,
 * enabling fast local evaluation and offline mode support.
 */

import type { FlagState, KillSwitchState, VariantValue } from './types';

/**
 * In-memory cache for feature flag states.
 */
export class FlagCache {
	private flags: Map<string, FlagState> = new Map();
	private killSwitches: Map<string, KillSwitchState> = new Map();
	private lastUpdated: Date | null = null;
	private _initialized = false;

	/**
	 * Returns true if the cache has been initialized with data.
	 */
	isInitialized(): boolean {
		return this._initialized;
	}

	/**
	 * Returns the timestamp of the last cache update.
	 */
	getLastUpdated(): Date | null {
		return this.lastUpdated;
	}

	/**
	 * Initializes the cache with a full set of flags and kill switches.
	 *
	 * This is typically called when receiving an `init` event from SSE.
	 */
	initialize(flags: FlagState[], killSwitches: KillSwitchState[]): void {
		this.flags.clear();
		for (const flag of flags) {
			this.flags.set(flag.key, flag);
		}

		this.killSwitches.clear();
		for (const ks of killSwitches) {
			this.killSwitches.set(ks.key, ks);
		}

		this.lastUpdated = new Date();
		this._initialized = true;
	}

	/**
	 * Gets a flag state by key.
	 */
	getFlag(key: string): FlagState | undefined {
		return this.flags.get(key);
	}

	/**
	 * Gets all cached flag states.
	 */
	getAllFlags(): FlagState[] {
		return Array.from(this.flags.values());
	}

	/**
	 * Updates a single flag state.
	 */
	updateFlag(flag: FlagState): void {
		this.flags.set(flag.key, flag);
		this.lastUpdated = new Date();
	}

	/**
	 * Marks a flag as archived.
	 */
	archiveFlag(key: string): void {
		const flag = this.flags.get(key);
		if (flag) {
			this.flags.set(key, { ...flag, archived: true });
			this.lastUpdated = new Date();
		}
	}

	/**
	 * Marks a flag as restored (unarchived) with updated enabled status.
	 */
	restoreFlag(key: string, enabled: boolean): void {
		const flag = this.flags.get(key);
		if (flag) {
			this.flags.set(key, { ...flag, archived: false, enabled });
			this.lastUpdated = new Date();
		}
	}

	/**
	 * Updates the enabled status of a flag.
	 */
	updateFlagEnabled(key: string, enabled: boolean): void {
		const flag = this.flags.get(key);
		if (flag) {
			this.flags.set(key, { ...flag, enabled });
			this.lastUpdated = new Date();
		}
	}

	/**
	 * Updates a flag with new variant information.
	 */
	updateFlagVariant(key: string, defaultVariant: string, defaultValue: VariantValue): void {
		const flag = this.flags.get(key);
		if (flag) {
			this.flags.set(key, { ...flag, defaultVariant, defaultValue });
			this.lastUpdated = new Date();
		}
	}

	/**
	 * Gets a kill switch state by key.
	 */
	getKillSwitch(key: string): KillSwitchState | undefined {
		return this.killSwitches.get(key);
	}

	/**
	 * Gets all cached kill switch states.
	 */
	getAllKillSwitches(): KillSwitchState[] {
		return Array.from(this.killSwitches.values());
	}

	/**
	 * Activates a kill switch.
	 */
	activateKillSwitch(key: string, reason: string): void {
		const ks = this.killSwitches.get(key);
		if (ks) {
			this.killSwitches.set(key, { ...ks, isActive: true, activationReason: reason });
			this.lastUpdated = new Date();
		}
	}

	/**
	 * Deactivates a kill switch.
	 */
	deactivateKillSwitch(key: string): void {
		const ks = this.killSwitches.get(key);
		if (ks) {
			this.killSwitches.set(key, { ...ks, isActive: false, activationReason: undefined });
			this.lastUpdated = new Date();
		}
	}

	/**
	 * Checks if any active kill switch affects the given flag.
	 * Returns the kill switch key if found, undefined otherwise.
	 */
	isFlagKilled(flagKey: string): string | undefined {
		for (const ks of this.killSwitches.values()) {
			if (ks.isActive && ks.linkedFlagKeys.includes(flagKey)) {
				return ks.key;
			}
		}
		return undefined;
	}

	/**
	 * Returns the number of cached flags.
	 */
	flagCount(): number {
		return this.flags.size;
	}

	/**
	 * Returns the number of cached kill switches.
	 */
	killSwitchCount(): number {
		return this.killSwitches.size;
	}

	/**
	 * Clears all cached data.
	 */
	clear(): void {
		this.flags.clear();
		this.killSwitches.clear();
		this.lastUpdated = null;
		this._initialized = false;
	}
}
