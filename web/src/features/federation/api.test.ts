import { afterEach, beforeEach, expect, it, vi } from "vitest";

const discovery = "/v1/projects/homelab/federation";
const bootstrap = "/v1/auth/session/bootstrap";
const csrf = "synthetic-federation-csrf";

function clearCookie() {
  document.cookie = "codifix_csrf=; max-age=0; path=/";
}

beforeEach(() => { vi.resetModules(); clearCookie(); });
afterEach(() => { vi.unstubAllGlobals(); clearCookie(); });

it("bootstraps a fresh single-user session before the first federation read", async () => {
  const fetch = vi.fn(async (input: RequestInfo | URL) => {
    if (input === "/v1/auth/me") return Response.json({ mode: "single_user", read_only: false });
    if (input === bootstrap) {
      document.cookie = `codifix_csrf=${csrf}; path=/`;
      return Response.json({ authenticated: true });
    }
    expect(input).toBe(discovery);
    expect(document.cookie).toContain(csrf);
    return Response.json({ space: "company" });
  });
  vi.stubGlobal("fetch", fetch);
  const { federationRequest } = await import("../../api");
  const controller = new AbortController();
  await expect(federationRequest("homelab", null, "", undefined, controller.signal))
    .resolves.toEqual({ space: "company" });
  expect(fetch.mock.calls.map(([url]) => url)).toEqual(["/v1/auth/me", bootstrap, discovery]);
  expect(fetch).toHaveBeenNthCalledWith(2, bootstrap, { method: "POST", credentials: "same-origin" });
  expect(fetch).toHaveBeenNthCalledWith(3, discovery, expect.objectContaining({
    method: "GET", credentials: "same-origin", cache: "no-store", signal: controller.signal,
  }));
});

it("shares one bootstrap across concurrent federation reads and waits for its completion", async () => {
  let finishBootstrap!: (response: Response) => void;
  const pending = new Promise<Response>(resolve => { finishBootstrap = resolve; });
  const fetch = vi.fn(async (input: RequestInfo | URL) => {
    if (input === "/v1/auth/me") return Response.json({ mode: "single_user", read_only: false });
    if (input === bootstrap) return pending;
    return Response.json({ space: "company" });
  });
  vi.stubGlobal("fetch", fetch);
  const { federationRequest } = await import("../../api");
  const reads = Promise.all([federationRequest("homelab", null), federationRequest("memory", null)]);
  await vi.waitFor(() => expect(fetch.mock.calls.filter(([url]) => url === bootstrap)).toHaveLength(1));
  expect(fetch.mock.calls.some(([url]) => String(url).includes("/federation"))).toBe(false);
  document.cookie = `codifix_csrf=${csrf}; path=/`;
  finishBootstrap(Response.json({ authenticated: true }));
  await reads;
  expect(fetch.mock.calls.filter(([url]) => url === bootstrap)).toHaveLength(1);
  expect(fetch.mock.calls.filter(([url]) => String(url).includes("/federation"))).toHaveLength(2);
});

it("keeps session cookies and CSRF on confirmed federation mutations", async () => {
  const fetch = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
    if (input === "/v1/auth/me") return Response.json({ mode: "single_user", read_only: false });
    if (input === bootstrap) {
      document.cookie = `codifix_csrf=${csrf}; path=/`;
      return Response.json({ authenticated: true });
    }
    expect(input).toBe(`${discovery}/company/questions/submit`);
    expect(init?.method).toBe("POST");
    expect(init?.credentials).toBe("same-origin");
    expect(new Headers(init?.headers).get("x-csrf-token")).toBe(csrf);
    expect(JSON.parse(String(init?.body))).toEqual({ request_id: "synthetic-request" });
    return Response.json({ state: "queued" });
  });
  vi.stubGlobal("fetch", fetch);
  const { federationRequest } = await import("../../api");
  await federationRequest("homelab", "company", "/questions/submit", { request_id: "synthetic-request" });
  expect(fetch.mock.calls.filter(([url]) => url === bootstrap)).toHaveLength(1);
});

it.each([false, true])("never bootstraps or starts login for OIDC with a CSRF cookie: %s", async hasCookie => {
  if (hasCookie) document.cookie = `codifix_csrf=${csrf}; path=/`;
  const fetch = vi.fn(async (input: RequestInfo | URL) => input === "/v1/auth/me"
    ? Response.json({ mode: "multi_user", read_only: false }) : Response.json({}));
  vi.stubGlobal("fetch", fetch);
  const { federationRequest } = await import("../../api");
  await federationRequest("homelab", null);
  await federationRequest("homelab", "company", "/actions", { request_id: "synthetic-request" });
  expect(fetch.mock.calls.map(([url]) => url)).toEqual([
    "/v1/auth/me", discovery, `${discovery}/company/actions`,
  ]);
});

it.each([401, 403])("does not replace an existing session or replay after federation denial %s", async status => {
  document.cookie = `codifix_csrf=${csrf}; path=/`;
  const fetch = vi.fn(async (input: RequestInfo | URL) => input === "/v1/auth/me"
    ? Response.json({ mode: "single_user", read_only: false }) : new Response("Denied", { status }));
  vi.stubGlobal("fetch", fetch);
  const { federationRequest } = await import("../../api");
  await expect(federationRequest("homelab", null)).rejects.toThrow(String(status));
  expect(fetch.mock.calls.map(([url]) => url)).toEqual(["/v1/auth/me", discovery]);
  await expect(federationRequest("homelab", null)).rejects.toThrow(String(status));
  expect(fetch.mock.calls.map(([url]) => url)).toEqual(["/v1/auth/me", discovery, discovery]);
  expect(document.cookie).toContain(csrf);
});

it("fails closed when the server rejects a stale HttpOnly session before discovery", async () => {
  // The HttpOnly session is deliberately inaccessible to JavaScript, including
  // when its paired readable CSRF cookie is absent.
  const fetch = vi.fn().mockResolvedValue(new Response("Signed out", { status: 401 }));
  vi.stubGlobal("fetch", fetch);
  const { federationRequest } = await import("../../api");
  await expect(federationRequest("homelab", null)).rejects.toThrow("401");
  expect(fetch).toHaveBeenCalledExactlyOnceWith("/v1/auth/me", { credentials: "same-origin" });
});

it("does not remint an already bootstrapped session after cookie loss and a denial", async () => {
  let denied = false;
  const fetch = vi.fn(async (input: RequestInfo | URL) => {
    if (input === "/v1/auth/me") return Response.json({ mode: "single_user", read_only: false });
    if (input === bootstrap) {
      document.cookie = `codifix_csrf=${csrf}; path=/`;
      return Response.json({ authenticated: true });
    }
    return denied ? new Response("Signed out", { status: 401 }) : Response.json({});
  });
  vi.stubGlobal("fetch", fetch);
  const { federationRequest } = await import("../../api");
  await federationRequest("homelab", null);
  denied = true;
  clearCookie();
  await expect(federationRequest("homelab", null)).rejects.toThrow("401");
  await expect(federationRequest("homelab", null)).rejects.toThrow("401");
  expect(fetch.mock.calls.filter(([url]) => url === bootstrap)).toHaveLength(1);
});

it("does not request federation data when local bootstrap is denied", async () => {
  const fetch = vi.fn(async (input: RequestInfo | URL) => input === "/v1/auth/me"
    ? Response.json({ mode: "single_user", read_only: false }) : new Response("Denied", { status: 403 }));
  vi.stubGlobal("fetch", fetch);
  const { federationRequest } = await import("../../api");
  await expect(federationRequest("homelab", null)).rejects.toThrow("session bootstrap failed: 403");
  expect(fetch.mock.calls.map(([url]) => url)).toEqual(["/v1/auth/me", bootstrap]);
});
