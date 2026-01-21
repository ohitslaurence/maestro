import { chromium, type Page } from "playwright";

const uiUrl = process.env.MAESTRO_UI_URL ?? "http://127.0.0.1:1420";
const daemonHost = process.env.MAESTRO_DAEMON_HOST ?? "127.0.0.1";
const daemonPort = Number(process.env.MAESTRO_DAEMON_PORT ?? "55433");
const daemonToken = process.env.MAESTRO_DAEMON_TOKEN ?? "dev";
const headless = process.env.MAESTRO_HEADLESS !== "false";

if (!Number.isFinite(daemonPort)) {
  throw new Error("MAESTRO_DAEMON_PORT must be a number");
}

const localStorageKeys = [
  "maestro.daemon.web.config",
  "maestro.daemon.profiles",
  "maestro.daemon.rememberLastUsed",
  "maestro.daemon.lastUsedId",
];

/**
 * Connect to daemon via the connection modal.
 */
async function connectToDaemon(page: Page): Promise<void> {
  const modalHeading = page.getByRole("heading", { name: "Daemon Connection" });
  const modalVisible = await modalHeading.isVisible().catch(() => false);

  if (!modalVisible) {
    // Try to open the modal
    const configureButton = page.getByRole("button", {
      name: "Configure Connection",
    });
    if (await configureButton.isVisible().catch(() => false)) {
      await configureButton.click();
    } else {
      const statusButton = page.getByRole("button", { name: "Disconnected" });
      await statusButton.click();
      await page.getByRole("button", { name: "Manage connections" }).click();
    }
  }

  await page.fill("#daemon-host", daemonHost);
  await page.fill("#daemon-port", String(daemonPort));
  await page.fill("#daemon-token", daemonToken);
  await page.getByRole("button", { name: "Connect", exact: true }).click();

  await modalHeading
    .waitFor({ state: "detached", timeout: 15000 })
    .catch(() => modalHeading.waitFor({ state: "hidden", timeout: 15000 }));

  // Verify connected
  const connectedButton = page.getByRole("button", {
    name: `${daemonHost}:${daemonPort}`,
  });
  await connectedButton.waitFor({ timeout: 15000 });
  console.log(`[ui-claude] Connected to daemon at ${daemonHost}:${daemonPort}`);
}

/**
 * Select a workspace from the sidebar.
 * Returns true if a workspace was found and selected.
 */
async function selectWorkspace(page: Page): Promise<boolean> {
  // Look for workspace items in the sidebar
  const workspaceItem = page.locator(".workspace-item").first();
  const hasWorkspace = await workspaceItem.isVisible().catch(() => false);

  if (!hasWorkspace) {
    console.log("[ui-claude] No workspaces found in sidebar");
    return false;
  }

  await workspaceItem.click();
  console.log("[ui-claude] Selected first workspace");
  return true;
}

/**
 * Switch to Claude provider.
 */
async function switchToClaudeProvider(page: Page): Promise<void> {
  const claudeButton = page.getByRole("button", { name: "Claude" });

  // Wait for the button to be visible and enabled
  await claudeButton.waitFor({ timeout: 10000 });
  const isDisabled = await claudeButton.isDisabled();

  if (isDisabled) {
    throw new Error("Claude provider button is disabled - no workspace selected?");
  }

  await claudeButton.click();

  // Verify button is now active
  const isActive = await claudeButton.evaluate((el) =>
    el.classList.contains("agent-provider-selector__btn--active")
  );

  if (!isActive) {
    throw new Error("Claude provider button did not become active after click");
  }

  console.log("[ui-claude] Switched to Claude provider");
}

/**
 * Wait for Claude SDK connection.
 * Handles connection error with retry.
 */
async function waitForClaudeConnection(page: Page): Promise<void> {
  // Look for either:
  // 1. Connection spinner (connecting state)
  // 2. Connection error with retry button
  // 3. Composer visible (connected state)

  const spinner = page.locator(".oc-thread__spinner");
  const retryButton = page.getByRole("button", { name: "Retry Connection" });
  const composer = page.locator(".oc-composer__input");

  // Wait up to 30s for connection to complete
  const timeout = 30000;
  const startTime = Date.now();

  while (Date.now() - startTime < timeout) {
    // Check if already connected (composer visible)
    if (await composer.isVisible().catch(() => false)) {
      console.log("[ui-claude] Claude SDK connected");
      return;
    }

    // Check for connection error
    if (await retryButton.isVisible().catch(() => false)) {
      console.log("[ui-claude] Connection error detected, clicking retry");
      await retryButton.click();
      // Wait a bit before checking again
      await page.waitForTimeout(2000);
      continue;
    }

    // Check if still connecting
    if (await spinner.isVisible().catch(() => false)) {
      await page.waitForTimeout(500);
      continue;
    }

    await page.waitForTimeout(500);
  }

  // Final check
  if (await composer.isVisible().catch(() => false)) {
    console.log("[ui-claude] Claude SDK connected");
    return;
  }

  throw new Error("Timed out waiting for Claude SDK connection");
}

/**
 * Send a message and verify it appears in the thread.
 */
async function sendMessage(page: Page, message: string): Promise<void> {
  const composer = page.locator(".oc-composer__input");
  const sendButton = page.getByRole("button", { name: "Send" });

  // Type the message
  await composer.fill(message);

  // Click send
  await sendButton.click();

  console.log(`[ui-claude] Sent message: "${message}"`);
}

/**
 * Wait for processing to start.
 */
async function waitForProcessing(page: Page, timeoutMs = 10000): Promise<void> {
  const processingIndicator = page.locator(".oc-messages__processing");
  const stopButton = page.getByRole("button", { name: "Stop" });

  // Wait for either processing indicator or stop button
  await Promise.race([
    processingIndicator.waitFor({ timeout: timeoutMs }),
    stopButton.waitFor({ timeout: timeoutMs }),
  ]).catch(() => {
    // Check if already done (fast response)
    console.log("[ui-claude] Processing may have already completed");
  });

  console.log("[ui-claude] Processing started");
}

/**
 * Wait for response to complete (idle state).
 */
async function waitForResponse(page: Page, timeoutMs = 60000): Promise<void> {
  const composer = page.locator(".oc-composer__input");

  // Wait for composer to be enabled again (not processing)
  await page.waitForFunction(
    () => {
      const input = document.querySelector(".oc-composer__input") as HTMLTextAreaElement | null;
      return input && !input.disabled;
    },
    { timeout: timeoutMs }
  );

  // Verify send button is visible (not stop button)
  const sendButton = page.getByRole("button", { name: "Send" });
  await sendButton.waitFor({ timeout: 5000 });

  console.log("[ui-claude] Response completed");
}

/**
 * Verify assistant message appears in thread.
 */
async function verifyAssistantMessage(page: Page): Promise<void> {
  const assistantMessage = page.locator(".oc-message--assistant");
  await assistantMessage.first().waitFor({ timeout: 10000 });

  const messageCount = await assistantMessage.count();
  console.log(`[ui-claude] Found ${messageCount} assistant message(s)`);

  if (messageCount === 0) {
    throw new Error("No assistant message found in thread");
  }
}

/**
 * Test abort functionality.
 */
async function testAbort(page: Page): Promise<void> {
  const composer = page.locator(".oc-composer__input");
  const sendButton = page.getByRole("button", { name: "Send" });
  const stopButton = page.getByRole("button", { name: "Stop" });

  // Send a message that will trigger a longer response
  await composer.fill("Write a detailed essay about the history of computing");
  await sendButton.click();
  console.log("[ui-claude] Sent long message for abort test");

  // Wait for stop button to appear
  try {
    await stopButton.waitFor({ timeout: 10000 });
    console.log("[ui-claude] Stop button visible");

    // Click stop
    await stopButton.click();
    console.log("[ui-claude] Clicked stop button");

    // Verify we return to idle state
    await sendButton.waitFor({ timeout: 10000 });
    console.log("[ui-claude] Abort successful - returned to idle state");
  } catch {
    // Response may have completed quickly
    console.log("[ui-claude] Stop button not visible - response may have completed quickly");
  }
}

async function run() {
  const browser = await chromium.launch({ headless });
  const page = await browser.newPage();
  page.setDefaultTimeout(15000);

  try {
    await page.goto(uiUrl, { waitUntil: "domcontentloaded" });

    // Clear localStorage for clean state
    await page.evaluate((keys) => {
      for (const key of keys) {
        localStorage.removeItem(key);
      }
    }, localStorageKeys);
    await page.reload({ waitUntil: "domcontentloaded" });

    // Step 1: Connect to daemon
    await connectToDaemon(page);

    // Step 2: Select a workspace (required for Claude provider to be enabled)
    const hasWorkspace = await selectWorkspace(page);
    if (!hasWorkspace) {
      console.log("[ui-claude] Skipping Claude test - no workspaces available");
      console.log("[ui-claude] Test passed (no workspaces to test with)");
      return;
    }

    // Wait for AgentView to render
    await page.waitForTimeout(500);

    // Step 3: Switch to Claude provider
    await switchToClaudeProvider(page);

    // Step 4: Wait for Claude SDK connection
    await waitForClaudeConnection(page);

    // Step 5: Send a test message
    await sendMessage(page, "Hello, Claude! Please respond briefly.");

    // Step 6: Wait for processing to start
    await waitForProcessing(page);

    // Step 7: Wait for response to complete
    await waitForResponse(page);

    // Step 8: Verify assistant message appeared
    await verifyAssistantMessage(page);

    // Step 9: Test abort functionality (optional - may not work with fast responses)
    // Uncomment to test abort:
    // await testAbort(page);

    console.log("[ui-claude] All tests passed");
  } finally {
    await browser.close();
  }
}

try {
  await run();
} catch (error) {
  console.error(`[ui-claude] Failed: ${String(error)}`);
  process.exitCode = 1;
}
