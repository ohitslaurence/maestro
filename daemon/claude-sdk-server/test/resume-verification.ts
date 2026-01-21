/**
 * Resume Verification Test (Phase 7)
 *
 * Verifies that session resume works after server restart per spec §5 Resume Flow:
 * 1. Create a session and send a message
 * 2. Verify resumeId is saved to disk
 * 3. [Manual step] Restart server
 * 4. Send another message to the same session
 * 5. Verify the SDK receives context from the previous conversation
 *
 * Run: bun test/resume-verification.ts
 * Requires: Server running on localhost:9100, ANTHROPIC_API_KEY set
 *
 * For full verification (restart test):
 *   bun test/resume-verification.ts --step 1  # Before restart
 *   # Stop server (Ctrl+C), restart it
 *   bun test/resume-verification.ts --step 2 --session <session-id>  # After restart
 */

import { existsSync, readFileSync } from 'fs';
import { join } from 'path';
import { homedir } from 'os';

const BASE_URL = process.env.TEST_URL || 'http://localhost:9100';
const WORKSPACE_ID = process.env.WORKSPACE_ID || 'test-workspace';

interface Session {
  id: string;
  resumeId?: string;
  workspaceId: string;
  directory: string;
  title: string;
  status: string;
}

interface MessageResponse {
  info: { id: string; role: string };
  parts: Array<{ type: string; text?: string }>;
}

/**
 * Get session file path
 */
function getSessionFilePath(sessionId: string): string {
  return join(
    homedir(),
    '.maestro',
    'claude',
    WORKSPACE_ID,
    'sessions',
    `${sessionId}.json`
  );
}

/**
 * Read session from disk
 */
function readSessionFromDisk(sessionId: string): Session | null {
  const path = getSessionFilePath(sessionId);
  if (!existsSync(path)) return null;
  try {
    return JSON.parse(readFileSync(path, 'utf-8'));
  } catch {
    return null;
  }
}

/**
 * Create a test session
 */
async function createSession(): Promise<Session> {
  const response = await fetch(`${BASE_URL}/session`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({
      title: `Resume Test ${Date.now()}`,
      permission: 'bypassPermissions',
    }),
  });

  if (!response.ok) {
    throw new Error(`Failed to create session: ${response.status}`);
  }

  return response.json() as Promise<Session>;
}

/**
 * Get session from API
 */
async function getSession(sessionId: string): Promise<Session> {
  const response = await fetch(`${BASE_URL}/session/${sessionId}`);
  if (!response.ok) {
    throw new Error(`Failed to get session: ${response.status}`);
  }
  return response.json() as Promise<Session>;
}

/**
 * Send a message and wait for response
 */
async function sendMessage(sessionId: string, text: string): Promise<MessageResponse> {
  const response = await fetch(`${BASE_URL}/session/${sessionId}/message`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ parts: [{ type: 'text', text }] }),
  });

  if (!response.ok) {
    const error = await response.text();
    throw new Error(`Failed to send message: ${response.status} - ${error}`);
  }

  return response.json() as Promise<MessageResponse>;
}

/**
 * Extract text content from message parts
 */
function extractText(parts: MessageResponse['parts']): string {
  return parts
    .filter((p) => p.type === 'text' && p.text)
    .map((p) => p.text)
    .join('\n');
}

/**
 * Step 1: Create session and send initial message
 */
async function runStep1(): Promise<void> {
  console.log('=== Resume Verification - Step 1 (Before Restart) ===\n');

  // Check server health
  const health = await fetch(`${BASE_URL}/health`);
  if (!health.ok) {
    console.error('✗ Server not reachable');
    process.exit(1);
  }
  console.log('✓ Server health check passed\n');

  // Create session
  console.log('Creating session...');
  const session = await createSession();
  console.log(`✓ Session created: ${session.id}\n`);

  // Send first message with memorable context
  console.log('Sending initial message with context to remember...');
  const firstMessage = await sendMessage(
    session.id,
    'Remember this secret word: PINEAPPLE. Just acknowledge you understand and will remember it.'
  );
  console.log('✓ First message sent\n');
  console.log('Response:', extractText(firstMessage.parts).slice(0, 200) + '...\n');

  // Check resumeId was saved
  const savedSession = readSessionFromDisk(session.id);
  if (savedSession?.resumeId) {
    console.log(`✓ resumeId saved to disk: ${savedSession.resumeId.slice(0, 20)}...\n`);
  } else {
    console.log('✗ resumeId NOT found on disk\n');
    console.log(`  Expected at: ${getSessionFilePath(session.id)}`);
  }

  console.log('=== Step 1 Complete ===\n');
  console.log('Next steps:');
  console.log('1. Stop the server (Ctrl+C in the server terminal)');
  console.log('2. Start the server again: bun run src/index.ts');
  console.log(`3. Run: bun test/resume-verification.ts --step 2 --session ${session.id}\n`);
}

/**
 * Step 2: After restart, verify resume works
 */
async function runStep2(sessionId: string): Promise<void> {
  console.log('=== Resume Verification - Step 2 (After Restart) ===\n');

  // Check server health
  const health = await fetch(`${BASE_URL}/health`);
  if (!health.ok) {
    console.error('✗ Server not reachable');
    process.exit(1);
  }
  console.log('✓ Server health check passed\n');

  // Check session still exists in API
  console.log(`Checking session ${sessionId}...`);
  let session: Session;
  try {
    session = await getSession(sessionId);
    console.log(`✓ Session found: ${session.title}\n`);
  } catch (err) {
    console.error(`✗ Session not found: ${err}`);
    process.exit(1);
  }

  // Check resumeId was loaded from disk
  if (session.resumeId) {
    console.log(`✓ resumeId loaded from disk: ${session.resumeId.slice(0, 20)}...\n`);
  } else {
    console.log('⚠ resumeId not visible in API (may be internal only)\n');
  }

  // Send follow-up message asking about the secret word
  console.log('Sending follow-up message asking about the secret word...');
  const followUp = await sendMessage(
    sessionId,
    'What was the secret word I told you to remember? Just say the word.'
  );
  console.log('✓ Follow-up message sent\n');

  const responseText = extractText(followUp.parts);
  console.log('Response:', responseText.slice(0, 500), '\n');

  // Check if response mentions PINEAPPLE
  if (responseText.toUpperCase().includes('PINEAPPLE')) {
    console.log('✓✓✓ SUCCESS: Resume worked! The agent remembered the secret word.\n');
  } else {
    console.log('✗ FAILURE: The agent did not remember the secret word.\n');
    console.log('  This could mean:');
    console.log('  - resumeId was not properly saved/loaded');
    console.log('  - SDK did not restore conversation context');
    console.log('  - The model chose not to repeat the word\n');
  }

  console.log('=== Resume Verification Complete ===');
}

/**
 * Quick test (no restart) - verify resumeId persistence
 */
async function runQuickTest(): Promise<void> {
  console.log('=== Resume Verification - Quick Test (No Restart) ===\n');

  // Check server health
  const health = await fetch(`${BASE_URL}/health`);
  if (!health.ok) {
    console.error('✗ Server not reachable');
    process.exit(1);
  }
  console.log('✓ Server health check passed\n');

  // Create session
  console.log('Creating session...');
  const session = await createSession();
  console.log(`✓ Session created: ${session.id}\n`);

  // Send first message
  console.log('Sending first message...');
  const firstMessage = await sendMessage(
    session.id,
    'Remember this number: 42. Acknowledge you understand.'
  );
  console.log('✓ First message response received\n');

  // Check resumeId was saved
  const savedSession = readSessionFromDisk(session.id);
  if (savedSession?.resumeId) {
    console.log(`✓ resumeId persisted: ${savedSession.resumeId.slice(0, 20)}...\n`);
  } else {
    console.log('✗ resumeId NOT persisted\n');
    process.exit(1);
  }

  // Send second message (same server instance)
  console.log('Sending follow-up message...');
  const secondMessage = await sendMessage(session.id, 'What number did I just mention?');
  const responseText = extractText(secondMessage.parts);
  console.log('Response:', responseText.slice(0, 200), '\n');

  if (responseText.includes('42')) {
    console.log('✓ Resume works within same server instance\n');
  } else {
    console.log('⚠ Number not found in response (model may have phrased differently)\n');
  }

  // Check resumeId was updated
  const finalSession = readSessionFromDisk(session.id);
  if (finalSession?.resumeId && finalSession.resumeId !== savedSession?.resumeId) {
    console.log('✓ resumeId updated after second message\n');
  } else if (finalSession?.resumeId) {
    console.log('ℹ resumeId unchanged (SDK may reuse same token)\n');
  }

  console.log('=== Quick Test Complete ===');
  console.log('\nFor full restart verification, run with --step 1 and --step 2');
}

// Parse arguments and run
const args = process.argv.slice(2);
const stepArg = args.indexOf('--step');
const sessionArg = args.indexOf('--session');

if (stepArg !== -1 && args[stepArg + 1] === '1') {
  runStep1().catch((err) => {
    console.error('Error:', err);
    process.exit(1);
  });
} else if (stepArg !== -1 && args[stepArg + 1] === '2') {
  if (sessionArg === -1 || !args[sessionArg + 1]) {
    console.error('Step 2 requires --session <session-id>');
    process.exit(1);
  }
  runStep2(args[sessionArg + 1]).catch((err) => {
    console.error('Error:', err);
    process.exit(1);
  });
} else {
  // Default: run quick test
  runQuickTest().catch((err) => {
    console.error('Error:', err);
    process.exit(1);
  });
}
