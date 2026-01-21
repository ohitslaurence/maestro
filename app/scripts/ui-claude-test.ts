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

  await page.evaluate((keys) => {
    for (const key of keys) localStorage.removeItem(key);
  }, localStorageKeys);
  await page.reload({ waitUntil: "domcontentloaded" });

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

  await page.fill("#daemon-host", daemonHost);
  await page.fill("#daemon-port", String(daemonPort));
  await page.fill("#daemon-token", daemonToken);
  await page.getByRole("button", { name: "Connect", exact: true }).click();

  await modalHeading
    .waitFor({ state: "detached", timeout: 15000 })
    .catch(() => modalHeading.waitFor({ state: "hidden", timeout: 15000 }));

  console.log("[test] Connected to daemon");
}

async function run() {
  const browser = await chromium.launch({ headless, slowMo: 50 });
  const page = await browser.newPage();
  page.setDefaultTimeout(60000);

  page.on("console", (msg) => {
    const text = msg.text();
    if (msg.type() === "error" || text.includes("[claude]") || text.includes("[opencode]") || text.includes("stream")) {
      console.log(`[browser] ${text}`);
    }
  });

  try {
    await connectToDaemon(page);
    await page.waitForTimeout(1000);

    // Click on "maestro" session in sidebar
    const maestroSession = page.locator("text=maestro").first();
    if (await maestroSession.isVisible()) {
      await maestroSession.click();
      console.log("[test] Clicked maestro session");
      await page.waitForTimeout(1000);
    }

    await page.screenshot({ path: "/tmp/test-01-session.png" });
    console.log("[test] Screenshot: /tmp/test-01-session.png");

    // Switch to Claude provider if available
    const claudeButton = page.locator("button, span").filter({ hasText: "Claude" }).first();
    if (await claudeButton.isVisible().catch(() => false)) {
      await claudeButton.click();
      console.log("[test] Clicked Claude selector");
      await page.waitForTimeout(1000);
    }

    await page.screenshot({ path: "/tmp/test-02-claude.png" });
    console.log("[test] Screenshot: /tmp/test-02-claude.png");

    // Find textarea and send message
    const textarea = page.locator("textarea").first();
    if (await textarea.isVisible()) {
      await textarea.fill("Say exactly one word: Hello");
      console.log("[test] Filled message");

      await page.screenshot({ path: "/tmp/test-03-message.png" });

      // Submit
      await textarea.press("Enter");
      // Or find submit button
      // const sendButton = page.locator("button[type='submit']").first();
      // await sendButton.click();
      console.log("[test] Submitted message");

      // Monitor for 60 seconds
      const startTime = Date.now();
      let lastStatus = "";
      while (Date.now() - startTime < 60000) {
        await page.waitForTimeout(2000);

        // Check for working indicator
        const pageContent = await page.content();
        const hasWorking = pageContent.includes("Working") || pageContent.includes("working");
        const hasProcessing = pageContent.includes("processing");

        // Check for response messages
        const assistantMessages = page.locator(".oc-message--assistant, [class*='assistant']");
        const msgCount = await assistantMessages.count();

        const status = `Working: ${hasWorking}, Processing: ${hasProcessing}, Messages: ${msgCount}`;
        if (status !== lastStatus) {
          console.log(`[test] ${status}`);
          lastStatus = status;
          await page.screenshot({ path: `/tmp/test-status-${Date.now()}.png` });
        }

        // If we have messages and no working indicator, we're done
        if (msgCount > 0 && !hasWorking && !hasProcessing) {
          console.log("[test] PASS - Response received, working indicator stopped");
          break;
        }
      }

      await page.screenshot({ path: "/tmp/test-final.png" });
      console.log("[test] Final screenshot: /tmp/test-final.png");

    } else {
      console.log("[test] No textarea found");
      await page.screenshot({ path: "/tmp/test-no-textarea.png" });
    }

  } catch (error) {
    await page.screenshot({ path: "/tmp/test-error.png" });
    console.error(`[test] Error: ${error}`);
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
