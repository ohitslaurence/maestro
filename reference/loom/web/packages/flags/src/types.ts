/**
 * Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
 * SPDX-License-Identifier: Proprietary
 */

/**
 * Types for the feature flags SDK.
 * These types mirror the Rust types in loom-flags-core.
 */

/**
 * Context passed by SDK for flag evaluation.
 */
export interface EvaluationContext {
	userId?: string;
	orgId?: string;
	sessionId?: string;
	environment: string;
	attributes: Record<string, unknown>;
	geo?: GeoContext;
}

/**
 * GeoIP context resolved from client IP.
 */
export interface GeoContext {
	country?: string;
	region?: string;
	city?: string;
}

/**
 * Value of a variant.
 */
export type VariantValue =
	| { type: 'boolean'; value: boolean }
	| { type: 'string'; value: string }
	| { type: 'json'; value: unknown };

/**
 * Result of evaluating a feature flag.
 */
export interface EvaluationResult {
	flagKey: string;
	variant: string;
	value: VariantValue;
	reason: EvaluationReason;
}

/**
 * The reason for an evaluation result.
 */
export type EvaluationReason =
	| { type: 'Default' }
	| { type: 'Strategy'; strategyId: string }
	| { type: 'KillSwitch'; killSwitchId: string }
	| { type: 'Prerequisite'; missingFlag: string }
	| { type: 'Disabled' }
	| { type: 'Error'; message: string };

/**
 * Bulk evaluation results for all flags.
 */
export interface BulkEvaluationResult {
	results: EvaluationResult[];
	evaluatedAt: string;
}

/**
 * Compact representation of a flag's current state.
 */
export interface FlagState {
	key: string;
	id: string;
	enabled: boolean;
	defaultVariant: string;
	defaultValue: VariantValue;
	archived: boolean;
}

/**
 * Compact representation of a kill switch's current state.
 */
export interface KillSwitchState {
	key: string;
	id: string;
	isActive: boolean;
	linkedFlagKeys: string[];
	activationReason?: string;
}

/**
 * SSE event types for flag streaming.
 */
export type FlagStreamEvent =
	| { event: 'init'; data: InitData }
	| { event: 'flag.updated'; data: FlagUpdatedData }
	| { event: 'flag.archived'; data: FlagArchivedData }
	| { event: 'flag.restored'; data: FlagRestoredData }
	| { event: 'killswitch.activated'; data: KillSwitchActivatedData }
	| { event: 'killswitch.deactivated'; data: KillSwitchDeactivatedData }
	| { event: 'heartbeat'; data: HeartbeatData };

/**
 * Initial state data sent on SSE connection.
 */
export interface InitData {
	flags: FlagState[];
	killSwitches: KillSwitchState[];
	timestamp: string;
}

/**
 * Data for flag.updated event.
 */
export interface FlagUpdatedData {
	flagKey: string;
	environment: string;
	enabled: boolean;
	defaultVariant: string;
	defaultValue: VariantValue;
	timestamp: string;
}

/**
 * Data for flag.archived event.
 */
export interface FlagArchivedData {
	flagKey: string;
	timestamp: string;
}

/**
 * Data for flag.restored event.
 */
export interface FlagRestoredData {
	flagKey: string;
	environment: string;
	enabled: boolean;
	timestamp: string;
}

/**
 * Data for killswitch.activated event.
 */
export interface KillSwitchActivatedData {
	killSwitchKey: string;
	linkedFlagKeys: string[];
	reason: string;
	timestamp: string;
}

/**
 * Data for killswitch.deactivated event.
 */
export interface KillSwitchDeactivatedData {
	killSwitchKey: string;
	linkedFlagKeys: string[];
	timestamp: string;
}

/**
 * Data for heartbeat event.
 */
export interface HeartbeatData {
	timestamp: string;
}

/**
 * Helper functions for working with types.
 */
export function createEvaluationContext(
	environment: string,
	options?: Partial<Omit<EvaluationContext, 'environment'>>
): EvaluationContext {
	return {
		environment,
		attributes: {},
		...options
	};
}

/**
 * Get boolean value from VariantValue.
 */
export function getVariantBool(value: VariantValue, defaultValue: boolean): boolean {
	if (value.type === 'boolean') {
		return value.value;
	}
	return defaultValue;
}

/**
 * Get string value from VariantValue.
 */
export function getVariantString(value: VariantValue, defaultValue: string): string {
	if (value.type === 'string') {
		return value.value;
	}
	return defaultValue;
}

/**
 * Get JSON value from VariantValue.
 */
export function getVariantJson<T>(value: VariantValue, defaultValue: T): T {
	if (value.type === 'json') {
		return value.value as T;
	}
	return defaultValue;
}
