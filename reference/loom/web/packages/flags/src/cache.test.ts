/**
 * Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
 * SPDX-License-Identifier: Proprietary
 */

import { describe, it, expect, beforeEach } from 'vitest';
import { fc, test as fcTest } from '@fast-check/vitest';
import { FlagCache } from './cache';
import type { FlagState, KillSwitchState, VariantValue } from './types';

function createTestFlag(key: string, enabled: boolean): FlagState {
	return {
		key,
		id: `flag-${key}`,
		enabled,
		defaultVariant: 'on',
		defaultValue: { type: 'boolean', value: true },
		archived: false
	};
}

function createTestKillSwitch(
	key: string,
	linkedKeys: string[],
	active: boolean
): KillSwitchState {
	return {
		key,
		id: `ks-${key}`,
		isActive: active,
		linkedFlagKeys: linkedKeys,
		activationReason: active ? 'Testing' : undefined
	};
}

describe('FlagCache', () => {
	let cache: FlagCache;

	beforeEach(() => {
		cache = new FlagCache();
	});

	describe('initialization', () => {
		it('should start uninitialized', () => {
			expect(cache.isInitialized()).toBe(false);
			expect(cache.getLastUpdated()).toBeNull();
		});

		it('should initialize with flags and kill switches', () => {
			const flags = [
				createTestFlag('feature.test', true),
				createTestFlag('feature.other', false)
			];
			const killSwitches = [createTestKillSwitch('emergency', ['feature.test'], false)];

			cache.initialize(flags, killSwitches);

			expect(cache.isInitialized()).toBe(true);
			expect(cache.flagCount()).toBe(2);
			expect(cache.killSwitchCount()).toBe(1);
			expect(cache.getLastUpdated()).not.toBeNull();
		});
	});

	describe('flag operations', () => {
		beforeEach(() => {
			cache.initialize([createTestFlag('feature.test', true)], []);
		});

		it('should get a flag by key', () => {
			const flag = cache.getFlag('feature.test');
			expect(flag).toBeDefined();
			expect(flag?.key).toBe('feature.test');
		});

		it('should return undefined for missing flag', () => {
			expect(cache.getFlag('nonexistent')).toBeUndefined();
		});

		it('should update flag', () => {
			const updated = createTestFlag('feature.test', false);
			cache.updateFlag(updated);

			const flag = cache.getFlag('feature.test');
			expect(flag?.enabled).toBe(false);
		});

		it('should archive and restore flag', () => {
			cache.archiveFlag('feature.test');
			expect(cache.getFlag('feature.test')?.archived).toBe(true);

			cache.restoreFlag('feature.test', true);
			expect(cache.getFlag('feature.test')?.archived).toBe(false);
			expect(cache.getFlag('feature.test')?.enabled).toBe(true);
		});

		it('should update flag enabled status', () => {
			cache.updateFlagEnabled('feature.test', false);
			expect(cache.getFlag('feature.test')?.enabled).toBe(false);
		});
	});

	describe('kill switch operations', () => {
		beforeEach(() => {
			cache.initialize([], [createTestKillSwitch('emergency', ['feature.test'], false)]);
		});

		it('should get kill switch by key', () => {
			const ks = cache.getKillSwitch('emergency');
			expect(ks).toBeDefined();
			expect(ks?.key).toBe('emergency');
		});

		it('should activate kill switch', () => {
			cache.activateKillSwitch('emergency', 'System failure');
			const ks = cache.getKillSwitch('emergency');
			expect(ks?.isActive).toBe(true);
			expect(ks?.activationReason).toBe('System failure');
		});

		it('should deactivate kill switch', () => {
			cache.activateKillSwitch('emergency', 'Testing');
			cache.deactivateKillSwitch('emergency');

			const ks = cache.getKillSwitch('emergency');
			expect(ks?.isActive).toBe(false);
			expect(ks?.activationReason).toBeUndefined();
		});
	});

	describe('isFlagKilled', () => {
		it('should return kill switch key when flag is killed', () => {
			cache.initialize([], [createTestKillSwitch('emergency', ['feature.dangerous'], true)]);

			expect(cache.isFlagKilled('feature.dangerous')).toBe('emergency');
		});

		it('should return undefined when flag is not killed', () => {
			cache.initialize([], [createTestKillSwitch('emergency', ['feature.dangerous'], false)]);

			expect(cache.isFlagKilled('feature.dangerous')).toBeUndefined();
		});

		it('should return undefined for unrelated flag', () => {
			cache.initialize([], [createTestKillSwitch('emergency', ['feature.dangerous'], true)]);

			expect(cache.isFlagKilled('feature.safe')).toBeUndefined();
		});
	});

	describe('clear', () => {
		it('should clear all data', () => {
			cache.initialize([createTestFlag('test', true)], [createTestKillSwitch('ks', [], false)]);

			cache.clear();

			expect(cache.isInitialized()).toBe(false);
			expect(cache.flagCount()).toBe(0);
			expect(cache.killSwitchCount()).toBe(0);
		});
	});
});

// Property-based tests
describe('FlagCache property tests', () => {
	const arbFlagState = fc
		.record({
			key: fc.string({ minLength: 1, maxLength: 50 }).filter((s) => /^[a-z][a-z0-9_.]*$/.test(s)),
			enabled: fc.boolean(),
			archived: fc.boolean()
		})
		.map(
			({ key, enabled, archived }): FlagState => ({
				key,
				id: `flag-${key}`,
				enabled,
				defaultVariant: 'default',
				defaultValue: { type: 'boolean', value: enabled },
				archived
			})
		);

	const arbKillSwitchState = fc
		.record({
			key: fc.string({ minLength: 1, maxLength: 20 }).filter((s) => /^[a-z][a-z0-9_]*$/.test(s)),
			linkedFlagKeys: fc.array(
				fc.string({ minLength: 1, maxLength: 20 }).filter((s) => /^[a-z][a-z0-9_.]*$/.test(s)),
				{ maxLength: 5 }
			),
			isActive: fc.boolean()
		})
		.map(
			({ key, linkedFlagKeys, isActive }): KillSwitchState => ({
				key,
				id: `ks-${key}`,
				isActive,
				linkedFlagKeys,
				activationReason: isActive ? 'Testing' : undefined
			})
		);

	fcTest.prop([fc.array(arbFlagState, { minLength: 1, maxLength: 20 })])(
		'cache should preserve all flags',
		(flags) => {
			const cache = new FlagCache();
			const uniqueFlags = Array.from(new Map(flags.map((f) => [f.key, f])).values());

			cache.initialize(uniqueFlags, []);

			expect(cache.isInitialized()).toBe(true);
			expect(cache.flagCount()).toBe(uniqueFlags.length);

			for (const flag of uniqueFlags) {
				const cached = cache.getFlag(flag.key);
				expect(cached).toBeDefined();
				expect(cached?.key).toBe(flag.key);
				expect(cached?.enabled).toBe(flag.enabled);
			}
		}
	);

	fcTest.prop([fc.array(arbKillSwitchState, { minLength: 1, maxLength: 10 })])(
		'cache should preserve all kill switches',
		(killSwitches) => {
			const cache = new FlagCache();
			const uniqueKs = Array.from(new Map(killSwitches.map((k) => [k.key, k])).values());

			cache.initialize([], uniqueKs);

			expect(cache.killSwitchCount()).toBe(uniqueKs.length);

			for (const ks of uniqueKs) {
				const cached = cache.getKillSwitch(ks.key);
				expect(cached).toBeDefined();
				expect(cached?.key).toBe(ks.key);
				expect(cached?.isActive).toBe(ks.isActive);
			}
		}
	);

	fcTest.prop([
		fc.array(
			fc.string({ minLength: 1, maxLength: 20 }).filter((s) => /^[a-z][a-z0-9_.]*$/.test(s)),
			{ minLength: 1, maxLength: 10 }
		),
		fc.string({ minLength: 1, maxLength: 20 }).filter((s) => /^[a-z][a-z0-9_]*$/.test(s))
	])('active kill switch should affect all linked flags', (flagKeys, ksKey) => {
		const cache = new FlagCache();
		const uniqueFlags = [...new Set(flagKeys)];

		const ks: KillSwitchState = {
			key: ksKey,
			id: `ks-${ksKey}`,
			isActive: true,
			linkedFlagKeys: uniqueFlags,
			activationReason: 'Testing'
		};

		cache.initialize([], [ks]);

		for (const flagKey of uniqueFlags) {
			expect(cache.isFlagKilled(flagKey)).toBe(ksKey);
		}
	});

	fcTest.prop([
		fc.array(
			fc.string({ minLength: 1, maxLength: 20 }).filter((s) => /^[a-z][a-z0-9_.]*$/.test(s)),
			{ minLength: 1, maxLength: 10 }
		),
		fc.string({ minLength: 1, maxLength: 20 }).filter((s) => /^[a-z][a-z0-9_]*$/.test(s))
	])('inactive kill switch should not affect flags', (flagKeys, ksKey) => {
		const cache = new FlagCache();
		const uniqueFlags = [...new Set(flagKeys)];

		const ks: KillSwitchState = {
			key: ksKey,
			id: `ks-${ksKey}`,
			isActive: false,
			linkedFlagKeys: uniqueFlags,
			activationReason: undefined
		};

		cache.initialize([], [ks]);

		for (const flagKey of uniqueFlags) {
			expect(cache.isFlagKilled(flagKey)).toBeUndefined();
		}
	});
});
