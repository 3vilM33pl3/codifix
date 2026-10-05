import { afterEach, beforeEach, expect, it, vi } from "vitest";

beforeEach(() => { vi.resetModules(); document.cookie = "memory_csrf=onboarding-csrf; path=/"; });
afterEach(() => { vi.unstubAllGlobals(); document.cookie = "memory_csrf=; max-age=0; path=/"; });

it("uses the central session and CSRF boundary for company mutations", async () => {
  const fetcher = vi.fn().mockImplementation(async (url: string) => new Response(JSON.stringify(url === "/v1/auth/me"
    ? { mode: "multi_user", read_only: false, principal: { id: "admin" } }
    : { id: "device-id", revoked_at: "2026-10-05T01:00:00Z" }), { status: 200 }));
  vi.stubGlobal("fetch", fetcher);
  const { onboardingRequest } = await import("../../api");
  await onboardingRequest("/devices/device-id/revoke", {});
  expect(fetcher).toHaveBeenCalledWith("/v1/admin/onboarding/devices/device-id/revoke", expect.objectContaining({
    method: "POST", credentials: "same-origin", cache: "no-store", body: "{}",
  }));
  const init = fetcher.mock.calls.find((call) => call[0].endsWith("/revoke"))![1] as RequestInit;
  expect(new Headers(init.headers).get("x-csrf-token")).toBe("onboarding-csrf");
});

it("reports denial and conflict without rendering provider response details", async () => {
  let code = 403;
  vi.stubGlobal("fetch", vi.fn().mockImplementation(async (url: string) => url === "/v1/auth/me"
    ? new Response(JSON.stringify({ mode: "multi_user", read_only: false, principal: { id: "admin" } }), { status: 200 })
    : new Response("sensitive provider response", { status: code })));
  const { onboardingRequest } = await import("../../api");
  await expect(onboardingRequest("/users", {})).rejects.toThrow("Access denied");
  code = 409;
  await expect(onboardingRequest("/projects/project-id", {})).rejects.toThrow("Conflict. Refresh and review");
  code = 502;
  await expect(onboardingRequest("/users/invite", {})).rejects.toThrow("Administration request failed (502)");
});

it("extracts only a strict sanitized provider operation UUID on lost 503 confirmation", async () => {
  const operationId = "11111111-1111-4111-8111-111111111111";
  let message = `external effect may have occurred; inspect provider operation ${operationId}; outcome could not be recorded`;
  vi.stubGlobal("fetch", vi.fn().mockImplementation(async (url: string) => url === "/v1/auth/me"
    ? new Response(JSON.stringify({ mode: "multi_user", read_only: false, principal: { id: "admin" } }), { status: 200 })
    : new Response(JSON.stringify({ error: message, diagnostic: { raw_error: "private provider detail" } }), { status: 503 })));
  const { onboardingRequest } = await import("../../api");
  await expect(onboardingRequest("/users/invite", {})).rejects.toMatchObject({ operationId, message: expect.stringContaining(operationId) });
  message = "external effect may have occurred; inspect provider operation invalid-id; outcome could not be recorded private provider detail";
  await expect(onboardingRequest("/users/invite", {})).rejects.toMatchObject({ operationId: null });
  try { await onboardingRequest("/users/invite", {}); } catch (error) {
    expect((error as Error).message).toContain("duplicate an external effect");
    expect((error as Error).message).not.toContain("invalid-id");
    expect((error as Error).message).not.toContain("private provider detail");
  }
});
