import { afterEach, beforeEach, expect, it, vi } from "vitest";

const reader = { authenticated: true, mode: "single_user", read_only: false,
  principal: { kind: "internal", global_role: "reader", global_permissions: ["memory_read"] } };
const admin = { ...reader, principal: { ...reader.principal, global_role: "admin", global_permissions: ["system_admin"] } };
beforeEach(() => vi.resetModules());
afterEach(() => vi.unstubAllGlobals());

it("refreshes cached reader permissions after explicit cookie-based login", async () => {
  const fetcher = vi.fn().mockResolvedValueOnce(Response.json(reader))
    .mockResolvedValueOnce(Response.json({ authenticated: true }))
    .mockResolvedValueOnce(Response.json(admin));
  vi.stubGlobal("fetch", fetcher);
  const { getAuthMe, openLocalAdministration } = await import("../../api");
  await getAuthMe();
  await expect(openLocalAdministration()).resolves.toEqual(admin);
  expect(fetcher).toHaveBeenCalledTimes(3);
  expect(fetcher).toHaveBeenNthCalledWith(2, "/v1/auth/session/bootstrap", {
    method: "POST", credentials: "same-origin", cache: "no-store",
  });
  await expect(getAuthMe()).resolves.toEqual(admin);
  expect(fetcher).toHaveBeenCalledTimes(3);
});

it.each([
  { ...reader, mode: "multi_user" }, { ...reader, read_only: true },
  { ...reader, principal: { ...reader.principal, kind: "service_token" } },
])("never bootstraps a disallowed mode or identity", async (identity) => {
  const fetcher = vi.fn().mockResolvedValue(Response.json(identity));
  vi.stubGlobal("fetch", fetcher);
  const { openLocalAdministration } = await import("../../api");
  await expect(openLocalAdministration()).rejects.toThrow("unavailable");
  expect(fetcher).toHaveBeenCalledTimes(1);
});

it("does not infer administration from role when explicit permissions deny it", async () => {
  vi.stubGlobal("fetch", vi.fn().mockResolvedValueOnce(Response.json(reader))
    .mockResolvedValueOnce(Response.json({ authenticated: true }))
    .mockResolvedValueOnce(Response.json({ ...admin, principal: { ...admin.principal, global_permissions: [] } })));
  const { openLocalAdministration } = await import("../../api");
  await expect(openLocalAdministration()).rejects.toThrow("did not authorize");
});

it("does not reveal bootstrap failure response bodies", async () => {
  vi.stubGlobal("fetch", vi.fn().mockResolvedValueOnce(Response.json(reader))
    .mockResolvedValueOnce(new Response("private detail", { status: 403 })));
  const { openLocalAdministration } = await import("../../api");
  await expect(openLocalAdministration()).rejects.toThrow("login failed");
});
