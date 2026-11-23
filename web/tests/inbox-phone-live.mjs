// Run by the service's phone_browser_inbox_replies_and_clarifies test against its isolated database.
import { chromium, devices, expect } from "@playwright/test";
import { readFileSync } from "node:fs";

const { origin, session, csrf, conversation } = JSON.parse(readFileSync(0, "utf8"));
const browser = await chromium.launch({ executablePath: process.env.CHROMIUM_EXECUTABLE || undefined });
try {
  const context = await browser.newContext(devices["Pixel 7"]);
  const page = await context.newPage();
  page.setDefaultTimeout(5000);
  const direct = `${origin}/?inbox=1&conversation=${conversation}`;
  await page.goto(direct);
  const signIn = page.getByRole("link", { name: "Sign in" });
  await expect(signIn).toBeVisible();
  const login = new URL(await signIn.getAttribute("href"), origin);
  if (login.searchParams.get("return_to") !== ("/" + new URL(direct).search)) throw Error("Sign-in lost the selected conversation");
  await context.addCookies([
    { name: "codifix_session", value: session, url: origin, httpOnly: true },
    { name: "codifix_csrf", value: csrf, url: origin },
  ]);
  await page.goto(`${origin}/?inbox=1`);
  await page.getByRole("link", { name: /Phone Inbox rehearsal/ }).click();
  await expect(page.getByText(/Release review started\./)).toBeVisible();
  await expect(page.getByText("Which build should I check?", { exact: true }).first()).toBeVisible();
  await page.getByLabel("Answer clarification").fill("Use the ARM64 build");
  await page.getByRole("button", { name: "Send answer" }).click();
  await expect(page.getByText("Use the ARM64 build", { exact: true })).toBeVisible();
  await page.getByLabel("Reply to conversation").fill("Send the result here");
  await page.getByRole("button", { name: "Send reply" }).click();
  await expect(page.getByText("Send the result here", { exact: true })).toBeVisible();
  await page.getByRole("button", { name: "Refresh" }).click();
  await expect(page.getByText("continuing", { exact: true })).toBeVisible();
  if (await page.evaluate(() => document.documentElement.scrollWidth > window.innerWidth)) throw Error("Horizontal overflow at phone width");
} finally {
  await browser.close();
}
