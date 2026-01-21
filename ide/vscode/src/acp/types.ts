// Copyright (c) 2025 Geoffrey Huntley <ghuntley@ghuntley.com>. All rights reserved.
// SPDX-License-Identifier: Proprietary

// Re-export SDK types that we use
export type {
	SessionNotification,
	SessionUpdate,
	InitializeResponse,
	NewSessionResponse,
	LoadSessionResponse,
	PromptResponse,
	AgentCapabilities,
	ContentChunk,
	ToolCall,
	ToolCallUpdate,
	ToolCallStatus as SdkToolCallStatus,
	ContentBlock as SdkContentBlock,
} from '@agentclientprotocol/sdk';

export type SessionId = string;

export type StopReason =
	| 'end_turn'
	| 'max_tokens'
	| 'stop_sequence'
	| 'tool_use'
	| 'cancelled'
	| 'error';

// Content block types for prompts (simpler local types)
export interface TextBlock {
	type: 'text';
	text: string;
}

export interface ImageBlock {
	type: 'image';
	data: string;
	mimeType: string;
}

export type ContentBlock = TextBlock | ImageBlock;

// Error types
export interface AcpError {
	code: string;
	message: string;
	cause?: Error;
}

// Type guard helper types
interface SessionUpdateWithType {
	sessionUpdate: string;
}

interface AgentMessageChunkUpdate extends SessionUpdateWithType {
	sessionUpdate: 'agent_message_chunk';
	content: { type: string; text?: string };
}

interface ToolCallSessionUpdate extends SessionUpdateWithType {
	sessionUpdate: 'tool_call';
	toolCallId: string;
	title: string;
	status?: string;
}

interface ToolCallUpdateSessionUpdate extends SessionUpdateWithType {
	sessionUpdate: 'tool_call_update';
	toolCallId: string;
	status?: string | null;
}

// Type guards for session updates - using the SDK's sessionUpdate discriminator
export function isAgentMessageChunk(update: SessionUpdateWithType): update is AgentMessageChunkUpdate {
	return update.sessionUpdate === 'agent_message_chunk';
}

export function isUserMessageChunk(update: SessionUpdateWithType): boolean {
	return update.sessionUpdate === 'user_message_chunk';
}

export function isThoughtChunk(update: SessionUpdateWithType): boolean {
	return update.sessionUpdate === 'agent_thought_chunk';
}

export function isToolCall(update: SessionUpdateWithType): update is ToolCallSessionUpdate {
	return update.sessionUpdate === 'tool_call';
}

export function isToolCallUpdate(update: SessionUpdateWithType): update is ToolCallUpdateSessionUpdate {
	return update.sessionUpdate === 'tool_call_update';
}

export function isPlan(update: SessionUpdateWithType): boolean {
	return update.sessionUpdate === 'plan';
}

export function isCurrentModeUpdate(update: SessionUpdateWithType): boolean {
	return update.sessionUpdate === 'current_mode_update';
}
