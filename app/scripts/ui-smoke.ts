import { chromium, type Locator, type Page } from "playwright";

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

async function openConnectionModal(page: Page): Promise<Locator> {
  const modalHeading = page.getByRole("heading", { name: "Daemon Connection" });
  const modalVisible = await modalHeading.isVisible().catch(() => false);
  if (modalVisible) {
    return modalHeading;
  }

  const configureButton = page.getByRole("button", {
    name: "Configure Connection",
  });
  if (await configureButton.isVisible().catch(() => false)) {
    await configureButton.click();
    return modalHeading;
  }

  const statusButton = page.getByRole("button", { name: "Disconnected" });
  await statusButton.click();
  await page.getByRole("button", { name: "Manage connections" }).click();
  return modalHeading;
}

async function run() {
  const browser = await chromium.launch({ headless });
  const page = await browser.newPage();
  page.setDefaultTimeout(15000);

  try {
    await page.goto(uiUrl, { waitUntil: "domcontentloaded" });

    await page.evaluate((keys) => {
      for (const key of keys) {
        localStorage.removeItem(key);
      }
    }, localStorageKeys);
    await page.reload({ waitUntil: "domcontentloaded" });

    const modalHeading = await openConnectionModal(page);

    await page.fill("#daemon-host", daemonHost);
    await page.fill("#daemon-port", String(daemonPort));
    await page.fill("#daemon-token", daemonToken);
    await page.getByRole("button", { name: "Connect", exact: true }).click();

    await modalHeading
      .waitFor({ state: "detached", timeout: 15000 })
      .catch(() => modalHeading.waitFor({ state: "hidden", timeout: 15000 }));

    const connectedButton = page.getByRole("button", {
      name: `${daemonHost}:${daemonPort}`,
    });
    await connectedButton.waitFor({ timeout: 15000 });

    const welcomeHeading = page.getByRole("heading", {
      name: "Welcome to Maestro",
    });
    const welcomeText = page.getByText(
      "Select a session from the sidebar to view it.",
    );
    const welcomeVisible = await welcomeHeading.isVisible().catch(() => false);
    const selectVisible = await welcomeText.isVisible().catch(() => false);
    if (!welcomeVisible && !selectVisible) {
      throw new Error("Expected welcome screen after connect");
    }

    console.log(
      `[ui-smoke] Connected to ${daemonHost}:${daemonPort} via ${uiUrl}`,
    );
  } finally {
    await browser.close();
  }
}

try {
  await run();
} catch (error) {
  console.error(`[ui-smoke] Failed: ${String(error)}`);
  process.exitCode = 1;
}
