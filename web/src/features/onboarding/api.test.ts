import { afterEach, beforeEach, expect, it, vi } from "vitest";
beforeEach(() => { vi.resetModules(); document.cookie = "memory_csrf=device-csrf; path=/"; });
afterEach(() => { vi.unstubAllGlobals(); document.cookie = "memory_csrf=; max-age=0; path=/"; });

it("sends only a public code in review and uses CSRF for exact revision-scoped approvals", async () => {
  const fetcher = vi.fn().mockImplementation(async (url: string) => new Response(JSON.stringify(url === "/v1/auth/me"
    ? { mode: "multi_user", read_only: false, principal: { id: "human" } } : { status: "approved" }), { status: 200 }));
  vi.stubGlobal("fetch", fetcher);
  const { deviceReview, approveDevice } = await import("../../api");
  await deviceReview("ABCD-1234");
  const request = { user_code: "ABCD-1234", projects: [{ project_id: "project-id", role: "reader" as const, revision: 3 }], confirmed: true as const };
  await approveDevice(request);
  expect(fetcher).toHaveBeenCalledWith("/v1/onboarding/device/review?code=ABCD-1234", expect.objectContaining({ cache: "no-store", credentials: "same-origin" }));
  const approval = fetcher.mock.calls.find((call) => call[0] === "/v1/onboarding/device/approve")![1] as RequestInit;
  expect(approval.method).toBe("POST"); expect(approval.body).toBe(JSON.stringify(request));
  expect(new Headers(approval.headers).get("x-csrf-token")).toBe("device-csrf");
});

it("shows current membership, expiry and revision failures without raw response bodies", async () => {
  let status = 403;
  vi.stubGlobal("fetch", vi.fn().mockImplementation(async (url: string) => url === "/v1/auth/me"
    ? new Response(JSON.stringify({ mode: "multi_user", read_only: false, principal: { id: "human" } }), { status: 200 })
    : new Response("private auth details", { status })));
  const { deviceReview } = await import("../../api");
  await expect(deviceReview("ABCD-1234")).rejects.toThrow("project membership may have changed");
  status = 410;
  await expect(deviceReview("ABCD-1234")).rejects.toThrow("expired");
  status = 409;
  await expect(deviceReview("ABCD-1234")).rejects.toThrow("manifest revision changed");
});
