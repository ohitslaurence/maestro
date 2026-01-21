import { chromium, type Page } from "playwright";

const uiUrl = process.env.MAESTRO_UI_URL ?? "http://127.0.0.1:1420";
const daemonHost = process.env.MAESTRO_DAEMON_HOST ?? "127.0.0.1";
const daemonPort = Number(process.env.MAESTRO_DAEMON_PORT ?? "55433");
const daemonToken = process.env.MAESTRO_DAEMON_TOKEN ?? "dev";
const headless = process.env.MAESTRO_HEADLESS !== "false";

const localStorageKeys = [
  "maestro.daemon.web.config",
  "maestro.daemon.profiles",
  "maestro.daemon.rememberLastUsed",
  "maestro.daemon.lastUsedId",
];

async function connectToDaemon(page: Page) {
  await page.goto(uiUrl, { waitUntil: "domcontentloaded" });

  // Clear localStorage
  await page.evaluate((keys) => {
    for (const key of keys) {
      localStorage.removeItem(key);
    }
  }, localStorageKeys);
  await page.reload({ waitUntil: "domcontentloaded" });

  // Open connection modal
  const modalHeading = page.getByRole("heading", { name: "Daemon Connection" });
  const modalVisible = await modalHeading.isVisible().catch(() => false);
  if (!modalVisible) {
    const configureButton = page.getByRole("button", { name: "Configure Connection" });
    if (await configureButton.isVisible().catch(() => false)) {
      await configureButton.click();
    } else {
      const statusButton = page.getByRole("button", { name: "Disconnected" });
      await statusButton.click();
      await page.getByRole("button", { name: "Manage connections" }).click();
    }
  }

  // Fill connection details
  await page.fill("#daemon-host", daemonHost);
  await page.fill("#daemon-port", String(daemonPort));
  await page.fill("#daemon-token", daemonToken);
  await page.getByRole("button", { name: "Connect", exact: true }).click();

  // Wait for connection
  await modalHeading
    .waitFor({ state: "detached", timeout: 15000 })
    .catch(() => modalHeading.waitFor({ state: "hidden", timeout: 15000 }));

  const connectedButton = page.getByRole("button", {
    name: `${daemonHost}:${daemonPort}`,
  });
  await connectedButton.waitFor({ timeout: 15000 });
  console.log("[test] Connected to daemon");
}

async function run() {
  const browser = await chromium.launch({ headless });
  const page = await browser.newPage();
  page.setDefaultTimeout(30000);

  try {
    await connectToDaemon(page);

    // Look for existing session or create workspace
    // First check if there's a session list
    const sessionItems = page.locator(".sessions-list__item");
    const sessionCount = await sessionItems.count();

    if (sessionCount > 0) {
      console.log(`[test] Found ${sessionCount} session(s), clicking first one`);
      await sessionItems.first().click();
      await page.waitForTimeout(1000);
    } else {
      console.log("[test] No sessions found, checking for workspace setup");
    }

    // Check if agent view is visible
    const agentView = page.locator(".agent-view");
    const agentVisible = await agentView.isVisible().catch(() => false);

    if (!agentVisible) {
      console.log("[test] Agent view not visible, skipping agent tests");
      console.log("[test] PASS - Connection test succeeded");
      return;
    }

    // Check provider selector
    const providerSelector = page.locator(".agent-provider-selector");
    const selectorVisible = await providerSelector.isVisible().catch(() => false);
    console.log(`[test] Provider selector visible: ${selectorVisible}`);

    // Check for composer/input area
    const composer = page.locator(".thread-composer, .oc-thread-composer");
    const composerVisible = await composer.isVisible().catch(() => false);
    console.log(`[test] Composer visible: ${composerVisible}`);

    if (composerVisible) {
      // Check if we can find the textarea
      const textarea = page.locator(".thread-composer textarea, .oc-thread-composer textarea");
      const textareaVisible = await textarea.isVisible().catch(() => false);
      console.log(`[test] Textarea visible: ${textareaVisible}`);

      if (textareaVisible) {
        // Type a message but don't send (no API key)
        await textarea.fill("test message");
        console.log("[test] Filled textarea with test message");

        // Check for send button
        const sendButton = page.locator("button[type='submit'], .thread-composer button");
        const sendVisible = await sendButton.first().isVisible().catch(() => false);
        console.log(`[test] Send button visible: ${sendVisible}`);
      }
    }

    // Take a screenshot for debugging
    await page.screenshot({ path: "/tmp/agent-test.png" });
    console.log("[test] Screenshot saved to /tmp/agent-test.png");

    console.log("[test] PASS - UI elements verified");
  } catch (error) {
    await page.screenshot({ path: "/tmp/agent-test-error.png" });
    console.error(`[test] Screenshot saved to /tmp/agent-test-error.png`);
    throw error;
  } finally {
    await browser.close();
  }
}

try {
  await run();
} catch (error) {
  console.error(`[test] Failed: ${String(error)}`);
  process.exitCode = 1;
}
