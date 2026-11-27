// Real Chromium rendering with an explicit in-memory service fixture. No live
// identities, production services, model calls or external ticket writes.
import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { pathToFileURL, fileURLToPath } from "node:url";
import { setTimeout as delay } from "node:timers/promises";

const packageName = process.env.CODIFIX_DESKTOP_PLAYWRIGHT_MODULE;
const { chromium } = await import(
  packageName ? pathToFileURL(packageName).href : "playwright"
);
const cwd = fileURLToPath(new URL("..", import.meta.url));
const port = process.env.CODIFIX_DESKTOP_REVIEW_PORT || "19351";
const server = spawn(
  process.execPath,
  [
    "node_modules/vite/bin/vite.js",
    "--host",
    "127.0.0.1",
    "--port",
    port,
    "--strictPort",
  ],
  { cwd, stdio: ["ignore", "pipe", "pipe"] },
);
const origin = `http://127.0.0.1:${port}`;
let browser;
try {
  let ready = false;
  for (let attempt = 0; attempt < 100; attempt++) {
    if (server.exitCode != null)
      throw new Error("Fixture server exited before becoming ready");
    try {
      ready = (await fetch(`${origin}/network-review.html`)).ok;
    } catch {
      /* starting */
    }
    if (ready) break;
    await delay(100);
  }
  assert(ready, "Fixture server ready");
  browser = await chromium.launch({
    executablePath:
      process.env.CODIFIX_DESKTOP_CHROME || "/usr/bin/google-chrome",
    headless: true,
  });
  const page = await browser.newPage({
    viewport: { width: 1440, height: 1100 },
  });
  const errors = [];
  page.on("pageerror", (error) => errors.push(error.message));
  await page.goto(`${origin}/network-review.html`);
  await page.getByRole("button", { name: "Connected", exact: true }).waitFor();
  await page.getByRole("button", { name: "Inbox", exact: true }).click();
  await page
    .getByRole("heading", { name: "Release roadmap", exact: true })
    .waitFor();
  await page
    .getByRole("textbox", { name: "Message", exact: true })
    .fill("Draft the roadmap.");
  await page.getByRole("button", { name: "Send message", exact: true }).click();
  await page.getByText("I will draft the roadmap", { exact: false }).waitFor();
  await page
    .getByRole("button", { name: "Add specialist", exact: true })
    .click();
  const dialog = page.getByRole("dialog");
  await dialog
    .getByRole("textbox", { name: "Summary to share" })
    .fill("Review documentation for the planned release.");
  const confirm = dialog.getByRole("button", {
    name: "Confirm context and add specialist",
  });
  assert(await confirm.isDisabled(), "Invitation needs disclosure review");
  await dialog
    .getByRole("checkbox", { name: /I reviewed this summary/ })
    .check();
  await confirm.click();
  await page
    .getByRole("checkbox", { name: "@Documentation specialist" })
    .waitFor();
  await page.getByRole("button", { name: "Cancel task", exact: true }).click();
  await page.getByText("Cancellation pending", { exact: true }).waitFor();
  if (process.env.CODIFIX_DESKTOP_REVIEW_SCREENSHOT)
    await page.screenshot({
      path: process.env.CODIFIX_DESKTOP_REVIEW_SCREENSHOT.replace(
        /\.png$/,
        "-inbox.png",
      ),
      fullPage: true,
    });
  await page.getByRole("button", { name: "Network", exact: true }).click();
  await page
    .getByRole("heading", { name: "Your AT identity", exact: true })
    .waitFor();
  assert((await page.getByText("Enrolled", { exact: true }).count()) === 2);
  assert.equal(errors.length, 0, errors.join("\n"));
  if (process.env.CODIFIX_DESKTOP_REVIEW_SCREENSHOT)
    await page.screenshot({
      path: process.env.CODIFIX_DESKTOP_REVIEW_SCREENSHOT,
      fullPage: true,
    });
  console.log(
    "Network/Inbox Chromium smoke passed: project-independent navigation, send/readback, reviewed invitation, pending cancellation, enrolled directory.",
  );
} finally {
  await browser?.close();
  server.kill("SIGTERM");
}
