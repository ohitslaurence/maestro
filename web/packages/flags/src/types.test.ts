/**
 * Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
 * SPDX-License-Identifier: Proprietary
 */

import { describe, it, expect } from 'vitest';
import { fc, test as fcTest } from '@fast-check/vitest';
import {
	createEvaluationContext,
	getVariantBool,
	getVariantString,
	getVariantJson,
	type VariantValue
} from './types';

describe('createEvaluationContext', () => {
	it('should create context with environment', () => {
		const ctx = createEvaluationContext('prod');
		expect(ctx.environment).toBe('prod');
		expect(ctx.attributes).toEqual({});
	});

	it('should create context with all options', () => {
		const ctx = createEvaluationContext('prod', {
			userId: 'user123',
			orgId: 'org456',
			sessionId: 'sess789',
			attributes: { plan: 'enterprise' },
			geo: { country: 'US' }
		});

		expect(ctx.environment).toBe('prod');
		expect(ctx.userId).toBe('user123');
		expect(ctx.orgId).toBe('org456');
		expect(ctx.sessionId).toBe('sess789');
		expect(ctx.attributes).toEqual({ plan: 'enterprise' });
		expect(ctx.geo?.country).toBe('US');
	});
});

describe('getVariantBool', () => {
	it('should return boolean value for boolean variant', () => {
		const value: VariantValue = { type: 'boolean', value: true };
		expect(getVariantBool(value, false)).toBe(true);
	});

	it('should return default for non-boolean variant', () => {
		const value: VariantValue = { type: 'string', value: 'hello' };
		expect(getVariantBool(value, false)).toBe(false);
		expect(getVariantBool(value, true)).toBe(true);
	});
});

describe('getVariantString', () => {
	it('should return string value for string variant', () => {
		const value: VariantValue = { type: 'string', value: 'hello' };
		expect(getVariantString(value, 'default')).toBe('hello');
	});

	it('should return default for non-string variant', () => {
		const value: VariantValue = { type: 'boolean', value: true };
		expect(getVariantString(value, 'default')).toBe('default');
	});
});

describe('getVariantJson', () => {
	it('should return json value for json variant', () => {
		const value: VariantValue = { type: 'json', value: { foo: 'bar' } };
		expect(getVariantJson(value, {})).toEqual({ foo: 'bar' });
	});

	it('should return default for non-json variant', () => {
		const value: VariantValue = { type: 'boolean', value: true };
		expect(getVariantJson(value, { default: true })).toEqual({ default: true });
	});
});

// Property-based tests
describe('getVariant* property tests', () => {
	fcTest.prop([fc.boolean()])('getVariantBool should extract any boolean', (b) => {
		const value: VariantValue = { type: 'boolean', value: b };
		expect(getVariantBool(value, !b)).toBe(b);
	});

	fcTest.prop([fc.string()])('getVariantString should extract any string', (s) => {
		const value: VariantValue = { type: 'string', value: s };
		expect(getVariantString(value, 'default')).toBe(s);
	});

	fcTest.prop([fc.json()])('getVariantJson should extract any JSON', (json) => {
		const value: VariantValue = { type: 'json', value: json };
		expect(getVariantJson(value, null)).toEqual(json);
	});

	fcTest.prop([fc.boolean(), fc.boolean()])(
		'getVariantBool should return default for non-boolean',
		(defaultVal, _ignored) => {
			const stringValue: VariantValue = { type: 'string', value: 'test' };
			const jsonValue: VariantValue = { type: 'json', value: { test: true } };

			expect(getVariantBool(stringValue, defaultVal)).toBe(defaultVal);
			expect(getVariantBool(jsonValue, defaultVal)).toBe(defaultVal);
		}
	);

	fcTest.prop([fc.string()])(
		'getVariantString should return default for non-string',
		(defaultVal) => {
			const boolValue: VariantValue = { type: 'boolean', value: true };
			const jsonValue: VariantValue = { type: 'json', value: { test: true } };

			expect(getVariantString(boolValue, defaultVal)).toBe(defaultVal);
			expect(getVariantString(jsonValue, defaultVal)).toBe(defaultVal);
		}
	);
});
