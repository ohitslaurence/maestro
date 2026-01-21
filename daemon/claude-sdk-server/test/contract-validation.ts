/**
 * Contract Validation Tests (Phase 5.5)
 *
 * Validates that SSE events match the OpenCode-compatible schema:
 * - Payload shape is { type, properties }
 * - message.updated precedes message.part.updated for same message
 * - Event ordering matches spec (§4, Appendix B)
 *
 * Run: bun test/contract-validation.ts
 * Requires: Server running on localhost:9100
 */

import { randomUUID } from 'crypto';

const BASE_URL = process.env.TEST_URL || 'http://localhost:9100';

interface SSEEvent {
  type: string;
  properties: unknown;
}

interface EventLog {
  raw: string;
  parsed: SSEEvent;
  timestamp: number;
}

/**
 * Parse SSE data frame to event object
 */
function parseSSEEvent(data: string): SSEEvent | null {
  if (!data.startsWith('data: ')) return null;
  const json = data.slice(6).trim();
  if (!json) return null;
  try {
    return JSON.parse(json);
  } catch {
    return null;
  }
}

/**
 * Collect SSE events for a specified duration
 */
async function collectEvents(
  timeoutMs: number,
  stopOnIdle = true
): Promise<EventLog[]> {
  const events: EventLog[] = [];
  const controller = new AbortController();
  const timeout = setTimeout(() => controller.abort(), timeoutMs);

  try {
    const response = await fetch(`${BASE_URL}/event`, {
      signal: controller.signal,
      headers: { Accept: 'text/event-stream' },
    });

    if (!response.ok || !response.body) {
      throw new Error(`Failed to connect to SSE: ${response.status}`);
    }

    const reader = response.body.getReader();
    const decoder = new TextDecoder();
    let buffer = '';

    while (true) {
      const { done, value } = await reader.read();
      if (done) break;

      buffer += decoder.decode(value, { stream: true });
      const lines = buffer.split('\n\n');
      buffer = lines.pop() || '';

      for (const line of lines) {
        if (line.startsWith(':')) continue; // Skip comments/pings
        const parsed = parseSSEEvent(line);
        if (parsed) {
          events.push({ raw: line, parsed, timestamp: Date.now() });

          // Stop after session becomes idle (completed message)
          if (
            stopOnIdle &&
            parsed.type === 'session.status' &&
            (parsed.properties as { status?: { type?: string } })?.status?.type ===
              'idle' &&
            events.length > 2
          ) {
            controller.abort();
            break;
          }
        }
      }
    }
  } catch (err) {
    if ((err as Error).name !== 'AbortError') {
      throw err;
    }
  } finally {
    clearTimeout(timeout);
  }

  return events;
}

/**
 * Validate that all events have { type, properties } shape
 */
function validateEventShape(events: EventLog[]): { valid: boolean; errors: string[] } {
  const errors: string[] = [];

  for (const event of events) {
    const { parsed } = event;

    if (typeof parsed !== 'object' || parsed === null) {
      errors.push(`Event is not an object: ${event.raw}`);
      continue;
    }

    if (typeof parsed.type !== 'string') {
      errors.push(`Event missing 'type' string field: ${JSON.stringify(parsed)}`);
    }

    if (!('properties' in parsed)) {
      errors.push(`Event missing 'properties' field: ${JSON.stringify(parsed)}`);
    }
  }

  return { valid: errors.length === 0, errors };
}

/**
 * Validate that message.updated precedes message.part.updated for same message
 */
function validateEventOrdering(
  events: EventLog[]
): { valid: boolean; errors: string[] } {
  const errors: string[] = [];
  const messageUpdateTimes = new Map<string, number>();

  for (let i = 0; i < events.length; i++) {
    const { parsed, timestamp } = events[i];

    if (parsed.type === 'message.updated') {
      const props = parsed.properties as { info?: { id?: string } };
      const messageId = props.info?.id;
      if (messageId) {
        messageUpdateTimes.set(messageId, i);
      }
    }

    if (parsed.type === 'message.part.updated') {
      const props = parsed.properties as { part?: { messageId?: string } };
      const messageId = props.part?.messageId;
      if (messageId) {
        const updateIndex = messageUpdateTimes.get(messageId);
        if (updateIndex === undefined) {
          errors.push(
            `message.part.updated for messageId=${messageId} appeared before message.updated (event index ${i})`
          );
        } else if (updateIndex > i) {
          errors.push(
            `message.part.updated (index ${i}) appeared before message.updated (index ${updateIndex}) for messageId=${messageId}`
          );
        }
      }
    }
  }

  return { valid: errors.length === 0, errors };
}

/**
 * Create a session for testing
 */
async function createSession(): Promise<string> {
  const response = await fetch(`${BASE_URL}/session`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({
      title: `Contract Test ${Date.now()}`,
      permission: 'bypassPermissions',
    }),
  });

  if (!response.ok) {
    throw new Error(`Failed to create session: ${response.status}`);
  }

  const session = (await response.json()) as { id: string };
  return session.id;
}

/**
 * Send a message and collect events
 */
async function sendMessageWithEvents(
  sessionId: string,
  text: string,
  collectTimeoutMs = 30000
): Promise<EventLog[]> {
  // Start collecting events before sending message
  const eventsPromise = collectEvents(collectTimeoutMs, true);

  // Small delay to ensure SSE connection is established
  await new Promise((r) => setTimeout(r, 100));

  // Send message
  const response = await fetch(`${BASE_URL}/session/${sessionId}/message`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ parts: [{ type: 'text', text }] }),
  });

  if (!response.ok) {
    const error = await response.text();
    console.log('Message response:', response.status, error);
    // Don't throw - we still want to collect any events that were emitted
  }

  return eventsPromise;
}

/**
 * Print event summary
 */
function printEventSummary(events: EventLog[]): void {
  console.log('\n=== Event Summary ===');
  console.log(`Total events: ${events.length}\n`);

  for (let i = 0; i < events.length; i++) {
    const { parsed } = events[i];
    let summary = `[${i}] ${parsed.type}`;

    // Add useful context
    const props = parsed.properties as Record<string, unknown>;
    if (parsed.type === 'message.updated') {
      const info = props.info as { id?: string; role?: string };
      summary += ` (id=${info?.id?.slice(0, 8)}..., role=${info?.role})`;
    } else if (parsed.type === 'message.part.updated') {
      const part = props.part as { type?: string; messageId?: string; toolName?: string };
      summary += ` (type=${part?.type}, msgId=${part?.messageId?.slice(0, 8)}...)`;
      if (part?.toolName) summary += ` (tool=${part.toolName})`;
    } else if (parsed.type === 'session.status') {
      const status = props.status as { type?: string };
      summary += ` (status=${status?.type})`;
    }

    console.log(summary);
  }
}

/**
 * Run validation tests
 */
async function runTests(): Promise<void> {
  console.log('=== Claude SDK Server Contract Validation ===\n');
  console.log(`Server: ${BASE_URL}\n`);

  // Check server health
  try {
    const health = await fetch(`${BASE_URL}/health`);
    if (!health.ok) throw new Error('Health check failed');
    console.log('✓ Server health check passed\n');
  } catch (err) {
    console.error('✗ Server not reachable. Start with: bun run src/index.ts\n');
    process.exit(1);
  }

  // Test 1: Simple prompt (no tools)
  console.log('--- Test 1: Simple Prompt (no tools) ---');
  console.log('Creating session...');
  const session1 = await createSession();
  console.log(`Session created: ${session1}\n`);

  console.log('Sending simple prompt...');
  const events1 = await sendMessageWithEvents(session1, 'What is 2+2? Answer briefly.');

  printEventSummary(events1);

  // Validate shape
  const shape1 = validateEventShape(events1);
  if (shape1.valid) {
    console.log('\n✓ Event shape validation passed');
  } else {
    console.log('\n✗ Event shape validation failed:');
    shape1.errors.forEach((e) => console.log(`  - ${e}`));
  }

  // Validate ordering
  const order1 = validateEventOrdering(events1);
  if (order1.valid) {
    console.log('✓ Event ordering validation passed');
  } else {
    console.log('✗ Event ordering validation failed:');
    order1.errors.forEach((e) => console.log(`  - ${e}`));
  }

  // Test 2: Tool prompt (if API key available)
  if (process.env.ANTHROPIC_API_KEY) {
    console.log('\n--- Test 2: Tool Prompt ---');
    const session2 = await createSession();
    console.log(`Session created: ${session2}\n`);

    console.log('Sending tool prompt...');
    const events2 = await sendMessageWithEvents(
      session2,
      'Use the Read tool to read package.json and tell me the project name.'
    );

    printEventSummary(events2);

    const shape2 = validateEventShape(events2);
    if (shape2.valid) {
      console.log('\n✓ Event shape validation passed');
    } else {
      console.log('\n✗ Event shape validation failed:');
      shape2.errors.forEach((e) => console.log(`  - ${e}`));
    }

    const order2 = validateEventOrdering(events2);
    if (order2.valid) {
      console.log('✓ Event ordering validation passed');
    } else {
      console.log('✗ Event ordering validation failed:');
      order2.errors.forEach((e) => console.log(`  - ${e}`));
    }

    // Check for tool events
    const toolEvents = events2.filter((e) => {
      const props = e.parsed.properties as { part?: { type?: string } };
      return e.parsed.type === 'message.part.updated' && props.part?.type === 'tool';
    });
    if (toolEvents.length > 0) {
      console.log(`✓ Found ${toolEvents.length} tool part events`);
    } else {
      console.log('⚠ No tool events found (tool may not have been invoked)');
    }
  } else {
    console.log('\n⚠ Skipping tool test: ANTHROPIC_API_KEY not set');
  }

  console.log('\n=== Contract Validation Complete ===');
}

// Run if executed directly
runTests().catch((err) => {
  console.error('Test failed:', err);
  process.exit(1);
});
