#!/usr/bin/env bun
/**
 * Interactive Playwright driver - reads JSON commands from stdin, executes them.
 *
 * Usage:
 *   bun scripts/playwright-driver.ts
 *
 * Then send JSON commands (one per line):
 *   {"cmd": "goto", "url": "http://localhost:1420"}
 *   {"cmd": "click", "selector": "text=maestro"}
 *   {"cmd": "fill", "selector": "textarea", "value": "hello"}
 *   {"cmd": "screenshot", "path": "/tmp/shot.png"}
 *   {"cmd": "content"}
 *   {"cmd": "eval", "script": "document.title"}
 *   {"cmd": "quit"}
 */

import { chromium, type Browser, type Page } from "playwright";
import * as readline from "readline";

const headless = process.env.MAESTRO_HEADLESS !== "false";

let browser: Browser | null = null;
let page: Page | null = null;

async function init() {
  browser = await chromium.launch({ headless });
  page = await browser.newPage();
  page.setDefaultTimeout(30000);

  page.on("console", (msg) => {
    console.log(JSON.stringify({ type: "console", level: msg.type(), text: msg.text() }));
  });

  console.log(JSON.stringify({ type: "ready", message: "Playwright driver ready" }));
}

async function handleCommand(line: string) {
  if (!page) {
    console.log(JSON.stringify({ type: "error", message: "Page not initialized" }));
    return;
  }

  let cmd: any;
  try {
    cmd = JSON.parse(line);
  } catch {
    console.log(JSON.stringify({ type: "error", message: "Invalid JSON" }));
    return;
  }

  try {
    switch (cmd.cmd) {
      case "goto":
        await page.goto(cmd.url, { waitUntil: "domcontentloaded" });
        console.log(JSON.stringify({ type: "ok", url: page.url() }));
        break;

      case "click":
        await page.locator(cmd.selector).first().click({ timeout: cmd.timeout ?? 10000 });
        console.log(JSON.stringify({ type: "ok" }));
        break;

      case "fill":
        await page.locator(cmd.selector).first().fill(cmd.value);
        console.log(JSON.stringify({ type: "ok" }));
        break;

      case "type":
        await page.locator(cmd.selector).first().pressSequentially(cmd.value);
        console.log(JSON.stringify({ type: "ok" }));
        break;

      case "press":
        await page.locator(cmd.selector).first().press(cmd.key);
        console.log(JSON.stringify({ type: "ok" }));
        break;

      case "screenshot":
        await page.screenshot({ path: cmd.path ?? "/tmp/playwright-shot.png" });
        console.log(JSON.stringify({ type: "ok", path: cmd.path ?? "/tmp/playwright-shot.png" }));
        break;

      case "content":
        const html = await page.content();
        // Truncate for readability
        const preview = html.length > 2000 ? html.slice(0, 2000) + "..." : html;
        console.log(JSON.stringify({ type: "ok", length: html.length, preview }));
        break;

      case "text":
        const text = await page.locator(cmd.selector).first().textContent();
        console.log(JSON.stringify({ type: "ok", text }));
        break;

      case "count":
        const count = await page.locator(cmd.selector).count();
        console.log(JSON.stringify({ type: "ok", count }));
        break;

      case "visible":
        const visible = await page.locator(cmd.selector).first().isVisible().catch(() => false);
        console.log(JSON.stringify({ type: "ok", visible }));
        break;

      case "wait":
        await page.waitForTimeout(cmd.ms ?? 1000);
        console.log(JSON.stringify({ type: "ok" }));
        break;

      case "waitfor":
        await page.locator(cmd.selector).first().waitFor({ state: cmd.state ?? "visible", timeout: cmd.timeout ?? 30000 });
        console.log(JSON.stringify({ type: "ok" }));
        break;

      case "eval":
        const result = await page.evaluate(cmd.script);
        console.log(JSON.stringify({ type: "ok", result }));
        break;

      case "locators":
        // List all matching locators with their text
        const loc = page.locator(cmd.selector);
        const locCount = await loc.count();
        const items: string[] = [];
        for (let i = 0; i < Math.min(locCount, 20); i++) {
          const txt = await loc.nth(i).textContent().catch(() => null);
          items.push(txt ?? "(no text)");
        }
        console.log(JSON.stringify({ type: "ok", count: locCount, items }));
        break;

      case "quit":
        console.log(JSON.stringify({ type: "ok", message: "Closing" }));
        await browser?.close();
        process.exit(0);
        break;

      default:
        console.log(JSON.stringify({ type: "error", message: `Unknown command: ${cmd.cmd}` }));
    }
  } catch (error) {
    console.log(JSON.stringify({ type: "error", message: String(error) }));
  }
}

async function main() {
  await init();

  const rl = readline.createInterface({
    input: process.stdin,
    output: process.stdout,
    terminal: false,
  });

  rl.on("line", (line) => {
    handleCommand(line.trim());
  });

  rl.on("close", async () => {
    await browser?.close();
    process.exit(0);
  });
}

main().catch((err) => {
  console.error(JSON.stringify({ type: "fatal", message: String(err) }));
  process.exit(1);
});
