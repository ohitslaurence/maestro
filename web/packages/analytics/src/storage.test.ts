/**
 * Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
 * SPDX-License-Identifier: Proprietary
 */

import { describe, it, expect, beforeEach, vi } from 'vitest';
import { fc, test as fcTest } from '@fast-check/vitest';
import {
	generateDistinctId,
	isValidDistinctId,
	MemoryStorage,
	CombinedStorage,
	DistinctIdManager,
	createStorage
} from './storage';

describe('generateDistinctId', () => {
	it('should generate a valid UUIDv7 format', () => {
		const id = generateDistinctId();

		// UUIDv7 format: xxxxxxxx-xxxx-7xxx-yxxx-xxxxxxxxxxxx
		const uuidPattern = /^[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i;
		expect(id).toMatch(uuidPattern);
	});

	it('should generate unique IDs', () => {
		const ids = new Set<string>();
		for (let i = 0; i < 100; i++) {
			ids.add(generateDistinctId());
		}
		expect(ids.size).toBe(100);
	});

	it('should generate time-ordered IDs', () => {
		const id1 = generateDistinctId();
		const id2 = generateDistinctId();

		// The first 12 hex chars represent the timestamp
		const timestamp1 = id1.replace(/-/g, '').slice(0, 12);
		const timestamp2 = id2.replace(/-/g, '').slice(0, 12);

		// Second ID should have equal or greater timestamp
		expect(parseInt(timestamp2, 16)).toBeGreaterThanOrEqual(parseInt(timestamp1, 16));
	});
});

describe('isValidDistinctId', () => {
	it('should return true for valid distinct IDs', () => {
		expect(isValidDistinctId('user123')).toBe(true);
		expect(isValidDistinctId('user@example.com')).toBe(true);
		expect(isValidDistinctId(generateDistinctId())).toBe(true);
		expect(isValidDistinctId('a'.repeat(200))).toBe(true);
	});

	it('should return false for invalid distinct IDs', () => {
		expect(isValidDistinctId('')).toBe(false);
		expect(isValidDistinctId('   ')).toBe(false);
		expect(isValidDistinctId('a'.repeat(201))).toBe(false);
		expect(isValidDistinctId(null as unknown as string)).toBe(false);
		expect(isValidDistinctId(undefined as unknown as string)).toBe(false);
		expect(isValidDistinctId(123 as unknown as string)).toBe(false);
	});

	fcTest.prop([fc.string({ minLength: 1, maxLength: 200 })])(
		'should return true for any non-empty string up to 200 chars',
		(str) => {
			// Only valid if not all whitespace
			const expected = str.trim().length > 0;
			expect(isValidDistinctId(str)).toBe(expected);
		}
	);
});

describe('MemoryStorage', () => {
	let storage: MemoryStorage;

	beforeEach(() => {
		storage = new MemoryStorage();
	});

	it('should return null initially', () => {
		expect(storage.get()).toBeNull();
	});

	it('should store and retrieve distinct_id', () => {
		storage.set('test123');
		expect(storage.get()).toBe('test123');
	});

	it('should clear the stored value', () => {
		storage.set('test123');
		storage.clear();
		expect(storage.get()).toBeNull();
	});

	fcTest.prop([fc.string({ minLength: 1, maxLength: 200 })])(
		'should store and retrieve any string',
		(str) => {
			storage.set(str);
			expect(storage.get()).toBe(str);
		}
	);
});

describe('CombinedStorage', () => {
	it('should read from first storage that has a value', () => {
		const storage1 = new MemoryStorage();
		const storage2 = new MemoryStorage();
		const combined = new CombinedStorage([storage1, storage2]);

		storage2.set('from_storage2');
		expect(combined.get()).toBe('from_storage2');

		storage1.set('from_storage1');
		expect(combined.get()).toBe('from_storage1');
	});

	it('should write to all storages', () => {
		const storage1 = new MemoryStorage();
		const storage2 = new MemoryStorage();
		const combined = new CombinedStorage([storage1, storage2]);

		combined.set('test123');
		expect(storage1.get()).toBe('test123');
		expect(storage2.get()).toBe('test123');
	});

	it('should clear all storages', () => {
		const storage1 = new MemoryStorage();
		const storage2 = new MemoryStorage();
		const combined = new CombinedStorage([storage1, storage2]);

		combined.set('test123');
		combined.clear();
		expect(storage1.get()).toBeNull();
		expect(storage2.get()).toBeNull();
	});
});

describe('DistinctIdManager', () => {
	let storage: MemoryStorage;
	let manager: DistinctIdManager;

	beforeEach(() => {
		storage = new MemoryStorage();
		manager = new DistinctIdManager(storage);
	});

	it('should generate a new distinct_id on initialize if none exists', () => {
		const id = manager.initialize();

		const uuidPattern = /^[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i;
		expect(id).toMatch(uuidPattern);
		expect(storage.get()).toBe(id);
	});

	it('should use existing distinct_id on initialize', () => {
		storage.set('existing123');
		const id = manager.initialize();
		expect(id).toBe('existing123');
	});

	it('should return distinct_id via getDistinctId', () => {
		manager.initialize();
		const id1 = manager.getDistinctId();
		const id2 = manager.getDistinctId();
		expect(id1).toBe(id2);
	});

	it('should set a new distinct_id', () => {
		manager.initialize();
		manager.setDistinctId('user@example.com');
		expect(manager.getDistinctId()).toBe('user@example.com');
		expect(storage.get()).toBe('user@example.com');
	});

	it('should throw on invalid distinct_id', () => {
		manager.initialize();
		expect(() => manager.setDistinctId('')).toThrow('Invalid distinct_id');
		expect(() => manager.setDistinctId('a'.repeat(201))).toThrow('Invalid distinct_id');
	});

	it('should reset to a new anonymous distinct_id', () => {
		manager.initialize();
		const oldId = manager.getDistinctId();

		const newId = manager.reset();
		expect(newId).not.toBe(oldId);
		expect(manager.getDistinctId()).toBe(newId);
	});

	it('should identify anonymous vs identified distinct_ids', () => {
		manager.initialize();
		expect(manager.isAnonymous()).toBe(true);

		manager.setDistinctId('user@example.com');
		expect(manager.isAnonymous()).toBe(false);

		manager.reset();
		expect(manager.isAnonymous()).toBe(true);
	});
});

describe('createStorage', () => {
	it('should create memory storage for "memory" mode', () => {
		const storage = createStorage('memory');
		expect(storage).toBeInstanceOf(MemoryStorage);
	});

	it('should create combined storage for "localStorage+cookie" mode', () => {
		const storage = createStorage('localStorage+cookie');
		expect(storage).toBeInstanceOf(CombinedStorage);
	});
});
