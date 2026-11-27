// Disposable native-Bao fixture only. Inputs arrive on stdin; no trace or screenshot capture.
import { chromium, devices, expect } from "@playwright/test";
import { readFileSync } from "node:fs";
const { origin, token, share, decision } = JSON.parse(readFileSync(0, "utf8"));
const browser = await chromium.launch({ executablePath: process.env.CHROMIUM_EXECUTABLE || undefined });
try {
  const page = await browser.newPage(devices["Pixel 7"]);
  await page.goto(new URL("/unseal", origin).href);
  await page.getByLabel("Custodian access token").fill("invalid");
  await page.getByRole("button", { name: "Review request" }).click();
  await expect(page.getByRole("status")).toHaveText("Unable to verify this custodian.");
  await page.getByLabel("Custodian access token").fill(token);
  await page.getByRole("button", { name: "Review request" }).click();
  await expect(page.getByText(/2 of 3 accepted/)).toBeVisible();
  const url = new URL("/unseal/decision", origin).href;
  const options = { headers: { Authorization: "Bearer " + token } };
  const status = await (await page.request.get(new URL("/unseal/status", origin).href, options)).json();
  const stale = await page.request.post(url, { ...options, data: { attempt: "different", decision: "decline" } });
  if (stale.status() !== 409) throw Error("Accepted an unrelated attempt");
  if (decision === "approve") {
    await page.getByLabel("Your unseal share").fill("wrong-disposable-test-share");
    await page.getByRole("button", { name: "Approve and contribute" }).click();
    await expect(page.getByRole("status")).toHaveText("invalid share");
    await page.getByLabel("Your unseal share").fill(share);
    await page.getByRole("button", { name: "Approve and contribute" }).click();
    await expect(page.getByRole("status")).toHaveText("accepted");
  } else {
    await page.getByRole("button", { name: "Decline" }).click();
    await expect(page.getByRole("status")).toHaveText("declined");
  }
  const duplicate = await page.request.post(url, { ...options, data: { attempt: status.attempt, decision: "decline" } });
  if (duplicate.status() !== 409) throw Error("Accepted a duplicate decision");
  if (await page.evaluate(() => document.documentElement.scrollWidth > innerWidth)) throw Error("Phone view overflows");
} finally { await browser.close(); }
