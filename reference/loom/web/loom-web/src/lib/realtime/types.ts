/**
 * Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
 * SPDX-License-Identifier: Proprietary
 */

import type { LlmResponse } from '../api/types';

// Connection status
export type ConnectionStatus =
	| 'disconnected'
	| 'connecting'
	| 'connected'
	| 'reconnecting'
	| 'error';

// LLM event types from server
export type LlmEventType = 'text_delta' | 'tool_call_delta' | 'completed' | 'error';

// LLM event wire format
export interface LlmEventWire {
	event_type: LlmEventType;
	content?: string;
	call_id?: string;
	tool_name?: string;
	arguments_fragment?: string;
	response?: LlmResponse;
	error?: string;
}

// Control command types
export type ControlCommand = 'ping' | 'pong' | 'close' | 'resume';

// Control message wire format
export interface ControlWire {
	command: ControlCommand;
	reason?: string;
	payload?: unknown;
}

// Ack wire format
export interface AckWire {
	acked_at: string;
	sequence?: number;
}

// Server query wire format (for future use)
export interface ServerQueryWire {
	kind: unknown;
	sent_at: string;
	timeout_secs: number;
	metadata?: Record<string, unknown>;
}

// Query response wire format
export interface QueryResponseWire {
	result?: unknown;
	error?: string;
	responded_at: string;
}

// Tool event types from server
export type ToolEventType = 'tool_start' | 'tool_progress' | 'tool_output' | 'tool_done' | 'tool_error';

// Tool event wire format
export interface ToolEventWire {
	event_type: ToolEventType;
	call_id: string;
	tool_name?: string;
	progress?: number;
	message?: string;
	output?: unknown;
	error?: string;
}

// All realtime message types
export type RealtimeMessage =
	| { type: 'llm_event'; id: string; data: LlmEventWire; timestamp: string }
	| { type: 'tool_event'; id: string; data: ToolEventWire; timestamp: string }
	| { type: 'server_query'; id: string; data: ServerQueryWire; timestamp: string }
	| { type: 'query_response'; id: string; data: QueryResponseWire; timestamp: string }
	| { type: 'control'; id: string; data: ControlWire; timestamp: string }
	| { type: 'ack'; id: string; data: AckWire; timestamp: string }
	| { type: 'user_message'; content: string; timestamp: string };

// Parsed LLM event for UI consumption
export type LlmEvent =
	| { type: 'text_delta'; content: string }
	| { type: 'tool_call_delta'; callId: string; toolName: string; argsFragment: string }
	| { type: 'completed'; response: LlmResponse }
	| { type: 'error'; error: string };

// Parsed Tool event for UI consumption
export interface ToolEvent {
	type: ToolEventType;
	callId: string;
	toolName?: string;
	progress?: number;
	message?: string;
	output?: unknown;
	error?: string;
}

// Event handler types
export type MessageHandler = (msg: RealtimeMessage) => void;
export type LlmEventHandler = (event: LlmEvent) => void;
export type ToolEventHandler = (event: ToolEvent) => void;
export type StatusHandler = (status: ConnectionStatus) => void;
