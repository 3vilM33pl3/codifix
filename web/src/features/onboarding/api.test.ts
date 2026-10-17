import { afterEach, beforeEach, expect, it, vi } from "vitest";
beforeEach(() => { vi.resetModules(); document.cookie = "codifix_csrf=device-csrf; path=/"; });
afterEach(() => { vi.unstubAllGlobals(); document.cookie = "codifix_csrf=; max-age=0; path=/"; });

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

it("company requests stay on company-only routes with the exact immutable offline body and CSRF", async () => {
  const fetcher = vi.fn().mockImplementation(async (url: string) => new Response(JSON.stringify(url === "/v1/auth/me"
    ? { mode: "multi_user", read_only: false, principal: { id: "human" } } : { status: "pending" }), { status: 200 }));
  vi.stubGlobal("fetch", fetcher);
  const { companyDeviceReview, approveCompanyDevice } = await import("../../api");
  await companyDeviceReview("ABCD-1234");
  const request = { user_code: "ABCD-1234", confirmed: true as const, projects: [{ area: "workshop" as const, owner_instance_id: "owner", project_id: "project", manifest_revision: 2, manifest_sha256: "a".repeat(64), offline_confirmed: true as const }] };
  await approveCompanyDevice(request);
  const [url, init] = fetcher.mock.calls.at(-1)! as unknown as [string, RequestInit];
  expect(url).toBe("/v1/onboarding/company/device/approve"); expect(init.body).toBe(JSON.stringify(request));
  expect(init.redirect).toBe("error"); expect(init.credentials).toBe("same-origin"); expect(init.cache).toBe("no-store");
  expect(new Headers(init.headers).get("x-csrf-token")).toBe("device-csrf");
  expect(fetcher.mock.calls.some(call => /^\/v1\/onboarding\/device\//.test(String(call[0])))).toBe(false);
});
it.each([403, 404, 409, 410, 503])("company %s errors suppress private bodies and never fallback or replace the request", async status => {
  const fetcher = vi.fn().mockImplementation(async (url: string) => url === "/v1/auth/me"
    ? new Response(JSON.stringify({ mode: "multi_user", read_only: false, principal: { id: "human" } }), { status: 200 })
    : new Response("PRIVATE_OWNER_RESPONSE_BODY", { status }));
  vi.stubGlobal("fetch", fetcher); const { companyDeviceReview } = await import("../../api");
  await expect(companyDeviceReview("ABCD-1234")).rejects.not.toThrow("PRIVATE_OWNER_RESPONSE_BODY");
  expect(fetcher.mock.calls).toHaveLength(2);
  expect(fetcher.mock.calls[1][0]).toBe("/v1/onboarding/company/device/review?code=ABCD-1234");
});
