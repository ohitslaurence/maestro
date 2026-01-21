/**
 * SDK Message → OpenCode Part Mapper (§3, Appendix B)
 *
 * Maps Claude SDK messages to OpenCode-compatible Part types for SSE streaming.
 * Handles text, reasoning, tool_use, and tool_result content blocks.
 */

import { randomUUID } from 'crypto';
import type {
  Part,
  ReasoningPart,
  RetryPart,
  StepFinishPart,
  StepStartPart,
  TextPart,
  TokenUsage,
  ToolPart,
  ToolStatus,
} from '../types';

// --- SDK Message Types ---

/**
 * Content block types from Claude SDK responses.
 */
export interface TextBlock {
  type: 'text';
  text: string;
}

export interface ThinkingBlock {
  type: 'thinking';
  thinking: string;
}

export interface ToolUseBlock {
  type: 'tool_use';
  id: string;
  name: string;
  input: unknown;
}

export interface ToolResultBlock {
  type: 'tool_result';
  tool_use_id: string;
  content: string | { type: 'text'; text: string }[];
  is_error?: boolean;
}

export type ContentBlock = TextBlock | ThinkingBlock | ToolUseBlock | ToolResultBlock;

/**
 * SDK assistant message structure.
 */
export interface SDKAssistantMessage {
  type: 'assistant';
  uuid: string;
  session_id: string;
  message: {
    role: 'assistant';
    content: ContentBlock[];
  };
}

/**
 * SDK result message structure.
 */
export interface SDKResultMessage {
  type: 'result';
  session_id?: string;
  subtype?: string;
  is_error?: boolean;
  total_cost_usd?: number;
  usage?: {
    input_tokens?: number;
    output_tokens?: number;
  };
  modelUsage?: Record<string, { inputTokens?: number; outputTokens?: number }>;
}

// --- Mapper State ---

/**
 * State tracker for mapping SDK messages to Parts.
 * Maintains tool use state across messages for status transitions.
 */
export class MessageMapper {
  private messageId: string;
  private toolParts: Map<string, ToolPart> = new Map(); // toolUseId → ToolPart
  private partOrder: string[] = []; // Track part IDs in order

  constructor(messageId: string) {
    this.messageId = messageId;
  }

  /**
   * Create a step-start part (emitted at turn start).
   */
  createStepStart(): StepStartPart {
    const part: StepStartPart = {
      id: randomUUID(),
      messageId: this.messageId,
      type: 'step-start',
    };
    this.partOrder.push(part.id);
    return part;
  }

  /**
   * Create a step-finish part (emitted at turn end).
   */
  createStepFinish(usage?: TokenUsage, cost?: number): StepFinishPart {
    const part: StepFinishPart = {
      id: randomUUID(),
      messageId: this.messageId,
      type: 'step-finish',
      usage,
      cost,
    };
    this.partOrder.push(part.id);
    return part;
  }

  /**
   * Map a text content block to a TextPart.
   */
  mapTextBlock(block: TextBlock): { part: TextPart; delta: string } {
    const part: TextPart = {
      id: randomUUID(),
      messageId: this.messageId,
      type: 'text',
      text: block.text,
    };
    this.partOrder.push(part.id);
    return { part, delta: block.text };
  }

  /**
   * Map a thinking content block to a ReasoningPart.
   */
  mapThinkingBlock(block: ThinkingBlock): { part: ReasoningPart; delta: string } {
    const part: ReasoningPart = {
      id: randomUUID(),
      messageId: this.messageId,
      type: 'reasoning',
      text: block.thinking,
    };
    this.partOrder.push(part.id);
    return { part, delta: block.thinking };
  }

  /**
   * Map a tool_use content block to a ToolPart with 'pending' status.
   * Returns the new ToolPart.
   */
  mapToolUseBlock(block: ToolUseBlock): ToolPart {
    const part: ToolPart = {
      id: randomUUID(),
      messageId: this.messageId,
      type: 'tool',
      toolUseId: block.id,
      toolName: block.name,
      input: block.input,
      status: 'pending',
    };
    this.toolParts.set(block.id, part);
    this.partOrder.push(part.id);
    return part;
  }

  /**
   * Update a tool part to 'running' status.
   */
  updateToolRunning(toolUseId: string): ToolPart | undefined {
    const part = this.toolParts.get(toolUseId);
    if (part) {
      part.status = 'running';
      return part;
    }
    return undefined;
  }

  /**
   * Map a tool_result content block to update the corresponding ToolPart.
   * Sets status to 'completed' or 'failed' based on is_error.
   */
  mapToolResultBlock(block: ToolResultBlock): ToolPart | undefined {
    const part = this.toolParts.get(block.tool_use_id);
    if (!part) {
      return undefined;
    }

    // Extract text content from result
    let output: string;
    if (typeof block.content === 'string') {
      output = block.content;
    } else if (Array.isArray(block.content)) {
      output = block.content
        .filter((c): c is { type: 'text'; text: string } => c.type === 'text')
        .map((c) => c.text)
        .join('\n');
    } else {
      output = '';
    }

    const status: ToolStatus = block.is_error ? 'failed' : 'completed';
    part.output = output;
    part.status = status;

    if (block.is_error) {
      part.error = output;
    }

    return part;
  }

  /**
   * Create a retry part for SDK retry events.
   */
  createRetryPart(attempt: number, reason: string): RetryPart {
    const part: RetryPart = {
      id: randomUUID(),
      messageId: this.messageId,
      type: 'retry',
      attempt,
      reason,
    };
    this.partOrder.push(part.id);
    return part;
  }

  /**
   * Get a tool part by its tool use ID.
   */
  getToolPart(toolUseId: string): ToolPart | undefined {
    return this.toolParts.get(toolUseId);
  }

  /**
   * Get all parts in emission order.
   */
  getAllParts(): Part[] {
    const parts: Part[] = [];
    for (const id of this.partOrder) {
      // Find the part by ID
      for (const toolPart of this.toolParts.values()) {
        if (toolPart.id === id) {
          parts.push(toolPart);
          break;
        }
      }
    }
    return parts;
  }
}

// --- Type Guards ---

export function isTextBlock(block: unknown): block is TextBlock {
  return (
    typeof block === 'object' &&
    block !== null &&
    'type' in block &&
    (block as { type: unknown }).type === 'text' &&
    'text' in block
  );
}

export function isThinkingBlock(block: unknown): block is ThinkingBlock {
  return (
    typeof block === 'object' &&
    block !== null &&
    'type' in block &&
    (block as { type: unknown }).type === 'thinking' &&
    'thinking' in block
  );
}

export function isToolUseBlock(block: unknown): block is ToolUseBlock {
  return (
    typeof block === 'object' &&
    block !== null &&
    'type' in block &&
    (block as { type: unknown }).type === 'tool_use' &&
    'id' in block &&
    'name' in block
  );
}

export function isToolResultBlock(block: unknown): block is ToolResultBlock {
  return (
    typeof block === 'object' &&
    block !== null &&
    'type' in block &&
    (block as { type: unknown }).type === 'tool_result' &&
    'tool_use_id' in block
  );
}

export function isAssistantMessage(message: unknown): message is SDKAssistantMessage {
  return (
    typeof message === 'object' &&
    message !== null &&
    'type' in message &&
    (message as { type: unknown }).type === 'assistant' &&
    'message' in message
  );
}

export function isResultMessage(message: unknown): message is SDKResultMessage {
  return (
    typeof message === 'object' &&
    message !== null &&
    'type' in message &&
    (message as { type: unknown }).type === 'result'
  );
}

export function isUserMessage(
  message: unknown
): message is {
  type: 'user';
  uuid: string;
  session_id: string;
  message: { role: 'user'; content: unknown[] };
} {
  return (
    typeof message === 'object' &&
    message !== null &&
    'type' in message &&
    (message as { type: unknown }).type === 'user'
  );
}
